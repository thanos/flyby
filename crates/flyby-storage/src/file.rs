//! Sequential file backend.
//!
//! [`FileSource`] reads records from a local file using a configurable
//! [`Frame`] strategy.  It is the simplest storage backend: no async I/O,
//! no DMA, no ring buffers.  Its purpose is to validate the pipeline and
//! replay engine before the io_uring and SPDK backends are introduced
//! (ADR-0005).
//!
//! ## Read path
//!
//! ```text
//! file → read_buf → framer → RawRecordBatch → Decoder → typed Message
//! ```
//!
//! The read buffer is a flat `Vec<u8>` kept between polls so partial records
//! are not discarded.  When the framer signals `Ok(None)` (more data needed),
//! the source reads another chunk from the file and retries until either the
//! batch is full or EOF is reached.
//!
//! Incomplete trailing bytes at EOF (Stop policy) are discarded; they never
//! become a record.
//!
//! ## Restart support
//!
//! When `EofPolicy::Loop` is set the source rewinds the file to offset 0,
//! clears any partial buffer, and continues.  At most one rewind is
//! performed per `poll_batch` call.  If a full pass yields zero complete
//! records, the source returns a decode error instead of spinning.
//!
//! ## Follow policy
//!
//! `EofPolicy::Follow` returns an empty batch at EOF.  Sleeping for
//! `poll_interval` is the **caller's** responsibility (the engine does not
//! block).
//!
//! ## Config
//!
//! [`FileConfig::batch_size`] and [`FileConfig::max_record_size`] are used by
//! [`FileConfig::new_batch`].  The source rejects framed records larger than
//! the batch slot size.  Replay mode on the config is applied by a separate
//! [`crate::replay::ReplayEngine`] adapter, not inside this source.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

use flyby_core::{Error, ErrorKind, Lifecycle, Result};

use crate::batch::{RawRecordBatch, RecordMeta};
use crate::config::{EofPolicy, FileConfig};
use crate::framing::Frame;
use crate::source::StorageSource;

// Read buffer chunk size: read this many bytes from the file at a time.
const READ_CHUNK: usize = 64 * 1024; // 64 KiB

/// Sequential file source.
///
/// Reads records from a local file using a caller-supplied framing strategy.
/// All I/O is synchronous (`std::fs::File`).  For high-throughput workloads
/// on Linux, the io_uring backend is preferred.
pub struct FileSource<F: Frame> {
    config: FileConfig,
    framer: F,
    file: Option<File>,
    /// Buffered bytes awaiting framing.  May contain an incomplete record at the end.
    read_buf: Vec<u8>,
    /// Byte offset of `read_buf[0]` within the file.
    buf_file_offset: u64,
    /// Index of the next record to be emitted.
    record_index: u64,
    /// Total bytes read from the file.
    bytes_read: u64,
    /// Number of times the source has looped back to the start.
    loop_count: u64,
    /// EOF was reached and the policy is `Stop`.
    exhausted: bool,
    /// `true` after `init()`, `false` after `shutdown()`.
    initialized: bool,
    /// Earliest time a Follow poll may return data again.
    follow_next: Option<Instant>,
    /// Complete records emitted since the last Loop rewind (or init).
    records_since_rewind: u64,
}

impl<F: Frame> FileSource<F> {
    /// Create a new [`FileSource`] with the given config and framing strategy.
    ///
    /// The file is not opened until [`Lifecycle::init`] is called.
    pub fn new(config: FileConfig, framer: F) -> Self {
        Self {
            config,
            framer,
            file: None,
            read_buf: Vec::new(),
            buf_file_offset: 0,
            record_index: 0,
            bytes_read: 0,
            loop_count: 0,
            exhausted: false,
            initialized: false,
            follow_next: None,
            records_since_rewind: 0,
        }
    }

    /// Borrow the file configuration.
    pub fn config(&self) -> &FileConfig {
        &self.config
    }

