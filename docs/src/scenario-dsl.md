# FlyScenario DSL

Declarative scenario language for the FlyBy simulator. Authors write
version-controlled TOML (optional Rhai `[script]`) instead of Rust.
Documents compile onto the same [`SimScheduler`](./simulator.md) used by
built-in presets.

**Hard rule:** the language is backend-independent. Speak in *source /
ring / consumer / clock / faults* — never AF_XDP, DPDK, io_uring, or SPDK.

Results are always **simulated**. `scenario.simulated` must be `true`
(the default); the compiler rejects `false`.

## Quick start

```bash
cargo run -p flyby-simulator --bin flyby-sim -- run scenarios/constant_rate.fly.toml
cargo run -p flyby-simulator --bin flyby-sim -- tui scenarios/market_open_lossy.fly.toml
```

A path ending in `.toml` / `.fly.toml` (or containing `/`) is treated as a
DSL file; bare names resolve built-in Rust presets.

## Document shape

```text
scenario
├── [scenario]     meta (name, duration, clock, tick, mode)
├── [[nic]]…       synthetic packet sources
├── [[pcap]]…      classic pcap replay
├── [[storage]]…   parsed today; not yet wired into the scheduler
├── [fabric]       virtual shared-memory ring
├── [[consumer]]…  ring drains
├── [[timeline]]…  timed mutations
├── [edu] / [trace]
└── [script]       optional Rhai → more timeline actions
```

## Durations

Human strings parse to nanoseconds: `250ns`, `500us`, `1ms`, `1.5ms`, `5s`.
Bare integers are nanoseconds. Underscores in numbers are allowed (`1_000ms`).

## `[scenario]`

| Field | Default | Notes |
|---|---|---|
| `name` | required | Short snake_case id |
| `description` | `""` | Human-readable |
| `duration` | `"1s"` | Virtual (or real) run length |
| `tick` | `"1ms"` | Scheduler tick |
| `clock` | `"virtual"` | `virtual` \| `realtime` |
| `mode` | `"trace"` | `benchmark` \| `edu` \| `trace` |
| `seed` | `0` | Determinism root for faults / Gaussian |
| `simulated` | `true` | Must stay `true` |

`mode = "edu"` starts paused (drive via TUI / `EduControls`).
`mode = "benchmark"` prefers quieter event sinks.

## `[[nic]]`

| Field | Default | Notes |
|---|---|---|
| `name` | required | Timeline target |
| `batch_size` | `64` | Max packets per poll |
| `udp_port` | `9000` | Synthetic UDP dest |
| `traffic` | see below | Pattern |
| `payload` | `fixed_seq` | Generator |
| `fault` | clean | Drop / corrupt / spike |

### Traffic (`[nic.traffic]`)

| `pattern` | Fields |
|---|---|
| `fixed` | `pps` |
| `burst` | `burst_size`, `gap` |
| `gaussian` | `mean_pps`, `std_pps`, `seed` |
| `full` | (saturate batch every tick) |

### Payload (`[nic.payload]`)

| `kind` | Fields |
|---|---|
| `fixed_seq` | `size` |
| `random` | `size`, `seed` |
| `gaussian_size` | `mean`, `std`, `max`, `seed` |
| `protocol` | `proto` = `market_quote` \| `fix_quote`, `symbol` |
| `custom` | **Rust-only** — rejected in TOML |

### Fault (`[nic.fault]`)

| Field | Notes |
|---|---|
| `drop_rate` | `[0,1]` |
| `corrupt_rate` | `[0,1]` |
| `latency_spike_rate` | `[0,1]` |
| `latency_spike` | duration string |
| `seed` | overrides scenario seed |
| `malformed_rate` | reserved (future) |

## `[[pcap]]`

| Field | Default | Notes |
|---|---|---|
| `name` | required | |
| `path` | required | Relative to the scenario file |
| `replay` | `"full"` | `full` \| `original` \| `scaled` \| `burst` \| `single_step` |
| `scale` | `1.0` | For `scaled` |
| `loop` | `false` | Wrap when exhausted |
| `fault` | clean | Same shape as NIC faults |

## `[[storage]]`

Parsed for forward compatibility. Storage-only documents are rejected at
build time; mixed documents ignore storage until the scheduler wires
`VirtualStorageSource`. Use the Rust API today for storage fault demos.

