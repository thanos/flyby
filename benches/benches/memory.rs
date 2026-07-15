//! Benchmarks for the flyby-memory SPSC ring.
//!
//! Run with:
//!
//! ```sh
//! cargo bench -p flyby-benches --bench memory
//! ```
//!
//! These benchmarks establish the baseline numbers that any future
//! optimisation must improve upon (per the spec: no optimisation without
//! benchmark evidence).

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use flyby_core::Sink;
use flyby_memory::{SharedMemorySink, StubMessage, slot};

// ---------------------------------------------------------------------------
// Throughput: messages per second through push→pop.
// ---------------------------------------------------------------------------

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory/throughput");

    for &slot_count in &[64usize, 256, 1024] {
        group.throughput(Throughput::Elements(slot_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(slot_count),
            &slot_count,
            |b, &n| {
                let mut sink: SharedMemorySink<StubMessage> = SharedMemorySink::new(n, 64).unwrap();
                b.iter(|| {
                    // fill
                    for i in 0..n as u64 {
                        sink.write(&StubMessage { seq: i }).unwrap();
                    }
                    // drain
                    for _ in 0..n {
                        sink.pop(|buf| {
                            black_box(buf);
                        });
                    }
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Latency: single push→pop round-trip.
// ---------------------------------------------------------------------------

fn bench_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory/latency");
    let mut sink: SharedMemorySink<StubMessage> = SharedMemorySink::new(16, 64).unwrap();

    group.bench_function("single_push_pop", |b| {
        b.iter(|| {
            sink.write(black_box(&StubMessage { seq: 0 })).unwrap();
            sink.pop(|buf| {
                black_box(buf);
            });
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Slot size impact: same message, varying slot (and thus cache) footprint.
// ---------------------------------------------------------------------------

fn bench_slot_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory/slot_size");

    for &max_payload in &[32usize, 64, 128, 256] {
        group.bench_with_input(
            BenchmarkId::from_parameter(max_payload),
            &max_payload,
            |b, &mp| {
                let mut sink: SharedMemorySink<StubMessage> =
                    SharedMemorySink::new(256, mp).unwrap();
                b.iter(|| {
                    for i in 0..256u64 {
                        sink.write(&StubMessage { seq: i }).unwrap();
                    }
                    for _ in 0..256 {
                        sink.pop(|buf| {
                            black_box(buf);
                        });
                    }
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Slot encode/decode in isolation (no ring overhead).
// ---------------------------------------------------------------------------

fn bench_slot_encode_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory/slot");
    let mut buf = vec![0u8; 128];
    let payload = [0u8; 64];

    group.bench_function("encode", |b| {
        b.iter(|| {
            let header = slot::SlotHeader::new(1, slot::FLAG_VALID, 42, 1_000_000, 64);
            slot::encode(black_box(&header), black_box(&payload), black_box(&mut buf)).unwrap();
        });
    });

    // Pre-encode once, then bench decode in a loop.
    let header = slot::SlotHeader::new(1, slot::FLAG_VALID, 42, 1_000_000, 64);
    slot::encode(&header, &payload, &mut buf).unwrap();

    group.bench_function("decode", |b| {
        b.iter(|| {
            let result = slot::decode(black_box(&buf));
            black_box(result).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_throughput,
    bench_latency,
    bench_slot_size,
    bench_slot_encode_decode,
);
criterion_main!(benches);
