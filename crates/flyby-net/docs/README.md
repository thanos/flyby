# flyby-net

Networking backends for FlyBy.

## Status

| Component | Status |
|-----------|--------|
| `SimulatedNetSource` | **Implemented** (always available) |
| `RawBatch` / config types | **Implemented** |
| AF_XDP | Stub (`af_xdp` feature) — returns `NotImplemented` |
| DPDK | Stub (`dpdk` feature) — returns `NotImplemented` |

Real AF_XDP/DPDK bindings are deferred (ADR-0002, ADR-0004).
