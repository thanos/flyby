# Security policy

## Supported versions

While FlyBy is pre-`1.0`, security fixes land on the latest `main` and the
most recent `0.x` release tag when applicable.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Email or privately message the maintainers (see the GitHub org / repo
security advisory flow when enabled). Include:

- affected crate and version / commit,
- description and impact,
- minimal reproduction if possible.

We aim to acknowledge reports within a few business days.

## Scope

In scope: memory safety issues in `unsafe` regions, privilege escalation
via privileged backends, unintentional data disclosure through sinks.

Out of scope: denial-of-service via intentional load (file a performance
issue instead), issues only in unmaintained third-party deps (report
upstream; we will bump when fixed).
