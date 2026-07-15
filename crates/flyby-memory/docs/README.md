# flyby-memory

Shared-memory sink for FlyBy. Default backend, enabled by the `memory`
feature on the `flyby` facade.

## Status

**Implemented (v0.1):** anonymous mmap SPSC ring, fixed slot layout,
`SharedMemorySink` write path, `pop` for same-process consumers.

**Planned:** file-backed multi-process regions, `Producer`/`Consumer`
split, MPSC.
