# Encode

`Encode` serialises a typed message into bytes for sinks that store or
forward raw payloads (e.g. shared memory).

Not every `Message` needs `Encode`; sinks that write bytes require both
bounds. Round-trip with a paired `Decoder` is the recommended test.

See `flyby_core::Encode` via `cargo doc -p flyby-core --open`.
