//! Slot layout: the unit of storage inside a [`crate::region::Region`].
//!
//! Every slot consists of a fixed-size [`SlotHeader`] followed by a
//! variable-length payload area and alignment padding:
//!
//! ```text
//! +-------------------------------+  ← slot start (cache-line aligned)
//! | SlotHeader  (32 bytes)        |
//! +-------------------------------+
//! | Payload     (payload_len ≤ N) |
//! +-------------------------------+
//! | Padding     (to cache line)   |
//! +-------------------------------+  ← next slot start
//! ```
//!
//! ## Alignment
//!
//! Slots are always a multiple of [`CACHE_LINE`] bytes so that each slot
//! starts on a cache-line boundary with no producer/consumer false sharing.
//!
//! ## Corruption detection
//!
//! The `magic` field in the header lets readers detect uninitialised or
//! corrupted slots before attempting to decode the payload.

use flyby_core::{Error, ErrorKind, Result};

/// Magic value identifying a valid FlyBy slot (version 1).
pub const SLOT_MAGIC: u32 = 0xFB57_0001;

/// Size of one CPU cache line in bytes.
pub const CACHE_LINE: usize = 64;

/// Byte size of [`SlotHeader`]. Must divide [`CACHE_LINE`] evenly.
pub const HEADER_SIZE: usize = 32;

/// Slot flag: the slot contains a valid, fully-written message.
pub const FLAG_VALID: u16 = 0x0001;

/// Slot flag: the source marked this message as suspect.
pub const FLAG_SUSPECT: u16 = 0x0002;

/// Compute the total slot size (header + payload area + padding) for a
/// given maximum payload length, rounded up to the next cache-line
/// multiple.
///
/// # Panics
///
/// Panics in debug builds if the result would overflow `usize`. In
/// practice the inputs are always small.
pub const fn slot_size(max_payload: usize) -> usize {
    let raw = HEADER_SIZE + max_payload;
    // round up to the next multiple of CACHE_LINE
    (raw + CACHE_LINE - 1) & !(CACHE_LINE - 1)
}

/// The on-wire header stored at the start of every slot.
///
/// `repr(C)` guarantees a stable, predictable field layout. The total
/// size is exactly [`HEADER_SIZE`] bytes (verified by a compile-time
/// assertion below).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotHeader {
    /// Integrity marker. Must equal [`SLOT_MAGIC`] for a valid slot.
    pub magic: u32,
    /// Numeric schema identifier from [`flyby_core::SchemaId::id`].
    pub schema_id: u16,
    /// Bitmask of [`FLAG_VALID`], [`FLAG_SUSPECT`], etc.
    pub flags: u16,
    /// Monotonic sequence number assigned by the producer.
    pub sequence: u64,
    /// Nanoseconds since the UNIX epoch.
    pub timestamp: u64,
    /// Byte length of the payload following this header.
    pub payload_len: u32,
    pub(crate) _reserved: u32,
}

const _: () = assert!(
    core::mem::size_of::<SlotHeader>() == HEADER_SIZE,
    "SlotHeader must be exactly 32 bytes"
);

impl SlotHeader {
    /// Construct a valid slot header.
    pub fn new(
        schema_id: u16,
        flags: u16,
        sequence: u64,
        timestamp: u64,
        payload_len: u32,
    ) -> Self {
        Self {
            magic: SLOT_MAGIC,
            schema_id,
            flags,
            sequence,
            timestamp,
            payload_len,
            _reserved: 0,
        }
    }

    /// Returns `true` if the magic is correct and `FLAG_VALID` is set.
    pub fn is_valid(&self) -> bool {
        self.magic == SLOT_MAGIC && (self.flags & FLAG_VALID) != 0
    }
}

/// Encode a slot into `dst`.
///
/// Writes `header` followed immediately by `payload`. `dst` must be at
/// least `HEADER_SIZE + payload.len()` bytes; returns an error otherwise.
///
/// # Safety
///
/// The function contains one unsafe block that reads `header` as raw
/// bytes via `copy_nonoverlapping`. This is sound because:
///
/// - `SlotHeader` is `repr(C)` with no padding bytes (verified by the
///   size assertion above).
/// - We copy exactly `HEADER_SIZE` bytes from a valid, stack-allocated
///   value — no UB from reading uninitialised memory.
/// - Destination alignment is not required by `copy_nonoverlapping`.
pub fn encode(header: &SlotHeader, payload: &[u8], dst: &mut [u8]) -> Result<()> {
    let needed = HEADER_SIZE + payload.len();
    if dst.len() < needed {
        return Err(Error::new(ErrorKind::Encode, "destination buffer too small for slot"));
    }
    // SAFETY: see function-level safety comment.
    unsafe {
        core::ptr::copy_nonoverlapping(
            header as *const SlotHeader as *const u8,
            dst.as_mut_ptr(),
            HEADER_SIZE,
        );
    }
    dst[HEADER_SIZE..needed].copy_from_slice(payload);
    Ok(())
}

