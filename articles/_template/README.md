# Article template

Copy this folder to `articles/<slug>/` and edit.

## Medium draft

- Title:
- URL: _(fill after publish)_
- Git tag: `medium/<slug>`
- Scenario / pcap: _(must match `catalog.toml`)_

## Screenshots

Drop PNGs in `screenshots/` and name them clearly, e.g.:

- `01-cli-run.png`
- `02-results.png`

Reference the same filenames in the Medium post.

## Reproduce

From the repo root:

```bash
./scripts/reproduce-article.sh <slug>
```
