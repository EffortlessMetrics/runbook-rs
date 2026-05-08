# Clippy and policy allowlist governance

Runbook-rs uses the Effortless Metrics Rust policy model: one strict Clippy
baseline for the whole workspace, no test carveouts, and structured policy files
for any temporary exception. The policy is infrastructure, not cleanup: linting
keeps local code shape safe, while follow-on `ripr` evidence shows whether the
behavioral seams are exercised by tests.

## Baseline

The root `Cargo.toml` owns the active workspace lint block. Every crate must use
`[lints] workspace = true` so package-level settings cannot drift from the
workspace policy. The baseline is intentionally panic-free across production and
tests and covers these classes:

- panic-family lints (`unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`,
  `unreachable!`);
- parser, AST, UTF-8, and slice-safety lints;
- silent-failure lints that catch discarded futures, results, errors, and locks;
- async and concurrency footguns;
- unsafe and memory-review lints;
- numeric correctness lints, with some high-churn numeric checks staged at
  `warn` first;
- filesystem, process, path, API, and reviewability lints;
- suppression-governance lints.

The machine-readable source of truth is `policy/clippy-lints.toml`. It records
active lints, the workspace MSRV, policy posture, and planned Rust 1.94 / 1.95
flips before the repository bumps to those compiler versions.

## No test carveouts

Do not add Clippy test carveouts such as:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-indexing-slicing-in-tests = true
allow-dbg-in-tests = true
```

Tests should return `Result` and propagate setup/assertion failures through an
error channel instead of using `unwrap`, `expect`, or panic-driven setup:

```rust
#[test]
fn parses_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::fs::read_to_string("tests/fixtures/input.rs")?;
    let parsed = parse(&fixture)?;

    ensure_eq(parsed.items.len(), 3, "fixture should expose three items")?;

    Ok(())
}
```

## Suppression style

Use narrow `#[expect(..., reason = "...")]` suppressions only when the local code
shape is deliberately reviewed. Do not use broad `#[allow]` attributes for lint
debt. Repo-level temporary Clippy exceptions belong in `policy/clippy-debt.toml`
with a lint name, path, owner, reason, and expiry.

## Panic-family allowlist

`policy/no-panic-allowlist.toml` is reserved for semantic panic-family receipts.
The identity of a panic exception is `path + family + selector`; line and column
are advisory `last_seen` hints only. Any entry must include a human owner,
classification, explanation, and optional expiry.

## Non-Rust file allowlist

Rust is the default implementation language for this repository. Non-Rust files
that are part of product, tooling, tests, examples, configuration, documentation,
or platform integration are tracked in `policy/non-rust-allowlist.toml` with a
path or glob, kind, owner, reason, surface, classification, and coverage command.

## Local gate

Run the policy gate before review:

```sh
cargo xtask check-lint-policy
```

The gate verifies workspace MSRV alignment, crate lint inheritance, active/planned
lint consistency, absence of Clippy test carveouts, structured debt metadata,
structured non-Rust allowlist metadata, and suppression posture.
