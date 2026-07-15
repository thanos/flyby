//! Benchmarks for the flyby-net networking subsystem.
//!
//! Run with:
//!
//! ```sh
//! cargo bench -p flyby-benches --bench net
//! ```
//!
//! ## What is measured
//!
//! - Simulator poll throughput at various batch sizes.
//! - Single poll-batch latency.
//! - Batch size distribution (varying `batch_size` config).
//!
//! ## What is NOT measured here
//!
//! AF_XDP and DPDK benchmarks require a real Linux host with a compatible
//! NIC. Those run on a self-hosted runner. See docs/src/benchmarks.md.
//!
//! ## Interpreting results
//!
//! These numbers reflect the simulator overhead, not NIC hardware. They
//! establish a **floor**: any real backend should beat the simulator at
//! equivalent packet sizes because the simulator copies a template into
//! pre-allocated buffers, which is slower than DMA-backed receive.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use flyby_core::Lifecycle;
use flyby_net::{NetworkSource, RawBatch, SimNetConfig, SimulatedNetSource};

// ---------------------------------------------------------------------------
// Throughput: packets per second through poll_batch.
// ---------------------------------------------------------------------------

fn bench_sim_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("net/sim/throughput");

    for &batch_size in &[8usize, 32, 64, 256] {
        let total = batch_size as u64;
        group.throughput(Throughput::Elements(total));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &n| {
                let config = SimNetConfig {
                    batch_size: n,
                    ..SimNetConfig::default()
                };
                let mut src = SimulatedNetSource::new(config);
                src.init().unwrap();
                let mut batch = RawBatch::new(n, 2048);

                b.iter(|| {
                    let count = src.poll_batch(black_box(&mut batch)).unwrap();
                    black_box(count);
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Latency: single poll_batch call with batch_size = 1.
// ---------------------------------------------------------------------------

fn bench_sim_latency(c: &mut Criterion) {
    let config = SimNetConfig {
        batch_size: 1,
        ..SimNetConfig::default()
    };
    let mut src = SimulatedNetSource::new(config);
    src.init().unwrap();
    let mut batch = RawBatch::new(1, 2048);

    c.bench_function("net/sim/latency/single_packet", |b| {
        b.iter(|| {
            src.poll_batch(black_box(&mut batch)).unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// Packet size: same batch_size, varying payload.
// ---------------------------------------------------------------------------

fn bench_sim_packet_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("net/sim/packet_size");

    for &payload in &[8usize, 64, 512, 1400] {
        group.throughput(Throughput::Bytes((payload + 42) as u64 * 32)); // 32 per batch
        group.bench_with_input(BenchmarkId::from_parameter(payload), &payload, |b, &p| {
            let config = SimNetConfig {
                payload_size: p,
                batch_size: 32,
                ..SimNetConfig::default()
            };
            let mut src = SimulatedNetSource::new(config);
            src.init().unwrap();
            let mut batch = RawBatch::new(32, 2048);

            b.iter(|| {
                src.poll_batch(black_box(&mut batch)).unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_sim_throughput,
    bench_sim_latency,
    bench_sim_packet_size,
);
criterion_main!(benches);