## `[fabric]` / `[[consumer]]`

```toml
[fabric]
name      = "ring0"
slots     = 4096
max_frame = 128          # optional; inferred from NIC payloads

[[consumer]]
name          = "c0"
max_per_drain = "unlimited"   # or a positive integer
```

If consumers or NICs are present and `[fabric]` is omitted, a default
`ring0` / `c0` pair is synthesised.

## `[[timeline]]`

Timed mutations applied when virtual time reaches `at`:

| `action` | Required | Effect |
|---|---|---|
| `set_traffic` | `nic`, pattern fields | Hot-swap NIC traffic |
| `set_fault` | `nic`, fault fields | Hot-swap fault policy |
| `slow_consumer` | `consumer`, `max_per_drain` | Change drain budget |

```toml
[[timeline]]
at = "200ms"
action = "set_fault"
nic = "nic0"
drop_rate = 0.05

[[timeline]]
at = "500ms"
action = "set_traffic"
nic = "nic0"
pattern = "fixed"
pps = 20000

[[timeline]]
at = "800ms"
action = "slow_consumer"
consumer = "c0"
max_per_drain = 4
```

## `[edu]` / `[trace]`

```toml
[edu]
paused_start = true
breakpoint_tick = 100
# breakpoint_ns = "10ms"

[trace]
events = true
metrics = true
```

## `[script]` (Rhai)

Scripts run **once at compile time** and only append timeline actions.
They never touch hardware backends.

```toml
[script]
engine = "rhai"
source = '''
  for i in 0..10 {
      at(ms(i * 100));
      let n = nic("nic0");
      n.drop_rate = 0.05 * i;
  }
  schedule_slow_consumer(ms(500), "c0", 8);
'''
# or: path = "ramp.rhai"
```

### API surface

| API | Meaning |
|---|---|
| `ns` / `us` / `ms` / `s` | Time helpers → nanoseconds |
| `at(t)` | Set current schedule time for subsequent mutations |
| `nic(name)` / `consumer(name)` | Proxies (bind with `let` before property sets) |
| `n.drop_rate = x` | Schedule `set_fault` |
| `n.set_fixed(pps)` / `n.set_burst(n, gap_ns)` | Schedule traffic |
| `c.max_per_drain = n` | Schedule slow consumer (`≤0` → unlimited) |
| `schedule_fault(at, nic, rate)` | Explicit fault |
| `schedule_fixed(at, nic, pps)` | Explicit fixed traffic |
| `schedule_slow_consumer(at, name, max)` | Explicit consumer budget |

Rhai cannot assign properties on temporaries — write
`let n = nic("nic0"); n.drop_rate = …`, not `nic("nic0").drop_rate = …`.

## End-to-end example

See [`scenarios/market_open_lossy.fly.toml`](../../scenarios/market_open_lossy.fly.toml)
and [`scenarios/rhai_drop_ramp.fly.toml`](../../scenarios/rhai_drop_ramp.fly.toml).

```toml
[scenario]
name        = "demo"
description = "Fixed rate with mid-run loss"
duration    = "500ms"
tick        = "1ms"
clock       = "virtual"
mode        = "trace"
seed        = 1
simulated   = true

[[nic]]
name       = "nic0"
batch_size = 64

[nic.traffic]
pattern = "fixed"
pps     = 10000

[nic.payload]
kind = "fixed_seq"
size = 8

[fabric]
name  = "ring0"
slots = 1024

[[consumer]]
name = "c0"
max_per_drain = "unlimited"

[[timeline]]
at = "100ms"
action = "set_fault"
nic = "nic0"
drop_rate = 0.1
```

## Mapping Part VI §14

| Capability | DSL |
|---|---|
| create NIC | `[[nic]]` |
| generate packets | `[nic.traffic]` + `[nic.payload]` |
| inject failures | `[nic.fault]` / `set_fault` / Rhai |
| schedule bursts | `pattern = "burst"` + timeline / script |
| delay consumer | `[[consumer]]` / `slow_consumer` |
| replay capture | `[[pcap]]` |
| backend independent | no hardware verbs |

## Related

- [Simulator overview](./simulator.md)
- [Medium articles](./articles.md)
- Built-in Rust presets: `Scenario::by_name` in `flyby-simulator`
