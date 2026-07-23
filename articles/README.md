# Medium articles

Publishing hooks for FlyBy Medium posts (Part VI).

Each article is a folder under this directory plus an entry in
[`catalog.toml`](./catalog.toml). Readers reproduce a post with one command
from the repo root:

```bash
./scripts/reproduce-article.sh part-vi-simulator-intro
```

That script:

1. Warns if HEAD is not on the article's Git tag (does not force-checkout)
2. Runs the linked simulator scenario or pcap
3. Points at this folder for screenshots / notes

## Layout

```text
articles/
├── README.md              ← you are here
├── catalog.toml           ← slug → tag / workload / assets
└── <slug>/
    ├── README.md          ← article-facing notes for authors
    ├── expected-output.md ← what the CLI should look like (approx.)
    └── screenshots/       ← PNGs referenced from the Medium post
```

## Author checklist

When drafting a new Medium post:

1. Pick or add a simulator scenario / pcap fixture
2. Copy `articles/_template/` to `articles/<slug>/`
3. Add a row to `catalog.toml`
4. Capture screenshots into `screenshots/`
5. Fill `expected-output.md` from a local reproduce run
6. Publish the Medium draft, then set `medium_url` in the catalog
7. Tag the matching commit: `git tag medium/<slug> && git push origin medium/<slug>`

Do **not** put article metadata in the Rust crates — keep it here.