    /// Total bytes read from the underlying file.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Total records emitted.
    pub fn record_index(&self) -> u64 {
        self.record_index
    }

    /// Number of times the source has looped (EofPolicy::Loop only).
    pub fn loop_count(&self) -> u64 {
        self.loop_count
    }

    // Fill `read_buf` by appending up to READ_CHUNK bytes from the file.
    // Returns Ok(true) if bytes were appended, Ok(false) at EOF.
    fn fill_buf(&mut self) -> Result<bool> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| Error::lifecycle("FileSource: not initialized"))?;

        let prev_len = self.read_buf.len();
        self.read_buf.resize(prev_len + READ_CHUNK, 0);
        let n = file.read(&mut self.read_buf[prev_len..])?;
        self.read_buf.truncate(prev_len + n);
        self.bytes_read += n as u64;
        Ok(n > 0)
    }

    // Handle EOF according to the configured policy.
    // Returns Ok(true) if the source should continue reading (Loop policy),
    // Ok(false) if the batch is done for this poll.
    fn handle_eof(&mut self, rewound_this_poll: &mut bool) -> Result<bool> {
        match &self.config.eof_policy {
            EofPolicy::Stop => {
                // Incomplete trailing bytes are discarded by design.
                self.read_buf.clear();
                self.exhausted = true;
                Ok(false)
            }
            EofPolicy::Loop => {
                if *rewound_this_poll {
                    // Already rewound once this poll; stop to avoid spinning.
                    return Ok(false);
                }
                if self.records_since_rewind == 0 && self.read_buf.is_empty() {
                    return Err(Error::new(
                        ErrorKind::Decode,
                        "FileSource: EofPolicy::Loop with no complete records in file",
                    ));
                }
                if self.records_since_rewind == 0 {
                    return Err(Error::new(
                        ErrorKind::Decode,
                        "FileSource: EofPolicy::Loop cannot frame any complete record \
                         (file shorter than one frame or missing delimiter)",
                    ));
                }
                let file = self
                    .file
                    .as_mut()
                    .ok_or_else(|| Error::lifecycle("FileSource: not initialized"))?;
                // Drop partial trailing data so the next pass starts clean.
                self.read_buf.clear();
                file.seek(SeekFrom::Start(0))?;
                self.buf_file_offset = 0;
                self.loop_count += 1;
                self.records_since_rewind = 0;
                *rewound_this_poll = true;
                Ok(true)
            }
            EofPolicy::Follow { poll_interval } => {
                let interval = *poll_interval;
                // Incomplete buffer is retained so Follow can complete a
                // partial record when more data appears.
                self.follow_next = Some(Instant::now() + interval);
                Ok(false)
            }
        }
    }

    // Consume one complete record from the front of `read_buf`.
    fn consume_record(&mut self, len: usize, batch: &mut RawRecordBatch) -> Result<()> {
        if len == 0 {
            return Err(Error::new(
                ErrorKind::Decode,
                "framing: zero-length record is not allowed",
            ));
        }
        if len > self.read_buf.len() {
            return Err(Error::new(
                ErrorKind::Decode,
                format!(
                    "framing: record length {len} exceeds buffered bytes {}",
                    self.read_buf.len()
                ),
            ));
        }
        if len > batch.max_record_size() {
            return Err(Error::new(
                ErrorKind::Decode,
                format!(
                    "record length {len} exceeds max_record_size {}",
                    batch.max_record_size()
                ),
            ));
        }

        let meta = RecordMeta {
            file_offset: self.buf_file_offset,
            timestamp_ns: 0, // file source does not embed timestamps
            record_index: self.record_index,
        };
        batch.try_push(&self.read_buf[..len], meta)?;
        self.read_buf.drain(..len);
        self.buf_file_offset += len as u64;
        self.record_index += 1;
        self.records_since_rewind += 1;
        Ok(())
    }
}

