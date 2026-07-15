//! Integration tests: FileSource end-to-end behaviour.
//!
//! Tests cover the full read path: framing → batch fill → EOF handling.
//! These exercise FileSource as a black box through its public API
//! (Lifecycle + StorageSource) without access to internals.
//!
//! Covers:
//! - Fixed-length, delimiter-delimited, and length-prefixed files
//! - Batch size limits (capacity enforcement)
//! - EOF: Stop, Loop, Follow policies
//! - Multi-poll behaviour (resume across calls)
//! - Uninitialized source rejection
//! - Shutdown + reinit cycle

use flyby_core::Lifecycle;
use flyby_storage::{
    Delimiter, EofPolicy, FileConfig, FileSource, FixedLength, LengthPrefixed, PrefixWidth,
    RawRecordBatch, StorageSource,
};
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_file(data: &[u8]) -> (NamedTempFile, PathBuf) {
    let mut f = NamedTempFile::new().expect("tempfile");
    f.write_all(data).expect("write");
    f.flush().expect("flush");
    let path = f.path().to_path_buf();
    (f, path)
}

fn make_config(path: PathBuf) -> FileConfig {
    FileConfig {
        path,
        ..FileConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Fixed-length
// ---------------------------------------------------------------------------

#[test]
fn fixed_reads_all_records_in_one_poll() {
    let record: Vec<u8> = (0..16u8).collect();
    let mut data = Vec::new();
    for _ in 0..8 {
        data.extend_from_slice(&record);
    }
    let (_tmp, path) = write_file(&data);
    let mut src = FileSource::new(make_config(path), FixedLength::new(16));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(32, 32);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 8);
    assert!(src.is_exhausted());

    // Every record contains bytes 0..16
    for (payload, _meta) in batch.records() {
        assert_eq!(payload, record.as_slice());
    }
}

#[test]
fn fixed_splits_across_multiple_polls() {
    let record = vec![0xAAu8; 8];
    let mut data = Vec::new();
    for _ in 0..10 {
        data.extend_from_slice(&record);
    }
    let (_tmp, path) = write_file(&data);
    let cfg = FileConfig {
        path,
        batch_size: 4,
        ..FileConfig::default()
    };
    let mut src = FileSource::new(cfg, FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(4, 16);

    let n1 = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n1, 4);
    assert!(!src.is_exhausted());

    batch.reset();
    let n2 = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n2, 4);
    assert!(!src.is_exhausted());

    batch.reset();
    let n3 = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n3, 2);
    assert!(src.is_exhausted());
}

#[test]
fn fixed_file_offset_increases_per_record() {
    let (_tmp, path) = write_file(&[0u8; 32]); // 4 × 8-byte records
    let mut src = FileSource::new(make_config(path), FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(8, 16);
    src.poll_batch(&mut batch).unwrap();

    let offsets: Vec<u64> = batch.records().map(|(_, m)| m.file_offset).collect();
    assert_eq!(offsets, vec![0, 8, 16, 24]);
}

#[test]
fn fixed_record_index_increments_monotonically() {
    let (_tmp, path) = write_file(&[0u8; 48]); // 6 × 8-byte records
    let mut src = FileSource::new(make_config(path), FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(3, 16);
    src.poll_batch(&mut batch).unwrap();

    let mut batch2 = RawRecordBatch::new(3, 16);
    src.poll_batch(&mut batch2).unwrap();

    let idx1: Vec<u64> = batch.records().map(|(_, m)| m.record_index).collect();
    let idx2: Vec<u64> = batch2.records().map(|(_, m)| m.record_index).collect();
    assert_eq!(idx1, vec![0, 1, 2]);
    assert_eq!(idx2, vec![3, 4, 5]);
}

// ---------------------------------------------------------------------------
// Delimiter-delimited
// ---------------------------------------------------------------------------

#[test]
fn delimiter_reads_newline_records() {
    let data = b"tick,100,42.00\ntick,101,42.01\ntick,102,42.02\n";
    let (_tmp, path) = write_file(data);
    let mut src = FileSource::new(make_config(path), Delimiter::new(b'\n', 1024));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(8, 256);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 3);

    let lines: Vec<&[u8]> = batch.records().map(|(d, _)| d).collect();
    assert_eq!(lines[0], b"tick,100,42.00\n");
    assert_eq!(lines[1], b"tick,101,42.01\n");
    assert_eq!(lines[2], b"tick,102,42.02\n");
}

#[test]
fn delimiter_incomplete_last_line_held_until_more_data() {
    // File ends without a trailing newline — last partial line should not appear
    // as a record (it has no delimiter).
    let data = b"line1\nline2\nincomplete";
    let (_tmp, path) = write_file(data);
    let mut src = FileSource::new(make_config(path), Delimiter::new(b'\n', 1024));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(8, 256);
    let n = src.poll_batch(&mut batch).unwrap();
    // "incomplete" has no newline → framer returns None → not added to batch
    assert_eq!(n, 2, "only 2 complete newline-terminated records");
    assert!(src.is_exhausted());
}

// ---------------------------------------------------------------------------
// Length-prefixed
// ---------------------------------------------------------------------------

#[test]
fn length_prefixed_reads_variable_records() {
    // Build a file: [len=5][hello][len=3][bye]
    let mut data = vec![5u8];
    data.extend_from_slice(b"hello");
    data.push(3u8);
    data.extend_from_slice(b"bye");

    let (_tmp, path) = write_file(&data);
    let mut src = FileSource::new(make_config(path), LengthPrefixed::new(PrefixWidth::U8, 64));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(8, 128);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 2);

    let records: Vec<&[u8]> = batch.records().map(|(d, _)| d).collect();
    // Each record includes the 1-byte length prefix
    assert_eq!(&records[0][1..], b"hello");
    assert_eq!(&records[1][1..], b"bye");
}

