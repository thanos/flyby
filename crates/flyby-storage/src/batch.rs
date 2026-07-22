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

use flyby_core::{Error, ErrorKind, Result};

/// Per-record metadata stored alongside each raw payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordMeta {
    /// Byte offset of the record's first byte in the source file.
    ///
    /// Useful for diagnostics and restart markers. The replay engine uses
    /// timestamps for timing, not this offset, for seeks.
    pub file_offset: u64,
    /// Capture or injected timestamp in nanoseconds since the UNIX epoch.
    ///
    /// Zero when the source does not embed a timestamp.
    pub timestamp_ns: u64,
    /// Index of the record within the current source file.
    pub record_index: u64,
}

/// Result of attempting to push a record into a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    /// Record stored successfully.
    Ok,
    /// Batch is full; record was not stored.
    Full,
    /// Record exceeded the slot size; not stored (no silent truncation).
    Oversized,
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
    max_record_size: usize,
    /// Total records successfully read since this batch was created.
    pub records_read: u64,
    /// Total records skipped due to parser, framing, or size errors.
    pub parse_errors: u64,
}

impl RawRecordBatch {
    /// Allocate a batch of `capacity` slots each large enough for a record of
    /// `max_record_size` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is zero or `max_record_size` is zero.
    pub fn new(capacity: usize, max_record_size: usize) -> Self {
        assert!(capacity > 0, "RawRecordBatch capacity must be > 0");
        assert!(
            max_record_size > 0,
            "RawRecordBatch max_record_size must be > 0"
        );
        let bufs = (0..capacity).map(|_| vec![0u8; max_record_size]).collect();
        let lens = vec![0usize; capacity];
        let meta = vec![RecordMeta::default(); capacity];
        Self {
            bufs,
            lens,
            meta,
            count: 0,
            max_record_size,
            records_read: 0,
            parse_errors: 0,
        }
    }

    /// Slot capacity in bytes (configured `max_record_size`).
    pub fn max_record_size(&self) -> usize {
        self.max_record_size
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

    /// Mutable access to record `index` (payload slice + metadata).
    ///
    /// Returns `None` when `index >= len()`.
    pub fn record_mut(&mut self, index: usize) -> Option<(&mut [u8], &mut RecordMeta)> {
        if index >= self.count {
            return None;
        }
        let len = self.lens[index];
        Some((&mut self.bufs[index][..len], &mut self.meta[index]))
    }

    /// Shrink the occupied count to `new_len`, discarding trailing records.
    pub fn truncate(&mut self, new_len: usize) {
        if new_len < self.count {
            self.count = new_len;
        }
    }

    /// Copy `data` into the next free slot and record `meta`.
    ///
    /// Does **not** truncate: oversized records return [`PushResult::Oversized`]
    /// and increment [`parse_errors`][Self::parse_errors].
    pub fn push(&mut self, data: &[u8], meta: RecordMeta) -> PushResult {
        if self.count >= self.bufs.len() {
            return PushResult::Full;
        }
        if data.len() > self.bufs[self.count].len() {
            self.parse_errors += 1;
            return PushResult::Oversized;
        }
        let slot = &mut self.bufs[self.count];
        slot[..data.len()].copy_from_slice(data);
        self.lens[self.count] = data.len();
        self.meta[self.count] = meta;
        self.count += 1;
        self.records_read += 1;
        PushResult::Ok
    }

    /// Like [`push`][Self::push] but maps failures to [`Error`].
    pub(crate) fn try_push(&mut self, data: &[u8], meta: RecordMeta) -> Result<()> {
        match self.push(data, meta) {
            PushResult::Ok => Ok(()),
            PushResult::Full => Err(Error::new(ErrorKind::Source, "record batch is full")),
            PushResult::Oversized => Err(Error::new(
                ErrorKind::Decode,
                format!(
                    "record length {} exceeds max_record_size {}",
                    data.len(),
                    self.max_record_size
                ),
            )),
        }
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
        assert_eq!(batch.push(b"hello", meta), PushResult::Ok);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.records_read, 1);

        let records: Vec<_> = batch.records().collect();
        assert_eq!(records[0].0, b"hello");
        assert_eq!(records[0].1.file_offset, 100);
    }

    #[test]
    fn full_batch_returns_full() {
        let mut batch = RawRecordBatch::new(2, 64);
        let meta = RecordMeta::default();
        assert_eq!(batch.push(b"a", meta), PushResult::Ok);
        assert_eq!(batch.push(b"b", meta), PushResult::Ok);
        assert_eq!(batch.push(b"c", meta), PushResult::Full);
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
        assert_eq!(batch.push(b"z", meta), PushResult::Ok);
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
    fn rejects_oversized_records() {
        let mut batch = RawRecordBatch::new(1, 4);
        let meta = RecordMeta::default();
        assert_eq!(batch.push(b"hello world", meta), PushResult::Oversized);
        assert_eq!(batch.len(), 0);
        assert_eq!(batch.parse_errors, 1);
        assert!(batch.try_push(b"hello world", meta).is_err());
    }
}
