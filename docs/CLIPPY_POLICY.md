# Clippy and Policy Governance

Runbook treats linting as platform infrastructure, not as a local style file. The
workspace uses one strict lint baseline for production code, tests, examples, and
`xtask` so dangerous code shapes are blocked before review.

## Baseline contract

The root `Cargo.toml` owns the active workspace lint block. The baseline is
organized around these guarantees:

- **Panic-free workspace:** `panic!`, `unreachable!`, `todo!`, `unimplemented!`,
  `unwrap`, and `expect` are denied in production and tests.
- **AST/parser/string safety:** string slicing, direct indexing/slicing, and
  UTF-8 boundary hazards are denied.
- **Silent-failure prevention:** ignored futures, ignored must-use values,
  ignored `Result::ok`, ignored `map_err`, and result-state assertions are
  denied.
- **Suppression governance:** broad `#[allow]` attributes are rejected; justified
  local suppressions must use `#[expect(..., reason = "...")]`.
- **Reviewability lints:** format, allocation, iterator, control-flow, API, and
  error-documentation lints are enabled as deny or warn depending on risk.

Every workspace member must inherit the root lint policy with:

```toml
[lints]
workspace = true
```

## No test carveouts

This repository does not allow Clippy test carveouts such as:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-indexing-slicing-in-tests = true
allow-dbg-in-tests = true
```

Tests should return `Result` and use explicit checked assertions/helpers instead
of panic-driven setup.

## Policy ledgers

The machine-readable policy files live under `policy/`:

- `policy/clippy-lints.toml` mirrors active lints and tracks planned Rust 1.94
  and 1.95 flips before the MSRV bump.
- `policy/clippy-debt.toml` records temporary lint exceptions. Each debt entry
  needs `lint`, `path`, `owner`, `reason`, and `expires`.
- `policy/no-panic-allowlist.toml` records semantic panic-family exceptions if
  any are temporarily approved.
- `policy/non-rust-allowlist.toml` records non-Rust files that are intentionally
  part of the repo surface.

Debt is allowed only as explicit, reviewed, expiring policy data. Silent debt is
not allowed.

## Suppression style

Use narrow `#[expect]` suppressions only when the policy ledger explains the debt
or when the code has a local reason that reviewers can validate:

```rust
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded counter is guarded by the protocol maximum before addition"
)]
fn advance_counter(current: u16) -> u16 {
    current + 1
}
```

Do not use broad `#[allow]` attributes or blanket category suppressions.

## Xtask gates

Use the policy gates before review:

```sh
cargo xtask check-lint-policy
cargo xtask check-file-policy
cargo xtask check-no-panic-family
cargo xtask policy-report
```

`check-lint-policy` verifies MSRV consistency, member lint inheritance, active and
planned lint ledger consistency, the absence of test carveouts, suppression style,
and non-expired debt. `check-file-policy` verifies that non-Rust files are covered
by structured TOML receipts. `check-no-panic-family` verifies panic-family calls
against the semantic allowlist.
