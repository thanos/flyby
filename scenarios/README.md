# FlyScenario tutorial files

Declarative simulator scenarios (`.fly.toml`) for tutorials, CI, and Medium
demos. Full language reference: [`docs/src/scenario-dsl.md`](../docs/src/scenario-dsl.md).

| File | Demonstrates |
|---|---|
| `constant_rate.fly.toml` | Minimal NIC + fabric + consumer |
| `market_open_lossy.fly.toml` | Burst traffic + timed faults / slowdown |
| `rhai_drop_ramp.fly.toml` | Rhai `[script]` timeline ramp |

```bash
cargo run -p flyby-simulator --bin flyby-sim -- run scenarios/constant_rate.fly.toml
cargo run -p flyby-simulator --bin flyby-sim -- tui scenarios/market_open_lossy.fly.toml
```

Results are **simulated**.
