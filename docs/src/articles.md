# Medium articles

Part VI asks every Medium post to ship with a reproducible simulator demo:
scenario (or pcap), screenshots, labelled output, and a matching Git tag.

This is a **publishing workflow**, not part of the simulator library API.
Metadata lives under [`articles/`](../../articles/) in the repo root.

## One-command reproduce

From the workspace root:

```bash
./scripts/reproduce-article.sh --list
./scripts/reproduce-article.sh part-vi-simulator-intro
```

The script prints the article banner (tag, assets paths), warns if HEAD is
not on the article tag, then runs `flyby-sim` with the catalogued workload.
Results are always labelled **simulated**.

## Catalog

[`articles/catalog.toml`](../../articles/catalog.toml) maps each slug to:

| Field | Meaning |
|---|---|
| `slug` | CLI / folder id |
| `git_tag` | `medium/<slug>` |
| `workload` | `scenario` or `pcap` |
| `scenario` / `pcap` | what to run |
| `assets_dir` | screenshots + expected-output notes |
| `medium_url` | filled after publish |

## Authoring a new post

1. Implement or pick a scenario / pcap fixture.
2. `cp -R articles/_template articles/<slug>`
3. Append an `[[article]]` block to `catalog.toml`.
4. Capture screenshots into `articles/<slug>/screenshots/`.
5. Run the reproduce script and paste a transcript into `expected-output.md`.
6. After Medium publish, set `medium_url` and tag:  
   `git tag medium/<slug> && git push origin medium/<slug>`

## Seed articles

| Slug | Workload |
|---|---|
| `part-vi-simulator-intro` | `constant_rate` |
| `part-vi-fault-injection` | `packet_loss` |
| `part-vi-protocol-quotes` | `protocol_quotes` |
| `part-vi-pcap-replay` | `simulator/fixtures/udp_quotes.pcap` |

See also [Simulator](./simulator.md) and [FlyScenario DSL](./scenario-dsl.md).

DSL files can also be used as article workloads once the catalog supports
a `dsl` / path entry; today seed articles use built-in scenario names or
pcap paths.
