# Clippy and Policy Gates

Runbook uses the Effortless Metrics Rust policy model: one workspace lint surface,
structured exception receipts, and xtask checks that keep the policy reviewable.
The goal is not local taste; the goal is a shared platform posture for panic-free
production and test code, parser-safe string/index behavior, silent-failure
prevention, and explicit suppression governance.

## Workspace baseline

The root `Cargo.toml` owns all active Rust and Clippy lint levels under
`[workspace.lints]`. Every crate inherits that block with:

```toml
lints.workspace = true
```

The baseline forbids unsafe code, denies panic-family shapes such as `unwrap`,
`expect`, `panic!`, `todo!`, `unimplemented!`, and `unreachable!`, denies
silent-failure shapes such as ignored futures and ignored `Result` values, and
warns on reviewability lints that reduce allocation noise or unclear control
flow.

## No test carveouts

This repository intentionally treats tests as part of the workspace panic-free
surface. Do not add Clippy carveouts such as:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-indexing-slicing-in-tests = true
allow-dbg-in-tests = true
```

Prefer Result-returning tests and helpers:

```rust
#[test]
fn parses_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::fs::read_to_string("tests/fixtures/input.yaml")?;
    let parsed = parse_fixture(&fixture)?;
    assert_eq!(parsed.items.len(), 3);
    Ok(())
}
```

## Suppression style

Suppressions must be narrow and self-documenting. Use `#[expect(..., reason =
"...")]` only when the code needs a temporary local exception. Do not use
silent `#[allow(...)]` attributes. Longer-lived exceptions belong in
`policy/clippy-debt.toml` with an owner, reason, path, lint, and expiry.

## Machine-readable policy files

- `policy/clippy-lints.toml` is the ledger for active lint levels and planned
  Rust 1.94/1.95 flips.
- `policy/clippy-debt.toml` records temporary Clippy exceptions without weakening
  the global baseline.
- `policy/no-panic-allowlist.toml` records reviewed panic-family migration debt
  using semantic selectors. Identity is path + family + selector; `last_seen` is
  advisory repair context only.
- `policy/non-rust-allowlist.toml` records why non-Rust programming/config files
  exist, who owns them, and what covers them in CI.

## Planned Rust upgrades

The workspace MSRV is 1.93. The policy ledger already tracks planned 1.94 and
1.95 lints so upgrades can flip known checks deliberately instead of discovering
them during a compiler bump.

## Local checks

Run these before opening policy PRs:

```console
cargo fmt
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo xtask check-lint-policy
cargo xtask check-no-panic-family
cargo xtask check-file-policy
cargo xtask policy-report
```

On machines with Rust older than the repository MSRV, install the pinned MSRV or
newer before running Cargo checks.
