use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };
    let rest: Vec<OsString> = args.collect();
    let check = rest.iter().any(|arg| arg == "--check");

    match command.to_string_lossy().as_ref() {
        "badges" => badges(check),
        "ripr-pr" => ripr_pr(check),
        "ripr-review-comments" => ripr_review_comments(check),
        "docs-sync" if check => docs_sync_check(),
        "check-file-policy" => check_file_policy(),
        "pr" => pr_check(),
        other => bail!("unknown xtask command `{other}`"),
    }
}

fn print_help() {
    eprintln!("usage: cargo xtask <badges|ripr-pr|ripr-review-comments|docs-sync --check|check-file-policy|pr> [--check]");
}

fn workspace_root_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives under workspace root")
        .to_path_buf()
}

fn badges(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path();
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)?;

    let ripr_plus = ripr_plus_badge(&workspace_root)?;
    validate_shields_badge(&ripr_plus, Some("ripr+"))?;
    write_json_pretty(&target_dir.join("ripr-plus.json"), &ripr_plus)?;

    if check {
        compare_files(
            &workspace_root
                .join(BADGE_ENDPOINT_DIR)
                .join("ripr-plus.json"),
            &target_dir.join("ripr-plus.json"),
        )?;
        println!("badges: committed endpoints are current");
        return Ok(());
    }

    let committed_dir = workspace_root.join(BADGE_ENDPOINT_DIR);
    fs::create_dir_all(&committed_dir)?;
    fs::copy(
        target_dir.join("ripr-plus.json"),
        committed_dir.join("ripr-plus.json"),
    )?;
    println!("badges: refreshed public endpoint JSON under badges/");
    Ok(())
}

fn ripr_plus_badge(workspace_root: &Path) -> Result<ShieldsEndpointBadge> {
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg("repo-badge-plus-shields")
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to run `{ripr_bin}`; install ripr or set RIPR_BIN"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} repo-badge-plus-shields failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{ripr_bin} emitted invalid Shields endpoint JSON"))
}

fn validate_shields_badge(
    badge: &ShieldsEndpointBadge,
    expected_label: Option<&str>,
) -> Result<()> {
    if badge.schema_version != 1 {
        bail!("badge `{}` has unsupported schemaVersion", badge.label);
    }
    if let Some(expected_label) = expected_label {
        if badge.label != expected_label {
            bail!(
                "badge label drifted: got `{}`, expected `{expected_label}`",
                badge.label
            );
        }
    }
    if badge.message.trim().is_empty() {
        bail!("badge `{}` has empty message", badge.label);
    }
    if badge.color.trim().is_empty() {
        bail!("badge `{}` has empty color", badge.label);
    }
    Ok(())
}

fn write_json_pretty(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, [bytes, b"\n".to_vec()].concat())?;
    Ok(())
}

fn compare_files(committed: &Path, generated: &Path) -> Result<()> {
    let committed_bytes = fs::read(committed)
        .with_context(|| format!("missing committed badge endpoint `{}`", committed.display()))?;
    let generated_bytes = fs::read(generated)
        .with_context(|| format!("missing generated badge endpoint `{}`", generated.display()))?;
    if committed_bytes != generated_bytes {
        bail!(
            "badge endpoint drift: `{}` differs from `{}`; run `cargo xtask badges`",
            committed.display(),
            generated.display()
        );
    }
    Ok(())
}

fn ripr_pr(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path();
    let out_dir = workspace_root.join(RIPR_PR_DIR);
    if check {
        validate_ripr_pr_contract(&out_dir)
    } else {
        fs::create_dir_all(&out_dir)?;
        run_ripr_capture(
            &workspace_root,
            &["check", "--root", ".", "--format", "repo-exposure-json"],
            &out_dir.join("repo-exposure.json"),
        )?;
        run_ripr_capture(
            &workspace_root,
            &["check", "--root", ".", "--format", "repo-exposure-md"],
            &out_dir.join("repo-exposure.md"),
        )?;
        validate_ripr_pr_contract(&out_dir)
    }
}

fn ripr_review_comments(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path();
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let json_path = out_dir.join("comments.json");
    let md_path = out_dir.join("comments.md");
    if check {
        validate_json_file(&json_path)?;
        validate_nonempty_file(&md_path)?;
        return Ok(());
    }

    fs::create_dir_all(&out_dir)?;
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let status = Command::new(&ripr_bin)
        .args([
            "review-comments",
            "--root",
            ".",
            "--base",
            "origin/main",
            "--head",
            "HEAD",
            "--out",
        ])
        .arg(&json_path)
        .current_dir(&workspace_root)
        .status()
        .with_context(|| format!("failed to run `{ripr_bin}`; install ripr or set RIPR_BIN"))?;
    if !status.success() {
        bail!("{ripr_bin} review-comments failed");
    }
    validate_json_file(&json_path)?;
    validate_nonempty_file(&md_path)?;
    Ok(())
}

fn run_ripr_capture(workspace_root: &Path, args: &[&str], output_path: &Path) -> Result<()> {
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to run `{ripr_bin}`; install ripr or set RIPR_BIN"))?;
    if !output.status.success() {
        bail!(
            "{ripr_bin} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(output_path, output.stdout)?;
    Ok(())
}

fn validate_ripr_pr_contract(out_dir: &Path) -> Result<()> {
    validate_json_file(&out_dir.join("repo-exposure.json"))?;
    validate_nonempty_file(&out_dir.join("repo-exposure.md"))?;
    println!("ripr-pr: output contract is valid");
    Ok(())
}

fn validate_json_file(path: &Path) -> Result<()> {
    let bytes =
        fs::read(path).with_context(|| format!("missing required file `{}`", path.display()))?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .with_context(|| format!("invalid JSON in `{}`", path.display()))?;
    Ok(())
}

fn validate_nonempty_file(path: &Path) -> Result<()> {
    let bytes =
        fs::read(path).with_context(|| format!("missing required file `{}`", path.display()))?;
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        bail!("required file `{}` is empty", path.display());
    }
    Ok(())
}

fn docs_sync_check() -> Result<()> {
    validate_nonempty_file(&workspace_root_path().join("docs/VERIFICATION.md"))?;
    println!("docs-sync: verification docs are present");
    Ok(())
}

fn check_file_policy() -> Result<()> {
    println!("check-file-policy: no non-Rust file policy is configured for this repository");
    Ok(())
}

fn pr_check() -> Result<()> {
    docs_sync_check()?;
    println!("pr: local PR checks passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripr_plus_badge_shape_is_stable() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };
        validate_shields_badge(&badge, Some("ripr+")).unwrap();
    }
}