impl<F: Frame> Lifecycle for FileSource<F> {
    fn init(&mut self) -> Result<()> {
        let file = File::open(&self.config.path).map_err(|e| {
            Error::with_source(
                ErrorKind::Io,
                format!("FileSource: cannot open {:?}", self.config.path),
                e,
            )
        })?;
        self.file = Some(file);
        self.read_buf.clear();
        self.buf_file_offset = 0;
        self.record_index = 0;
        self.bytes_read = 0;
        self.loop_count = 0;
        self.exhausted = false;
        self.initialized = true;
        self.follow_next = None;
        self.records_since_rewind = 0;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.file = None;
        self.read_buf.clear();
        self.initialized = false;
        self.follow_next = None;
        Ok(())
    }
}

impl<F: Frame> StorageSource for FileSource<F> {
    fn poll_batch(&mut self, batch: &mut RawRecordBatch) -> Result<usize> {
        if !self.initialized {
            return Err(Error::lifecycle(
                "FileSource: call init() before poll_batch()",
            ));
        }
        if self.exhausted {
            return Ok(0);
        }

        if let Some(next) = self.follow_next {
            if Instant::now() < next {
                return Ok(0);
            }
            self.follow_next = None;
        }

        let start_count = batch.len();
        let mut rewound_this_poll = false;

        'outer: while batch.len() < batch.capacity() {
            // Try to frame a record from what's buffered.
            loop {
                if self.read_buf.is_empty() {
                    break;
                }
                match self.framer.next_record_len(&self.read_buf)? {
                    Some(len) => {
                        self.consume_record(len, batch)?;
                        if batch.len() >= batch.capacity() {
                            break 'outer;
                        }
                    }
                    None => break, // need more data
                }
            }

            // Need more data from the file.
            let got_data = self.fill_buf()?;
            if !got_data {
                let should_continue = self.handle_eof(&mut rewound_this_poll)?;
                if !should_continue {
                    break;
                }
            }
        }

        Ok(batch.len() - start_count)
    }

    fn backend_name() -> &'static str {
        "file"
    }

    fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// Suggested Follow interval when constructing configs (callers still sleep).
