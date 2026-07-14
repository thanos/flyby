//! Memory-mapped region: the container for the SPSC ring and its slots.
//!
//! A [`Region`] owns a contiguous anonymous memory-mapped allocation with
//! the following layout:
//!
//! ```text
//! Offset   Size  Contents
//! ──────────────────────────────────────────────────────
//!      0     64  RegionHeader  (magic, version, geometry)
//!     64     64  Producer control  (head AtomicU64 + padding)
//!    128     64  Consumer control  (tail AtomicU64 + padding)
//!    192      …  Slot[0] .. Slot[slot_count - 1]
//! ```
//!
//! Each control block occupies a full cache line to prevent false sharing
//! between the producer and consumer counters.
//!
//! ## Usage model
//!
//! `Region` is a single-owner object. Push and pop both take `&mut self`,
//! which enforces at the type level that only one operation runs at a
//! time in a single-process context.
//!
//! For true inter-process or inter-thread use — where the producer and
//! consumer live in separate contexts — a future revision will expose a
//! file-backed region and a `split()` method that yields typed
//! `Producer` and `Consumer` handles (Phase 2).

use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;
use flyby_core::{Error, Result};

use crate::ring::SpscRing;
use crate::slot;

/// Magic number identifying a valid FlyBy region header (version 1).
pub const REGION_MAGIC: u64 = 0x464C_5942_5F52_4701; // "FLYB_RG\x01"

/// Layout version stored in the region header.
pub const REGION_VERSION: u16 = 1;

// Fixed byte offsets of each section within the mmap.
const HEADER_OFFSET: usize = 0;
const HEADER_BYTES: usize = 64;
const PRODUCER_OFFSET: usize = 64;  // head counter — one full cache line
const CONSUMER_OFFSET: usize = 128; // tail counter — one full cache line
/// Byte offset at which slot[0] begins.
pub const SLOTS_OFFSET: usize = 192;

/// On-disk region header stored at offset 0 of the mmap.
///
/// `repr(C)` guarantees a stable, portable layout. The size is exactly
/// [`HEADER_BYTES`] bytes, verified by a compile-time assertion.
#[repr(C)]
struct RegionHeader {
    magic:      u64,       // 8  — REGION_MAGIC
    version:    u16,       // 2  — REGION_VERSION
    flags:      u16,       // 2  — feature flags (reserved, must be 0)
    _pad0:      u32,       // 4  — align slot_count to 8
    slot_count: u32,       // 4
    slot_size:  u32,       // 4  — bytes per slot including header + padding
    _pad1:      [u8; 8],   // 8  — align region_id to 16
    region_id:  u128,      // 16 — unique identifier (zero in v0.1)
    _pad2:      [u8; 16],  // 16 — pad to 64 bytes
}

const _: () = assert!(
    core::mem::size_of::<RegionHeader>() == HEADER_BYTES,
    "RegionHeader must be exactly 64 bytes"
);

/// A memory-mapped SPSC ring of fixed-size slots.
///
/// Owns the mmap allocation and unmaps it when dropped.
pub struct Region {
    /// Start of the mmap. Always non-null and page-aligned.
    ptr: NonNull<u8>,
    /// Total byte length of the mmap.
    len: usize,
    /// Number of slots in the ring (power of two).
    slot_count: usize,
    /// Bytes per slot (header + payload area + alignment padding).
    slot_size: usize,
    /// SPSC ring control, backed by atomics at [`PRODUCER_OFFSET`] and
    /// [`CONSUMER_OFFSET`] within the mmap.
    ring: SpscRing,
}

// SAFETY: `Region` owns a unique mmap allocation. Sending it to another
// thread transfers sole ownership of that allocation.
unsafe impl Send for Region {}

// SAFETY: All methods that dereference the raw mmap pointer
// (`slot_mut`, `slot_ref`, `push`, `pop`) require `&mut self`, so no
// two threads can reach the raw memory through a shared reference
// simultaneously. The only `&self` methods (`len`, `is_empty`,
// `is_full`) access only the `AtomicU64` counters in the ring, which
// are inherently thread-safe. Therefore sharing a `&Region` between
// threads is safe.
unsafe impl Sync for Region {}

