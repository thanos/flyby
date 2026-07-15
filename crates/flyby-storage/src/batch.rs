//! [`RawRecordBatch`]: a pre-allocated, reusable batch of raw storage records.
//!
//! Mirrors `flyby_net::RawBatch` but for file/NVMe sources.  Each slot
//! holds the raw bytes of one framed record.  The batch is allocated once
//! and reused across polls to avoid per-batch heap allocation in the hot path.
//!
//! ## Lifecycle
//!
//! 1. Allocate once: `RawRecordBatch::new(capacity, max_record_size)`.
//! 2. On each poll: call [`RawRecordBatch::reset`], then pass to
//!    [`crate::source::StorageSource::poll_batch`].
//! 3. Iterate with [`RawRecordBatch::records`].

/// Per-record metadata stored alongside each raw payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordMeta {
    /// Byte offset of the record's first byte in the source file.
    ///
    /// Used by the replay engine to seek back to a known position.
    pub file_offset: u64,
    /// Capture or injected timestamp in nanoseconds since the UNIX epoch.
    ///
    /// Zero when the source does not embed a timestamp.
    pub timestamp_ns: u64,
    /// Index of the record within the current source file.
    pub record_index: u64,
}

/// A reusable batch of raw storage records.
///
/// Created once, reused across polls. [`reset`][Self::reset] clears the
/// occupied count without deallocating the underlying buffers.
pub struct RawRecordBatch {
    bufs: Vec<Vec<u8>>,
    lens: Vec<usize>,
    meta: Vec<RecordMeta>,
    count: usize,
    /// Total records successfully read since this batch was created.
    pub records_read: u64,
    /// Total records skipped due to parser or framing errors.
    pub parse_errors: u64,
}

impl RawRecordBatch {
    /// Allocate a batch of `capacity` slots each large enough for a record of
    /// `max_record_size` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is zero.
    pub fn new(capacity: usize, max_record_size: usize) -> Self {
        assert!(capacity > 0, "RawRecordBatch capacity must be > 0");
        let bufs = (0..capacity).map(|_| vec![0u8; max_record_size]).collect();
        let lens = vec![0usize; capacity];
        let meta = vec![RecordMeta::default(); capacity];
        Self {
            bufs,
            lens,
            meta,
            count: 0,
            records_read: 0,
            parse_errors: 0,
        }
    }

    /// Reset the batch for the next poll.
    ///
    /// Clears the occupied count; all pre-allocated buffers are retained.
    pub fn reset(&mut self) {
        self.count = 0;
    }

    /// Maximum number of records the batch can hold.
    pub fn capacity(&self) -> usize {
        self.bufs.len()
    }

    /// Number of records currently in the batch.
    pub fn len(&self) -> usize {
        self.count
    }

    /// `true` if no records are in the batch.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterate over `(data, meta)` pairs for all records in this batch.
    pub fn records(&self) -> impl Iterator<Item = (&[u8], &RecordMeta)> {
        (0..self.count).map(move |i| (&self.bufs[i][..self.lens[i]], &self.meta[i]))
    }

    /// Copy `data` into the next free slot and record `meta`.
    ///
    /// Returns `true` on success, `false` if the batch is full.
    /// Silently truncates records that exceed the slot capacity; the caller
    /// should size buffers to `max_record_size` to avoid silent data loss.
    pub(crate) fn push(&mut self, data: &[u8], meta: RecordMeta) -> bool {
        if self.count >= self.bufs.len() {
            return false;
        }
        let slot = &mut self.bufs[self.count];
        let copy_len = data.len().min(slot.len());
        slot[..copy_len].copy_from_slice(&data[..copy_len]);
        self.lens[self.count] = copy_len;
        self.meta[self.count] = meta;
        self.count += 1;
        self.records_read += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_iterate() {
        let mut batch = RawRecordBatch::new(4, 64);
        let meta = RecordMeta {
            file_offset: 100,
            timestamp_ns: 1_000_000,
            record_index: 0,
        };
        assert!(batch.push(b"hello", meta));
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.records_read, 1);

        let records: Vec<_> = batch.records().collect();
        assert_eq!(records[0].0, b"hello");
        assert_eq!(records[0].1.file_offset, 100);
    }

    #[test]
    fn full_batch_returns_false() {
        let mut batch = RawRecordBatch::new(2, 64);
        let meta = RecordMeta::default();
        assert!(batch.push(b"a", meta));
        assert!(batch.push(b"b", meta));
        assert!(!batch.push(b"c", meta));
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn reset_clears_count() {
        let mut batch = RawRecordBatch::new(4, 64);
        let meta = RecordMeta::default();
        batch.push(b"x", meta);
        batch.push(b"y", meta);
        batch.reset();
        assert_eq!(batch.len(), 0);
        assert!(batch.push(b"z", meta));
        let records: Vec<_> = batch.records().collect();
        assert_eq!(records[0].0, b"z");
    }

    #[test]
    fn records_read_is_cumulative() {
        let mut batch = RawRecordBatch::new(4, 64);
        let meta = RecordMeta::default();
        batch.push(b"a", meta);
        batch.push(b"b", meta);
        batch.reset();
        assert_eq!(batch.records_read, 2);
        batch.push(b"c", meta);
        assert_eq!(batch.records_read, 3);
    }

    #[test]
    fn truncates_oversized_records() {
        let mut batch = RawRecordBatch::new(1, 4);
        let meta = RecordMeta::default();
        batch.push(b"hello world", meta);
        let records: Vec<_> = batch.records().collect();
        assert_eq!(records[0].0, b"hell");
    }
}
