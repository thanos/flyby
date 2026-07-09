//! Benchmark for the FlyBy builder skeleton.
//!
//! Run with:
//!
//! ```sh
//! cargo bench -p flyby-benches --bench builder
//! ```
//!
//! At this stage the builder is a skeleton, so the benchmark simply
//! measures construction + validation overhead. Real pipeline benchmarks
//! arrive with the memory and networking parts of the specification.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use flyby::prelude::*;

fn bench_builder(c: &mut Criterion) {
    c.bench_function("builder_memory", |b| {
        b.iter(|| {
            let result = FlyBy::builder().source().memory().placement().run::<()>();
            black_box(result)
        });
    });
}

criterion_group!(g, bench_builder);
criterion_main!(g);
