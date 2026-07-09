# Concepts

The FlyBy programming model is a small set of traits in
`flyby-core`. Backends implement these traits; the pipeline composes
them.

| Concept        | Trait                | Role                                            |
|----------------|----------------------|-------------------------------------------------|
| Message        | [`Message`]          | A typed record flowing downstream.              |
| Source         | [`Source`]           | Produces raw bytes or pre-decoded records.      |
| Sink           | [`Sink`]             | Terminal destination for decoded messages.      |
| PreProcessor   | [`PreProcessor`]     | Enrichment / transform before routing.          |
| Placement      | [`Placement`]        | Routes each message to a sink.                  |
| Pipeline       | [`Pipeline`]         | Wires stages together and drives them.          |
| Metrics        | [`MetricsCollector`] | Observability for every stage.                  |

All of them share the [`Lifecycle`] trait (`init` / `run` / `shutdown`).

These traits are expected to evolve, but changes require an
[Architecture Decision Record](../adr/README.md).

[`Message`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Message.html
[`Source`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Source.html
[`Sink`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Sink.html
[`PreProcessor`]: https://docs.rs/flyby-core/latest/flyby_core/trait.PreProcessor.html
[`Placement`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Placement.html
[`Pipeline`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Pipeline.html
[`MetricsCollector`]: https://docs.rs/flyby-core/latest/flyby_core/trait.MetricsCollector.html
[`Lifecycle`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Lifecycle.html
