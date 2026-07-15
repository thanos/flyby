# Concepts

The FlyBy programming model is a small set of traits in
`flyby-core`. Backends implement these traits; the pipeline composes
them.

| Concept        | Trait                | Role                                            |
|----------------|----------------------|-------------------------------------------------|
| Message        | `Message`            | A typed record flowing downstream.              |
| Source         | `Source`             | Produces raw bytes (batch APIs live in backends). |
| Decoder        | `Decoder`            | Bytes → typed message.                          |
| Encode         | `Encode`             | Message → bytes (for byte sinks).               |
| Sink           | `Sink`               | Terminal destination for decoded messages.      |
| PreProcessor   | `PreProcessor`       | Enrichment / transform before routing.          |
| Placement      | `Placement`          | Routes each message to a sink (1:1 today).      |
| Pipeline       | `Pipeline`           | Wires stages together and drives them.          |
| Metrics        | `MetricsCollector`   | Observability for stages.                       |

**Lifecycle** (`init` / `run` / `shutdown`) is shared by resource-owning
stages: **Source, Sink, and Pipeline**. Pure transforms (PreProcessor,
Placement, Decoder, Encode) and metrics collectors typically do not
implement `Lifecycle`.

These traits evolve via [Architecture Decision Records](../adr/README.md).

API reference: run `cargo doc -p flyby-core --open` (crates are not yet
published to docs.rs).
