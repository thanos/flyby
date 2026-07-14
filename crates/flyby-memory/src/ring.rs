//! Lock-free SPSC ring buffer control.
//!
//! [`SpscRing`] manages two atomic sequence counters — `head` (producer)
//! and `tail` (consumer) — stored externally in a memory-mapped region.
//! It exposes a two-phase push/pop protocol:
//!
//! **Producer:**
//! 1. [`try_push`][SpscRing::try_push] — claim a slot index if space is available.
//! 2. Fill the slot at that index (handled by the caller).
//! 3. [`commit_push`][SpscRing::commit_push] — publish the slot to the consumer.
//!
//! **Consumer:**
//! 1. [`try_pop`][SpscRing::try_pop] — claim a slot index if data is available.
//! 2. Read the slot at that index (handled by the caller).
//! 3. [`commit_pop`][SpscRing::commit_pop] — release the slot back to the producer.
//!
//! ## Memory ordering
//!
//! The ordering choices below are the minimal correct set for SPSC:
//!
//! | Operation | Ordering | Rationale |
//! |-----------|----------|-----------|
//! | Producer loads `head` | `Relaxed` | Producer is the sole writer of `head`; no other thread can race on it. |
//! | Producer loads `tail` | `Acquire` | Synchronises with the consumer's `Release` store of `tail`, ensuring the producer sees the consumer has freed the slot. |
//! | Producer stores `head` | `Release` | Publishes the slot payload to the consumer: any `Acquire` load of `head` by the consumer will see everything written before this store. |
//! | Consumer loads `tail` | `Relaxed` | Consumer is the sole writer of `tail`. |
//! | Consumer loads `head` | `Acquire` | Synchronises with the producer's `Release` store, ensuring the consumer sees the slot payload. |
//! | Consumer stores `tail` | `Release` | Publishes slot-freed state to the producer. |
//!
//! `SeqCst` is intentionally avoided: the Acquire/Release pair is
//! sufficient for correctness and avoids unnecessary memory fences on
//! x86_64 (where `Release` store and `Acquire` load compile to plain MOV).

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};

/// Lock-free single-producer / single-consumer ring control.
///
/// Holds non-owning pointers to two `AtomicU64` counters that live
/// inside a memory-mapped [`crate::region::Region`]. The `Region` is
/// responsible for ensuring those pointers remain valid for at least as
/// long as any `SpscRing` that references them.
pub(crate) struct SpscRing {
    /// Producer sequence counter. Written only by the producer.
    head: NonNull<AtomicU64>,
    /// Consumer sequence counter. Written only by the consumer.
    tail: NonNull<AtomicU64>,
    /// Number of slots. Must be a power of two.
    capacity: usize,
    /// Bit-mask for wrapping: `capacity - 1`.
    mask: u64,
}

// SAFETY: `AtomicU64` operations are lock-free and inherently
// thread-safe. `SpscRing` enforces SPSC discipline at the API level:
// callers must not call `try_push`/`commit_push` from more than one
// thread, nor `try_pop`/`commit_pop` from more than one thread. These
// requirements are documented on each method.
unsafe impl Send for SpscRing {}
unsafe impl Sync for SpscRing {}

impl SpscRing {
    /// Construct a ring view from raw pointers into a mmap'd region.
    ///
    /// # Safety
    ///
    /// - `head` and `tail` must point to properly aligned, valid
    ///   `AtomicU64` storage that remains live for at least as long as
    ///   this `SpscRing`.
    /// - The pointed-to values must have been zero-initialised before
    ///   this call (zero is the correct initial sequence value).
    /// - `capacity` must be a non-zero power of two and at most
    ///   `u64::MAX / 2` to prevent wrapping ambiguity.
    pub(crate) unsafe fn new(
        head: NonNull<AtomicU64>,
        tail: NonNull<AtomicU64>,
        capacity: usize,
    ) -> Self {
        debug_assert!(capacity.is_power_of_two(), "capacity must be a power of two");
        debug_assert!(capacity <= u64::MAX as usize / 2, "capacity too large");
        Self { head, tail, capacity, mask: (capacity - 1) as u64 }
    }

    fn head(&self) -> &AtomicU64 {
        // SAFETY: constructor invariant guarantees `head` is valid and aligned.
        unsafe { self.head.as_ref() }
    }

    fn tail(&self) -> &AtomicU64 {
        // SAFETY: constructor invariant guarantees `tail` is valid and aligned.
        unsafe { self.tail.as_ref() }
    }

