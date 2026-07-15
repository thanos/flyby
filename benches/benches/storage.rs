//! Benchmarks for the flyby-storage subsystem.
//!
//! Run with:
//!
//! ```sh
//! cargo bench -p flyby-benches --bench storage
//! ```
//!
//! ## What is measured
//!
//! - [`FileSource`] read throughput with fixed-length framing at various
//!   record sizes and batch sizes.
//! - [`FileSource`] latency per `poll_batch` call.
//! - Framing overhead: same read buffer, different framers.
//!
//! ## What is NOT measured here
//!
//! io_uring and SPDK benchmarks are Linux-specific and run on a self-hosted
//! runner.  See docs/src/benchmarks.md.
//!
//! ## Interpreting results
//!
//! These numbers reflect the file I/O stack + framing cost, not storage
//! hardware.  Use them to confirm that framer overhead is negligible compared
//! to read latency, and to detect regressions in the batch-fill loop.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use flyby_core::Lifecycle;
use flyby_storage::{FileConfig, FileSource, FixedLength, RawRecordBatch, StorageSource};
use std::io::Write;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_file(record_size: usize, record_count: usize) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("tempfile");
    let record = vec![0x42u8; record_size];
    for _ in 0..record_count {
        f.write_all(&record).expect("write");
    }
    f.flush().expect("flush");
    f
}

// ---------------------------------------------------------------------------
// Throughput: records/s with fixed-length framing.
// ---------------------------------------------------------------------------

fn bench_file_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage/file/throughput");

    for &record_size in &[16usize, 64, 256, 1024] {
        let record_count = 8192usize;
        let tmp = make_file(record_size, record_count);
        let total_bytes = (record_size * record_count) as u64;
        group.throughput(Throughput::Bytes(total_bytes));

        group.bench_with_input(
            BenchmarkId::from_parameter(record_size),
            &record_size,
            |b, &rs| {
                b.iter(|| {
                    let cfg = FileConfig {
                        path: tmp.path().to_path_buf(),
                        batch_size: 256,
                        ..FileConfig::default()
                    };
                    let mut src = FileSource::new(cfg, FixedLength::new(rs));
                    src.init().unwrap();
                    let mut batch = RawRecordBatch::new(256, rs + 8);
                    let mut total = 0usize;
                    loop {
                        batch.reset();
                        let n = src.poll_batch(black_box(&mut batch)).unwrap();
                        total += n;
                        if src.is_exhausted() {
                            break;
                        }
                    }
                    black_box(total);
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Latency: single poll_batch call.
// ---------------------------------------------------------------------------

fn bench_file_latency(c: &mut Criterion) {
    let record_size = 64usize;
    let tmp = make_file(record_size, 10_000);
    let cfg = FileConfig {
        path: tmp.path().to_path_buf(),
        batch_size: 1,
        ..FileConfig::default()
    };
    let mut src = FileSource::new(cfg, FixedLength::new(record_size));
    src.init().unwrap();
    let mut batch = RawRecordBatch::new(1, record_size + 8);

    c.bench_function("storage/file/latency/single_record", |b| {
        b.iter(|| {
            batch.reset();
            src.poll_batch(black_box(&mut batch)).unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// Batch size impact.
// ---------------------------------------------------------------------------

fn bench_file_batch_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage/file/batch_size");
    let record_size = 64usize;

    for &batch_size in &[1usize, 8, 64, 256] {
        let record_count = batch_size * 4;
        let tmp = make_file(record_size, record_count);
        group.throughput(Throughput::Elements(record_count as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &bs| {
                b.iter(|| {
                    let cfg = FileConfig {
                        path: tmp.path().to_path_buf(),
                        batch_size: bs,
                        ..FileConfig::default()
                    };
                    let mut src = FileSource::new(cfg, FixedLength::new(record_size));
                    src.init().unwrap();
                    let mut batch = RawRecordBatch::new(bs, record_size + 8);
                    let mut total = 0;
                    loop {
                        batch.reset();
                        let n = src.poll_batch(black_box(&mut batch)).unwrap();
                        total += n;
                        if src.is_exhausted() {
                            break;
                        }
                    }
                    black_box(total);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_file_throughput,
    bench_file_latency,
    bench_file_batch_size,
);
criterion_main!(benches);
