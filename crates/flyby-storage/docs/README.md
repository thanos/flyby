# flyby-storage

Storage backends for FlyBy.

## Status

| Component | Status |
|-----------|--------|
| `FileSource` + framing | **Implemented** |
| `ReplayEngine` | **Implemented** (timing helper; not auto-wired into FileSource) |
| io_uring | Stub (`io_uring` feature) — returns `NotImplemented` |
| SPDK | Stub (`spdk` feature) — returns `NotImplemented` |

Real io_uring/SPDK bindings follow ADR-0005 / ADR-0006.