// ---------------------------------------------------------------------------
// EOF policies
// ---------------------------------------------------------------------------

#[test]
fn eof_stop_marks_exhausted() {
    let (_tmp, path) = write_file(b"AAAAAAAA");
    let mut src = FileSource::new(make_config(path), FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(8, 16);
    src.poll_batch(&mut batch).unwrap();
    assert!(src.is_exhausted());

    // Further polls return 0 and don't panic
    batch.reset();
    assert_eq!(src.poll_batch(&mut batch).unwrap(), 0);
}

#[test]
fn eof_loop_does_not_exhaust() {
    let (_tmp, path) = write_file(b"RECORD__"); // 1 record
    let cfg = FileConfig {
        path,
        eof_policy: EofPolicy::Loop,
        ..FileConfig::default()
    };
    let mut src = FileSource::new(cfg, FixedLength::new(8));
    src.init().unwrap();

    // Poll enough to cross the loop boundary multiple times
    for _poll in 0..3 {
        let mut batch = RawRecordBatch::new(2, 16);
        let n = src.poll_batch(&mut batch).unwrap();
        assert!(n > 0, "loop policy should always yield records");
        assert!(!src.is_exhausted());
    }
    assert!(src.loop_count() >= 1);
}

#[test]
fn eof_loop_loop_count_tracks_rewinds() {
    let (_tmp, path) = write_file(b"RECORD__"); // 1 record
    let cfg = FileConfig {
        path,
        eof_policy: EofPolicy::Loop,
        ..FileConfig::default()
    };
    let mut src = FileSource::new(cfg, FixedLength::new(8));
    src.init().unwrap();

    // At most one rewind per poll_batch (anti-spin).
    // First poll: emit 1, rewind, emit 1 → loop_count=1.
    let mut batch = RawRecordBatch::new(4, 16);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 2);
    assert_eq!(src.loop_count(), 1);

    // Second poll starts at EOF, rewinds once, emits 1 more.
    batch.reset();
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 1);
    assert_eq!(src.loop_count(), 2);
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn uninitialized_source_returns_error() {
    let cfg = FileConfig::default();
    let mut src = FileSource::new(cfg, FixedLength::new(8));
    let mut batch = RawRecordBatch::new(4, 16);
    assert!(src.poll_batch(&mut batch).is_err());
}

#[test]
fn shutdown_then_reinit_resets_state() {
    let data: Vec<u8> = vec![0u8; 24]; // 3 × 8-byte records
    let (_tmp, path) = write_file(&data);
    let cfg = make_config(path);

    let mut src = FileSource::new(cfg, FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(8, 16);
    let n1 = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n1, 3);
    assert!(src.is_exhausted());

    // Re-init should reset exhausted flag and replay from the beginning.
    src.shutdown().unwrap();
    src.init().unwrap();
    assert!(!src.is_exhausted());

    batch = RawRecordBatch::new(8, 16);
    let n2 = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n2, 3);
}

#[test]
fn nonexistent_file_returns_error_on_init() {
    let cfg = FileConfig {
        path: "/nonexistent/path/that/cannot/exist.bin".into(),
        ..FileConfig::default()
    };
    let mut src = FileSource::new(cfg, FixedLength::new(8));
    assert!(src.init().is_err());
}

// ---------------------------------------------------------------------------
// Corruption
// ---------------------------------------------------------------------------

#[test]
fn corrupt_length_prefix_exceeding_max_returns_error() {
    // U8 prefix says 200 bytes but max is 10 — should return an error
    let data = vec![200u8, 0, 0, 0, 0]; // 200 > max_payload
    let (_tmp, path) = write_file(&data);
    let mut src = FileSource::new(make_config(path), LengthPrefixed::new(PrefixWidth::U8, 10));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(4, 256);
    assert!(
        src.poll_batch(&mut batch).is_err(),
        "oversized payload should be an error"
    );
}

#[test]
fn corrupt_delimiter_record_exceeding_max_returns_error() {
    // 20 bytes with no newline but max_len=10 → error after scanning 10 bytes
    let data = vec![b'x'; 20];
    let (_tmp, path) = write_file(&data);
    let mut src = FileSource::new(make_config(path), Delimiter::new(b'\n', 10));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(4, 256);
    assert!(
        src.poll_batch(&mut batch).is_err(),
        "record exceeding max_len should be an error"
    );
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[test]
fn bytes_read_matches_file_size() {
    let data = vec![0u8; 64]; // exactly 64 bytes
    let (_tmp, path) = write_file(&data);
    let mut src = FileSource::new(make_config(path), FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(16, 16);
    src.poll_batch(&mut batch).unwrap();
    assert_eq!(src.bytes_read(), 64);
}

#[test]
fn record_index_matches_expected_count() {
    let data = vec![0u8; 40]; // 5 × 8-byte records
    let (_tmp, path) = write_file(&data);
    let mut src = FileSource::new(make_config(path), FixedLength::new(8));
    src.init().unwrap();

    let mut batch = RawRecordBatch::new(16, 16);
    src.poll_batch(&mut batch).unwrap();
    assert_eq!(src.record_index(), 5);
}