impl Region {
    /// Create an anonymous (single-process) mmap'd region.
    ///
    /// # Parameters
    ///
    /// - `slot_count` — number of ring slots. Must be a non-zero power of two.
    /// - `slot_size` — total bytes per slot (use [`slot::slot_size`] to
    ///   compute). Must be a multiple of [`slot::CACHE_LINE`] and at
    ///   least [`slot::HEADER_SIZE`].
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::Config`] for invalid parameters or
    /// [`ErrorKind::Io`] if `mmap(2)` fails.
    pub fn anonymous(slot_count: usize, slot_size: usize) -> Result<Self> {
        Self::validate_params(slot_count, slot_size)?;
        let len = SLOTS_OFFSET + slot_count * slot_size;

        // SAFETY: We request a fresh anonymous private mapping. The OS
        // returns a page-zeroed allocation or MAP_FAILED. We validate
        // the return value before constructing a NonNull pointer.
        //
        // Invariants established here:
        // - `ptr` is non-null and valid for `len` bytes on success.
        // - The allocation is zero-filled, which is the correct initial
        //   state for both the AtomicU64 counters (== 0) and slot magic
        //   fields (== 0, invalid until a producer writes them).
        let raw = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if raw == libc::MAP_FAILED {
            return Err(Error::from(std::io::Error::last_os_error()));
        }

        // SAFETY: mmap succeeded; raw is non-null and aligned.
        let ptr = unsafe { NonNull::new_unchecked(raw as *mut u8) };

        // SAFETY: ptr is valid for HEADER_BYTES bytes at offset 0.
        // RegionHeader is repr(C) and we write a fully-initialised value.
        unsafe {
            let header = RegionHeader {
                magic: REGION_MAGIC,
                version: REGION_VERSION,
                flags: 0,
                _pad0: 0,
                slot_count: slot_count as u32,
                slot_size: slot_size as u32,
                _pad1: [0; 8],
                region_id: 0,
                _pad2: [0; 16],
            };
            core::ptr::write(
                ptr.as_ptr().add(HEADER_OFFSET) as *mut RegionHeader,
                header,
            );
        }

        // SAFETY:
        // - `head` and `tail` are within the mmap at their respective
        //   offsets, both properly aligned to 8 bytes (AtomicU64 alignment).
        // - The OS zero-filled the allocation, so both counters start at 0.
        // - The mmap lives as long as `Region` (dropped in `Drop`), which
        //   is at least as long as `ring`.
        let ring = unsafe {
            let head = NonNull::new_unchecked(
                ptr.as_ptr().add(PRODUCER_OFFSET) as *mut AtomicU64,
            );
            let tail = NonNull::new_unchecked(
                ptr.as_ptr().add(CONSUMER_OFFSET) as *mut AtomicU64,
            );
            SpscRing::new(head, tail, slot_count)
        };

        Ok(Self { ptr, len, slot_count, slot_size, ring })
    }

    fn validate_params(slot_count: usize, slot_size: usize) -> Result<()> {
        if slot_count == 0 || !slot_count.is_power_of_two() {
            return Err(Error::config("slot_count must be a non-zero power of two"));
        }
        if slot_size < slot::HEADER_SIZE {
            return Err(Error::config("slot_size must be at least HEADER_SIZE (32)"));
        }
        if slot_size % slot::CACHE_LINE != 0 {
            return Err(Error::config(
                "slot_size must be a multiple of CACHE_LINE (64)",
            ));
        }
        // Guard against overflow in total mmap size calculation.
        slot_count
            .checked_mul(slot_size)
            .and_then(|n| n.checked_add(SLOTS_OFFSET))
            .ok_or_else(|| Error::config("region size overflows usize"))?;
        Ok(())
    }

    /// Returns a mutable byte slice for the slot at `index`.
    ///
    /// # Safety
    ///
    /// `index` must be in `[0, slot_count)`. This is guaranteed by the
    /// ring logic which masks indices by `capacity - 1`.
    fn slot_mut(&mut self, index: usize) -> &mut [u8] {
        let offset = SLOTS_OFFSET + index * self.slot_size;
        // SAFETY: offset is within [SLOTS_OFFSET, SLOTS_OFFSET + slot_count*slot_size).
        // The mmap is valid for `self.len` bytes. `&mut self` ensures
        // no other reference to this memory exists simultaneously.
        unsafe {
            core::slice::from_raw_parts_mut(self.ptr.as_ptr().add(offset), self.slot_size)
        }
    }

    /// Returns an immutable byte slice for the slot at `index`.
    fn slot_ref(&self, index: usize) -> &[u8] {
        let offset = SLOTS_OFFSET + index * self.slot_size;
        // SAFETY: same bounds reasoning as slot_mut. `&self` ensures
        // no mutable reference to this memory exists simultaneously.
        unsafe {
            core::slice::from_raw_parts(self.ptr.as_ptr().add(offset), self.slot_size)
        }
    }

    /// Write one slot into the ring.
    ///
    /// Calls `fill(buf)` with a mutable view of the claimed slot. If the
    /// fill closure succeeds, the slot is committed and visible to the
    /// consumer. Returns `Ok(true)` on success, `Ok(false)` if the ring
    /// is full.
    ///
    /// Must be called from the single producer.
    pub fn push(&mut self, fill: impl FnOnce(&mut [u8]) -> Result<()>) -> Result<bool> {
        match self.ring.try_push() {
            None => Ok(false),
            Some(idx) => {
                let buf = self.slot_mut(idx);
                fill(buf)?;
                self.ring.commit_push();
                Ok(true)
            }
        }
    }

    /// Read one slot from the ring.
    ///
    /// Calls `consume(buf)` with an immutable view of the next slot. The
    /// slot is released after `consume` returns. Returns `true` if a slot
    /// was available, `false` if the ring was empty.
    ///
    /// Must be called from the single consumer.
    pub fn pop(&mut self, consume: impl FnOnce(&[u8])) -> bool {
        match self.ring.try_pop() {
            None => false,
            Some(idx) => {
                let buf = self.slot_ref(idx);
                consume(buf);
                self.ring.commit_pop();
                true
            }
        }
    }

    /// Number of slots in the ring.
    pub fn slot_count(&self) -> usize {
        self.slot_count
    }

    /// Bytes per slot (header + payload area + padding).
    pub fn slot_size(&self) -> usize {
        self.slot_size
    }

    /// Number of occupied slots (approximate outside a single thread).
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// `true` if no slots are currently occupied.
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// `true` if all slots are occupied and the next push will fail.
    pub fn is_full(&self) -> bool {
        self.ring.is_full()
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        // SAFETY: `ptr` and `len` were set from a successful mmap call
        // in `anonymous`. This is the only place the mapping is freed.
        // Calling munmap twice is avoided by the single-owner semantics
        // of `Region` (no Clone, no copy).
        unsafe {
            libc::munmap(self.ptr.as_ptr() as *mut libc::c_void, self.len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot::{self as s, SlotHeader, FLAG_VALID, HEADER_SIZE};

    fn default_region() -> Region {
        Region::anonymous(8, s::slot_size(64)).unwrap()
    }

    fn fill_slot(seq: u64) -> impl FnOnce(&mut [u8]) -> Result<()> {
        move |buf| {
            let header = SlotHeader::new(1, FLAG_VALID, seq, 0, 4);
            s::encode(&header, b"data", buf)
        }
    }

    #[test]
    fn create_and_drop() {
        let region = default_region();
        assert_eq!(region.slot_count(), 8);
        assert!(region.is_empty());
    }

    #[test]
    fn invalid_slot_count_rejected() {
        assert!(Region::anonymous(0, 64).is_err());
        assert!(Region::anonymous(3, 64).is_err()); // not a power of two
    }

    #[test]
    fn invalid_slot_size_rejected() {
        assert!(Region::anonymous(4, HEADER_SIZE - 1).is_err());
        assert!(Region::anonymous(4, 48).is_err()); // 48 is not a multiple of 64
    }

    #[test]
    fn push_pop_roundtrip() {
        let mut region = default_region();
        let pushed = region.push(fill_slot(99)).unwrap();
        assert!(pushed);
        assert_eq!(region.len(), 1);

        let mut seq = 0u64;
        let popped = region.pop(|buf| {
            let (hdr, _payload) = s::decode(buf).unwrap();
            seq = hdr.sequence;
        });
        assert!(popped);
        assert_eq!(seq, 99);
        assert!(region.is_empty());
    }

    #[test]
    fn full_ring_push_returns_false() {
        let mut region = default_region();
        for i in 0..8 {
            assert!(region.push(fill_slot(i)).unwrap());
        }
        assert!(region.is_full());
        assert!(!region.push(fill_slot(99)).unwrap());
    }

    #[test]
    fn empty_ring_pop_returns_false() {
        let mut region = default_region();
        assert!(!region.pop(|_| {}));
    }

    #[test]
    fn wrap_around_preserves_order() {
        let mut region = Region::anonymous(4, s::slot_size(64)).unwrap();
        let mut results = Vec::new();

        for round in 0..3u64 {
            // push 4
            for i in 0..4u64 {
                region.push(fill_slot(round * 4 + i)).unwrap();
            }
            // pop 4
            for _ in 0..4 {
                region.pop(|buf| {
                    let (hdr, _) = s::decode(buf).unwrap();
                    results.push(hdr.sequence);
                });
            }
        }

        let expected: Vec<u64> = (0..12).collect();
        assert_eq!(results, expected);
    }

    #[test]
    fn mmap_lifecycle() {
        // create, use, and drop — valgrind / miri would catch any UAF
        {
            let mut region = Region::anonymous(4, 64).unwrap();
            region.push(fill_slot(1)).unwrap();
        } // munmap called in Drop
    }
}