pub fn default_follow_interval() -> Duration {
    Duration::from_millis(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::{Delimiter, FixedLength};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(data: &[u8]) -> (NamedTempFile, FileConfig) {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(data).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_path_buf();
        let cfg = FileConfig {
            path,
            ..FileConfig::default()
        };
        (tmp, cfg)
    }

    #[test]
    fn reads_fixed_length_records() {
        let data = b"AAAAAAAABBBBBBBBCCCCCCCC"; // 3 × 8-byte records
        let (_tmp, cfg) = write_temp(data);
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(16, 64);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 3);
        let records: Vec<_> = batch.records().map(|(d, _)| d.to_vec()).collect();
        assert_eq!(records[0], b"AAAAAAAA");
        assert_eq!(records[1], b"BBBBBBBB");
        assert_eq!(records[2], b"CCCCCCCC");
    }

    #[test]
    fn reads_newline_delimited_records() {
        let data = b"hello\nworld\nfoo\n";
        let (_tmp, cfg) = write_temp(data);
        let mut src = FileSource::new(cfg, Delimiter::new(b'\n', 1024));
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(16, 256);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 3);
        let records: Vec<_> = batch.records().map(|(d, _)| d.to_vec()).collect();
        assert_eq!(records[0], b"hello\n");
        assert_eq!(records[2], b"foo\n");
    }

    #[test]
    fn stops_at_eof() {
        let (_tmp, cfg) = write_temp(b"AAAAAAAA");
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(16, 64);
        src.poll_batch(&mut batch).unwrap();
        assert!(src.is_exhausted());
        batch.reset();
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn loops_on_eof_loop_policy() {
        let data = b"AAAAAAAA"; // exactly one 8-byte record
        let (_tmp, mut cfg) = write_temp(data);
        cfg.eof_policy = EofPolicy::Loop;
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();

        // capacity=2: reads 1 record, hits EOF (loops once), reads 1 more.
        let mut batch = RawRecordBatch::new(2, 64);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 2, "should have read 2 records across the loop boundary");
        assert!(!src.is_exhausted());
        assert_eq!(src.loop_count(), 1);
    }

    #[test]
    fn empty_file_loop_returns_error() {
        let (_tmp, mut cfg) = write_temp(b"");
        cfg.eof_policy = EofPolicy::Loop;
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(4, 64);
        let err = src.poll_batch(&mut batch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode);
    }

    #[test]
    fn partial_file_loop_returns_error() {
        // 5 bytes cannot form an 8-byte fixed record.
        let (_tmp, mut cfg) = write_temp(b"AAAAA");
        cfg.eof_policy = EofPolicy::Loop;
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(4, 64);
        let err = src.poll_batch(&mut batch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode);
    }

    #[test]
    fn loop_does_not_splice_partial_tail() {
        // 13 bytes = 1 full 8-byte record + 5 trailing bytes.
        let data = b"AAAAAAAABBBBB";
        let (_tmp, mut cfg) = write_temp(data);
        cfg.eof_policy = EofPolicy::Loop;
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(3, 64);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 2);
        let records: Vec<_> = batch.records().map(|(d, _)| d.to_vec()).collect();
        assert_eq!(records[0], b"AAAAAAAA");
        // Second record is a clean rewind of the first, not a splice of
        // trailing BBBBB + AAAAAAA.
        assert_eq!(records[1], b"AAAAAAAA");
    }

    #[test]
    fn oversized_record_errors() {
        let data = b"AAAAAAAA";
        let (_tmp, cfg) = write_temp(data);
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(4, 4); // slot too small
        let err = src.poll_batch(&mut batch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode);
    }

    #[test]
    fn follow_returns_zero_at_eof() {
        let (_tmp, mut cfg) = write_temp(b"AAAAAAAA");
        cfg.eof_policy = EofPolicy::Follow {
            poll_interval: Duration::from_secs(3600),
        };
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(4, 64);
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 1);
        batch.reset();
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 0);
        assert!(!src.is_exhausted());
    }

    #[test]
    fn batch_size_limits_records_per_poll() {
        let mut data = Vec::new();
        for _ in 0..10 {
            data.extend_from_slice(b"AAAAAAAA");
        }
        let (_tmp, cfg) = write_temp(&data);
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(4, 64); // capacity = 4
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 4);
        assert!(!src.is_exhausted());
    }

    #[test]
    fn config_new_batch_uses_config_sizes() {
        let (_tmp, cfg) = write_temp(b"AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDD");
        assert_eq!(cfg.batch_size, 256);
        let mut src = FileSource::new(cfg.clone(), FixedLength::new(8));
        src.init().unwrap();
        let batch = cfg.new_batch();
        // Limit capacity for the test by using a small config clone.
        let mut small = cfg.clone();
        small.batch_size = 2;
        small.max_record_size = 64;
        let mut batch2 = small.new_batch();
        let n = src.poll_batch(&mut batch2).unwrap();
        assert_eq!(n, 2);
        assert_eq!(batch.capacity(), 256);
        assert_eq!(batch2.capacity(), 2);
    }

    #[test]
    fn bytes_read_tracks_total() {
        let (_tmp, cfg) = write_temp(b"AAAAAAAABBBBBBBB");
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(16, 64);
        src.poll_batch(&mut batch).unwrap();
        assert_eq!(src.bytes_read(), 16);
    }

    #[test]
    fn uninitialized_returns_error() {
        let cfg = FileConfig::default();
        let mut src = FileSource::new(cfg, FixedLength::new(8));
        let mut batch = RawRecordBatch::new(4, 64);
        assert!(src.poll_batch(&mut batch).is_err());
    }
}