    /// Attempt to claim the next write slot.
    ///
    /// Returns the slot index to write into, or `None` if the ring is full.
    ///
    /// **Must be called from the single producer thread only.**
    pub(crate) fn try_push(&self) -> Option<usize> {
        let head = self.head().load(Ordering::Relaxed);
        let tail = self.tail().load(Ordering::Acquire);
        if head.wrapping_sub(tail) == self.capacity as u64 {
            return None;
        }
        Some((head & self.mask) as usize)
    }

    /// Publish the slot claimed by [`try_push`][Self::try_push].
    ///
    /// Must be called after the slot contents have been written.
    ///
    /// **Must be called from the single producer thread only.**
    pub(crate) fn commit_push(&self) {
        let head = self.head().load(Ordering::Relaxed);
        self.head().store(head.wrapping_add(1), Ordering::Release);
    }

    /// Attempt to claim the next read slot.
    ///
    /// Returns the slot index to read from, or `None` if the ring is empty.
    ///
    /// **Must be called from the single consumer thread only.**
    pub(crate) fn try_pop(&self) -> Option<usize> {
        let tail = self.tail().load(Ordering::Relaxed);
        let head = self.head().load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        Some((tail & self.mask) as usize)
    }

    /// Release the slot claimed by [`try_pop`][Self::try_pop].
    ///
    /// Must be called after the slot contents have been read.
    ///
    /// **Must be called from the single consumer thread only.**
    pub(crate) fn commit_pop(&self) {
        let tail = self.tail().load(Ordering::Relaxed);
        self.tail().store(tail.wrapping_add(1), Ordering::Release);
    }

    /// Number of slots the ring can hold in total.
    #[allow(dead_code)]
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of slots currently occupied.
    ///
    /// Approximate in a concurrent context: both counters are loaded
    /// with `Relaxed` so the result may be stale. Callers that need
    /// precise accounting must coordinate externally.
    pub(crate) fn len(&self) -> usize {
        let head = self.head().load(Ordering::Relaxed);
        let tail = self.tail().load(Ordering::Relaxed);
        head.wrapping_sub(tail) as usize
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn is_full(&self) -> bool {
        self.len() == self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU64;

    fn make_ring(capacity: usize) -> (SpscRing, Box<AtomicU64>, Box<AtomicU64>) {
        let head = Box::new(AtomicU64::new(0));
        let tail = Box::new(AtomicU64::new(0));
        let ring = unsafe {
            SpscRing::new(
                NonNull::from(head.as_ref()),
                NonNull::from(tail.as_ref()),
                capacity,
            )
        };
        (ring, head, tail)
    }

    #[test]
    fn empty_ring_returns_none_on_pop() {
        let (ring, _h, _t) = make_ring(4);
        assert!(ring.try_pop().is_none());
        assert!(ring.is_empty());
    }

    #[test]
    fn full_ring_returns_none_on_push() {
        let (ring, _h, _t) = make_ring(4);
        for _ in 0..4 {
            let idx = ring.try_push().expect("should have space");
            assert!(idx < 4);
            ring.commit_push();
        }
        assert!(ring.is_full());
        assert!(ring.try_push().is_none());
    }

    #[test]
    fn push_pop_sequence_monotonic() {
        let (ring, _h, _t) = make_ring(4);
        let mut written = Vec::new();
        let mut read = Vec::new();

        for i in 0..4u64 {
            let idx = ring.try_push().unwrap();
            written.push(idx);
            ring.commit_push();
            assert_eq!(ring.len(), (i + 1) as usize);
        }

        for _ in 0..4 {
            let idx = ring.try_pop().unwrap();
            read.push(idx);
            ring.commit_pop();
        }

        assert_eq!(written, read, "slot indices must match push order (FIFO)");
        assert!(ring.is_empty());
    }

    #[test]
    fn wrap_around() {
        let (ring, _h, _t) = make_ring(4);
        // fill, drain, fill again — indices must wrap modulo capacity
        for _ in 0..8 {
            let idx = ring.try_push().unwrap();
            ring.commit_push();
            let popped = ring.try_pop().unwrap();
            ring.commit_pop();
            assert_eq!(idx, popped);
        }
    }

    #[test]
    fn capacity_is_preserved() {
        let (ring, _h, _t) = make_ring(8);
        assert_eq!(ring.capacity(), 8);
    }
}
