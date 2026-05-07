# Coverage

Codecov coverage is Rust execution-surface evidence.

It answers:

> Did tests execute this Rust surface?

It does not answer:

- whether operator-state transitions are correct,
- whether Claude Code hook events are semantically correct,
- whether WebSocket clients behave correctly,
- whether the Logi Actions plugin behaves correctly,
- whether the VS Code extension behaves correctly,
- whether policy checks are complete,
- whether release readiness is proven.

Those are separate proof lanes.

The Coverage workflow runs on:

- push to `main`,
- `workflow_dispatch`,
- PRs labeled `coverage` or `full-ci`.

Codecov comments are disabled. Durable receipts are:

- `coverage.json`,
- `coverage.txt`,
- `lcov.info`,
- the GitHub Actions coverage artifact,
- the Codecov dashboard.
