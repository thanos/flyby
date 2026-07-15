//! [`RawBatch`]: a pre-allocated, reusable packet batch.
//!
//! A batch is the unit of work handed from a [`crate::source::NetworkSource`]
//! to the pipeline. It holds up to `capacity` packets and is reused
//! across polls to avoid per-batch heap allocation in the hot path.
//!
//! ## Lifecycle
//!
//! 1. Allocate once: `RawBatch::new(capacity, max_frame_size)`.
//! 2. On each poll: call [`RawBatch::reset`] to clear the count, then
//!    pass `&mut batch` to [`crate::source::NetworkSource::poll_batch`].
//! 3. Iterate the result with [`RawBatch::packets`].
//!
//! ## Zero-copy note
//!
//! The current implementation copies packet data into pre-allocated
//! `Vec<u8>` buffers. This is intentionally conservative (copy mode).
//! The AF_XDP zero-copy backend will replace this with UMEM-backed
//! descriptors that point directly into kernel-shared memory — a
//! different memory domain that must not be confused with the FlyBy
//! shared-memory sink.

/// Per-packet metadata that accompanies each raw frame in a [`RawBatch`].
#[derive(Debug, Clone, Copy, Default)]
pub struct PacketMeta {
    /// Hardware or software receive timestamp in nanoseconds since the
    /// UNIX epoch. Zero when the source does not provide a timestamp.
    pub timestamp_ns: u64,
    /// NIC queue or ring index the packet was received on.
    pub queue_id: u16,
    /// Original wire length. May exceed `data.len()` if the packet was
    /// truncated by the capture path.
    pub original_len: u16,
}

/// A reusable batch of raw network packets.
///
/// Created once, reused across polls. [`reset`][Self::reset] clears the
/// occupied count without deallocating the underlying buffers.
pub struct RawBatch {
    /// Pre-allocated payload buffers, one per slot.
    bufs: Vec<Vec<u8>>,
    /// Actual bytes written into each slot (≤ `bufs[i].len()`).
    lens: Vec<usize>,
    /// Per-slot metadata, parallel to `bufs`.
    meta: Vec<PacketMeta>,
    /// Number of valid slots in this batch (≤ `bufs.len()`).
    count: usize,
    /// Total packets successfully received since this batch was created.
    pub received: u64,
    /// Total packets dropped since this batch was created.
    ///
    /// Incremented by the source when back-pressure forces a discard.
    /// Never silently zero.
    pub dropped: u64,
}

impl RawBatch {
    /// Allocate a batch of `capacity` slots, each large enough for a
    /// frame of `max_frame_size` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is zero.
    pub fn new(capacity: usize, max_frame_size: usize) -> Self {
        assert!(capacity > 0, "RawBatch capacity must be > 0");
        let bufs = (0..capacity).map(|_| vec![0u8; max_frame_size]).collect();
        let lens = vec![0usize; capacity];
        let meta = vec![PacketMeta::default(); capacity];
        Self { bufs, lens, meta, count: 0, received: 0, dropped: 0 }
    }

    /// Reset the batch for the next poll.
    ///
    /// Clears the occupied count; all pre-allocated buffers are retained.
    /// The `max_frame_size` parameter is accepted for API symmetry with
    /// zero-copy backends (which may need it to reset descriptor rings);
    /// the copy-mode implementation ignores it.
    pub fn reset(&mut self, _max_frame_size: usize) {
        self.count = 0;
    }

    /// Maximum number of packets the batch can hold.
    pub fn capacity(&self) -> usize {
        self.bufs.len()
    }

    /// Number of packets currently in the batch.
    pub fn len(&self) -> usize {
        self.count
    }

    /// `true` if no packets are in the batch.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterate over `(data, meta)` pairs for all packets in this batch.
    pub fn packets(&self) -> impl Iterator<Item = (&[u8], &PacketMeta)> {
        (0..self.count).map(move |i| (&self.bufs[i][..self.lens[i]], &self.meta[i]))
    }

    /// Copy `data` into the next free slot and record `meta`.
    ///
    /// Returns `true` on success, `false` if the batch is full.
    /// Truncates `data` silently if it exceeds the pre-allocated slot
    /// size (the `original_len` field in `meta` preserves the true length).
    pub(crate) fn push(&mut self, data: &[u8], meta: PacketMeta) -> bool {
        if self.count >= self.bufs.len() {
            return false;
        }
        let slot = &mut self.bufs[self.count];
        let copy_len = data.len().min(slot.len());
        slot[..copy_len].copy_from_slice(&data[..copy_len]);
        self.lens[self.count] = copy_len;
        self.meta[self.count] = meta;
        self.count += 1;
        self.received += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_iterate() {
        let mut batch = RawBatch::new(4, 64);
        let meta = PacketMeta { timestamp_ns: 1000, queue_id: 0, original_len: 10 };
        assert!(batch.push(b"hello", meta));
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.received, 1);

        let packets: Vec<_> = batch.packets().collect();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, b"hello");
        assert_eq!(packets[0].1.timestamp_ns, 1000);
    }

    #[test]
    fn full_batch_returns_false() {
        let mut batch = RawBatch::new(2, 64);
        let meta = PacketMeta::default();
        assert!(batch.push(b"a", meta));
        assert!(batch.push(b"b", meta));
        assert!(!batch.push(b"c", meta));
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn reset_reuses_allocation() {
        let mut batch = RawBatch::new(4, 64);
        let meta = PacketMeta::default();
        batch.push(b"packet1", meta);
        batch.push(b"packet2", meta);
        assert_eq!(batch.len(), 2);

        batch.reset(64);
        assert_eq!(batch.len(), 0);
        assert!(batch.push(b"packet3", meta));
        let packets: Vec<_> = batch.packets().collect();
        assert_eq!(packets[0].0, b"packet3");
    }

    #[test]
    fn truncates_oversized_data() {
        let mut batch = RawBatch::new(1, 4);
        let meta = PacketMeta::default();
        batch.push(b"hello world", meta);
        let packets: Vec<_> = batch.packets().collect();
        assert_eq!(packets[0].0, b"hell");
    }

    #[test]
    fn received_counter_increments() {
        let mut batch = RawBatch::new(4, 64);
        let meta = PacketMeta::default();
        for _ in 0..3 {
            batch.push(b"x", meta);
        }
        assert_eq!(batch.received, 3);
        batch.reset(64);
        assert_eq!(batch.received, 3); // cumulative, not reset
    }
}
