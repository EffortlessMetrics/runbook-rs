# Clippy and policy gate

Runbook uses Clippy as a governed engineering surface rather than a local-taste
lint file. The workspace policy has three goals:

1. keep production code and tests panic-free;
2. prevent silent failure, swallowed work, and silent suppression;
3. keep planned Rust lint upgrades visible before the MSRV moves.

## Active workspace baseline

The root `Cargo.toml` owns the active lint block under `[workspace.lints.rust]`
and `[workspace.lints.clippy]`. Workspace members inherit it with:

```toml
[lints]
workspace = true
```

Do not weaken the root baseline in member manifests. If a lint cannot be fixed
in the same change, record explicit temporary debt in `policy/clippy-debt.toml`
instead.

## MSRV

The workspace MSRV is Rust 1.93. The machine-readable ledger in
`policy/clippy-lints.toml` must match `workspace.package.rust-version` so policy
review and compiler upgrades move together.

## No test carveouts

This workspace is panic-free in production code and tests. Do not add Clippy
configuration such as:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-indexing-slicing-in-tests = true
allow-dbg-in-tests = true
```

Prefer tests that return `Result` and use `?` for setup and parsing failures:

```rust
#[test]
fn parses_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::fs::read_to_string("tests/fixtures/input.json")?;
    let parsed = parse(&fixture)?;

    ensure_eq(parsed.items.len(), 3, "fixture should expose three items")?;

    Ok(())
}
```

## Suppression style

Use narrow `#[expect(..., reason = "...")]` suppressions only when the code shape
is intentional, local, and reviewed. Bare `#[allow]` attributes are rejected by
policy checks unless the policy is extended with an explicit reviewed exception.

Good:

```rust
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded page count arithmetic; validated by page_nav_wraps"
)]
let next = current + 1;
```

Bad:

```rust
#[allow(clippy::arithmetic_side_effects)]
let next = current + 1;
```

## Debt ledger

`policy/clippy-debt.toml` is the receipt book for temporary exceptions. Every
entry must have:

- `lint`
- `path`
- `owner`
- `reason`
- `expires`

Expired debt fails `cargo xtask check-lint-policy`.

## Planned lint flips

`policy/clippy-lints.toml` tracks lints planned for Rust 1.94 and 1.95. Planned
lints must stay out of the active root `Cargo.toml` block until the workspace MSRV
is bumped to the activation version.

## Repo-local Clippy config

`clippy.toml` is reserved for repo-specific `disallowed-methods`,
`disallowed-types`, `disallowed-macros`, or similar domain policy. It is not a
place for test carveouts or blanket weakening.

## Policy command

Run:

```shell
cargo xtask check-lint-policy
```

The gate verifies MSRV consistency, lint inheritance, required active lint
sections, forbidden test carveouts, planned-lint timing, suppression style, and
debt entry shape/expiry.
