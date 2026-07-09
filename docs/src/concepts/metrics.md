# Metrics

Every stage reports counters, gauges, and histograms through a
[`MetricsCollector`]. The trait is intentionally minimal so backends can
plug in `prometheus`, `metrics`, or a no-op collector for benchmarks
without coupling to a specific crate.

## Why it exists

"Prefer measurable performance over theoretical optimisation" (design
principle #4) requires that every stage be measurable by default, with
no dependency forced on the core.

## How it works

A collector implements [`MetricsCollector`] with a `record` method
taking a [`MetricKey`], a [`MetricKind`], and a value. The default
[`NullCollector`] does no work and compiles away, so benchmarks stay
clean.

## Where it fits

Alongside every stage; the pipeline forwards samples to the registered
collector.

## When not to use it

- In a hot, allocation-sensitive benchmark. Use [`NullCollector`].

## How to measure it

- The collector itself should add negligible overhead; measure its cost
  against [`NullCollector`] as the baseline.

[`MetricsCollector`]: https://docs.rs/flyby-core/latest/flyby_core/trait.MetricsCollector.html
[`MetricKey`]: https://docs.rs/flyby-core/latest/flyby_core/trait.MetricKey.html
[`MetricKind`]: https://docs.rs/flyby-core/latest/flyby_core/enum.MetricKind.html
[`NullCollector`]: https://docs.rs/flyby-core/latest/flyby_core/struct.NullCollector.html
