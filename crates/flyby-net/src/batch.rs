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

/// Per-packet metadata that accompanies each raw frame in a [`RawBatch`].
#[derive(Debug, Clone, Copy, Default)]
pub struct PacketMeta {
    /// Hardware or software receive timestamp in nanoseconds since the
    /// UNIX epoch. Zero when the source does not provide a timestamp.
    pub timestamp_ns: u64,
    /// NIC queue or ring index the packet was received on.
    pub queue_id: u16,
    /// Original wire length in bytes. May exceed the copied length when
    /// the frame was truncated to fit the batch slot. Callers should set
    /// this to the true wire size; the batch push path updates it when
    /// truncation occurs.
    pub original_len: u32,
}

/// Result of pushing a packet into a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    /// Stored without truncation.
    Ok,
    /// Stored but truncated to the slot size.
    Truncated,
    /// Batch is full; packet not stored.
    Full,
}

/// A reusable batch of raw network packets.
pub struct RawBatch {
    /// Pre-allocated payload buffers, one per slot.
    bufs: Vec<Vec<u8>>,
    /// Actual bytes written into each slot (≤ `bufs[i].len()`).
    lens: Vec<usize>,
    /// Per-slot metadata, parallel to `bufs`.
    meta: Vec<PacketMeta>,
    /// Number of valid slots in this batch (≤ `bufs.len()`).
    count: usize,
    max_frame_size: usize,
    /// Total packets successfully received since this batch was created.
    received: u64,
    /// Total packets dropped or truncated since this batch was created.
    dropped: u64,
    /// Frames that were truncated on push.
    truncated: u64,
}

impl RawBatch {
    /// Allocate a batch of `capacity` slots, each large enough for a
    /// frame of `max_frame_size` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` or `max_frame_size` is zero.
    pub fn new(capacity: usize, max_frame_size: usize) -> Self {
        assert!(capacity > 0, "RawBatch capacity must be > 0");
        assert!(max_frame_size > 0, "RawBatch max_frame_size must be > 0");
        let bufs = (0..capacity).map(|_| vec![0u8; max_frame_size]).collect();
        let lens = vec![0usize; capacity];
        let meta = vec![PacketMeta::default(); capacity];
        Self {
            bufs,
            lens,
            meta,
            count: 0,
            max_frame_size,
            received: 0,
            dropped: 0,
            truncated: 0,
        }
    }

    /// Reset the batch for the next poll.
    ///
    /// Clears the occupied count; cumulative counters are retained.
    /// The `max_frame_size` parameter is accepted for API symmetry with
    /// zero-copy backends; the copy-mode implementation ignores it.
    pub fn reset(&mut self, _max_frame_size: usize) {
        self.count = 0;
    }

    /// Maximum number of packets the batch can hold.
    pub fn capacity(&self) -> usize {
        self.bufs.len()
    }

    /// Configured max frame size per slot.
    pub fn max_frame_size(&self) -> usize {
        self.max_frame_size
    }

    /// Number of packets currently in the batch.
    pub fn len(&self) -> usize {
        self.count
    }

    /// `true` if no packets are in the batch.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Cumulative packets successfully stored.
    pub fn received(&self) -> u64 {
        self.received
    }

    /// Cumulative packets dropped (back-pressure / policy).
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Cumulative frames truncated to fit a slot.
    pub fn truncated(&self) -> u64 {
        self.truncated
    }

    /// Record an external drop (e.g. simulated NIC drop).
    pub fn record_drop(&mut self) {
        self.dropped = self.dropped.saturating_add(1);
    }

    /// Iterate over `(data, meta)` pairs for all packets in this batch.
    pub fn packets(&self) -> impl Iterator<Item = (&[u8], &PacketMeta)> {
        (0..self.count).map(move |i| (&self.bufs[i][..self.lens[i]], &self.meta[i]))
    }

    /// Mutable access to packet `index` (payload slice + metadata).
    ///
    /// Returns `None` when `index >= len()`. Useful for fault injection
    /// (payload corruption) and educational packet inspection.
    pub fn packet_mut(&mut self, index: usize) -> Option<(&mut [u8], &mut PacketMeta)> {
        if index >= self.count {
            return None;
        }
        let len = self.lens[index];
        Some((&mut self.bufs[index][..len], &mut self.meta[index]))
    }

    /// Shrink the occupied count to `new_len`, discarding trailing packets.
    ///
    /// No-op when `new_len >= len()`. Cumulative counters are retained.
    pub fn truncate(&mut self, new_len: usize) {
        if new_len < self.count {
            self.count = new_len;
        }
    }

    /// Copy `data` into the next free slot and record `meta`.
    ///
    /// When `data` exceeds the slot size, the frame is truncated, `meta.original_len`
    /// is set to the true length (if it was zero or smaller), and
    /// [`PushResult::Truncated`] is returned. Prefer sizing slots to avoid
    /// truncation; callers that forbid truncation should check the result.
    pub fn push(&mut self, data: &[u8], mut meta: PacketMeta) -> PushResult {
        if self.count >= self.bufs.len() {
            return PushResult::Full;
        }
        let slot = &mut self.bufs[self.count];
        let true_len = data.len();
        if meta.original_len == 0 || (meta.original_len as usize) < true_len {
            meta.original_len = true_len.min(u32::MAX as usize) as u32;
        }
        let copy_len = true_len.min(slot.len());
        slot[..copy_len].copy_from_slice(&data[..copy_len]);
        self.lens[self.count] = copy_len;
        self.meta[self.count] = meta;
        self.count += 1;
        self.received += 1;
        if copy_len < true_len {
            self.truncated += 1;
            PushResult::Truncated
        } else {
            PushResult::Ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_iterate() {
        let mut batch = RawBatch::new(4, 64);
        let meta = PacketMeta {
            timestamp_ns: 1000,
            queue_id: 0,
            original_len: 10,
        };
        assert_eq!(batch.push(b"hello", meta), PushResult::Ok);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.received(), 1);

        let packets: Vec<_> = batch.packets().collect();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, b"hello");
        assert_eq!(packets[0].1.timestamp_ns, 1000);
    }

    #[test]
    fn full_batch_returns_full() {
        let mut batch = RawBatch::new(2, 64);
        let meta = PacketMeta::default();
        assert_eq!(batch.push(b"a", meta), PushResult::Ok);
        assert_eq!(batch.push(b"b", meta), PushResult::Ok);
        assert_eq!(batch.push(b"c", meta), PushResult::Full);
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
        assert_eq!(batch.push(b"packet3", meta), PushResult::Ok);
        let packets: Vec<_> = batch.packets().collect();
        assert_eq!(packets[0].0, b"packet3");
    }

    #[test]
    fn truncates_and_sets_original_len() {
        let mut batch = RawBatch::new(1, 4);
        let meta = PacketMeta::default();
        assert_eq!(batch.push(b"hello world", meta), PushResult::Truncated);
        let packets: Vec<_> = batch.packets().collect();
        assert_eq!(packets[0].0, b"hell");
        assert_eq!(packets[0].1.original_len, 11);
        assert_eq!(batch.truncated(), 1);
    }

    #[test]
    fn received_counter_increments() {
        let mut batch = RawBatch::new(4, 64);
        let meta = PacketMeta::default();
        for _ in 0..3 {
            batch.push(b"x", meta);
        }
        assert_eq!(batch.received(), 3);
        batch.reset(64);
        assert_eq!(batch.received(), 3);
    }
}