/// Decode a slot header and payload reference from `src`.
///
/// Returns `(header, payload_slice)` on success. Errors if:
///
/// - `src` is shorter than [`HEADER_SIZE`],
/// - the magic field does not equal [`SLOT_MAGIC`], or
/// - `src` is shorter than `HEADER_SIZE + header.payload_len`.
///
/// # Safety
///
/// The function contains one unsafe block that writes raw bytes into a
/// `MaybeUninit<SlotHeader>` via `copy_nonoverlapping`. This is sound
/// because:
///
/// - `SlotHeader` is `repr(C)` and every bit pattern is valid (no
///   invalid-enum variants, no references).
/// - We copy exactly `HEADER_SIZE` bytes from a caller-provided slice
///   that is at least that large (checked above).
/// - The destination is freshly allocated `MaybeUninit` so there is no
///   aliasing with any live reference.
pub fn decode(src: &[u8]) -> Result<(SlotHeader, &[u8])> {
    if src.len() < HEADER_SIZE {
        return Err(Error::new(ErrorKind::Decode, "buffer too small for slot header"));
    }
    // SAFETY: see function-level safety comment.
    let header: SlotHeader = unsafe {
        let mut h = core::mem::MaybeUninit::<SlotHeader>::uninit();
        core::ptr::copy_nonoverlapping(src.as_ptr(), h.as_mut_ptr() as *mut u8, HEADER_SIZE);
        h.assume_init()
    };
    if header.magic != SLOT_MAGIC {
        return Err(Error::new(ErrorKind::Decode, "invalid slot magic"));
    }
    let payload_end = HEADER_SIZE + header.payload_len as usize;
    if src.len() < payload_end {
        return Err(Error::new(ErrorKind::Decode, "buffer too small for slot payload"));
    }
    Ok((header, &src[HEADER_SIZE..payload_end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(seq: u64) -> SlotHeader {
        SlotHeader::new(1, FLAG_VALID, seq, 1_000_000_000, 4)
    }

    #[test]
    fn header_size_is_32() {
        assert_eq!(core::mem::size_of::<SlotHeader>(), HEADER_SIZE);
    }

    #[test]
    fn slot_size_rounds_to_cache_line() {
        assert_eq!(slot_size(0), CACHE_LINE);   // 32 header → rounds to 64
        assert_eq!(slot_size(32), CACHE_LINE);  // 32+32=64 → stays 64
        assert_eq!(slot_size(33), 2 * CACHE_LINE); // 32+33=65 → rounds to 128
        assert_eq!(slot_size(96), 2 * CACHE_LINE); // 32+96=128 → stays 128
        assert_eq!(slot_size(97), 3 * CACHE_LINE); // 32+97=129 → rounds to 192
    }

    #[test]
    fn encode_decode_roundtrip() {
        let header = make_header(42);
        let payload = b"tick";
        let mut buf = vec![0u8; HEADER_SIZE + payload.len()];

        encode(&header, payload, &mut buf).unwrap();
        let (decoded_header, decoded_payload) = decode(&buf).unwrap();

        assert_eq!(decoded_header, header);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn encode_rejects_short_dst() {
        let header = make_header(1);
        let mut buf = vec![0u8; HEADER_SIZE - 1];
        assert!(encode(&header, &[], &mut buf).is_err());
    }

    #[test]
    fn decode_rejects_short_src() {
        let buf = vec![0u8; HEADER_SIZE - 1];
        assert!(decode(&buf).is_err());
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let header = make_header(1);
        let mut buf = vec![0u8; HEADER_SIZE];
        encode(&header, &[], &mut buf).unwrap();
        buf[0] = 0xFF; // corrupt the magic
        assert!(decode(&buf).is_err());
    }

    #[test]
    fn decode_rejects_payload_overrun() {
        let header = SlotHeader::new(1, FLAG_VALID, 0, 0, 100);
        let mut buf = vec![0u8; HEADER_SIZE + 50]; // claim 100 but only have 50
        encode(&header, &[0u8; 50], &mut buf).unwrap();
        // Manually fix the payload_len to claim more than the buffer holds
        let len_offset = 28usize; // offset of payload_len in SlotHeader
        buf[len_offset..len_offset + 4].copy_from_slice(&100u32.to_ne_bytes());
        assert!(decode(&buf).is_err());
    }

    #[test]
    fn flag_valid_and_suspect() {
        let h = SlotHeader::new(1, FLAG_VALID | FLAG_SUSPECT, 0, 0, 0);
        assert!(h.is_valid());
        assert_eq!(h.flags & FLAG_SUSPECT, FLAG_SUSPECT);
    }

    #[test]
    fn flag_missing_valid() {
        let h = SlotHeader::new(1, 0, 0, 0, 0);
        assert!(!h.is_valid());
    }
}
