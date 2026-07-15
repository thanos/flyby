# Decoder

A `Decoder` converts a raw byte slice from a `Source` into a typed
`Message`. It is the only place supplier-specific wire formats should
live.

- `Ok(Some(msg))` — decoded successfully.
- `Ok(None)` — drop/filter this frame.
- `Err` — genuine decode failure.

Input is assumed to be one complete frame; framing belongs to the
source (or a storage framer).

See `flyby_core::Decoder` via `cargo doc -p flyby-core --open`.
