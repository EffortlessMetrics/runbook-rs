use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";

#[derive(Debug, Parser)]
#[command(author, version, about = "Repository automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Regenerate public Shields endpoint badge JSON.
    Badges(CheckArg),
    /// Produce or validate PR-scoped RIPR repo exposure evidence.
    RiprPr(CheckArg),
    /// Produce or validate PR-scoped RIPR review guidance.
    RiprReviewComments(CheckArg),
    /// Produce the repo-scoped test-efficiency fact source consumed by ripr+.
    TestEfficiencyReport,
    /// Check that documentation index links required docs.
    DocsSync(CheckArg),
    /// Validate non-Rust generated file ownership when a policy exists.
    CheckFilePolicy,
    /// Run the local PR gate used by this repository.
    Pr,
}

#[derive(Clone, Debug, Parser)]
struct CheckArg {
    /// Check committed or generated outputs instead of refreshing them.
    #[arg(long)]
    check: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Badges(args) => badges(args.check),
        Commands::RiprPr(args) => ripr_pr(args.check),
        Commands::RiprReviewComments(args) => ripr_review_comments(args.check),
        Commands::TestEfficiencyReport => test_efficiency_report(),
        Commands::DocsSync(args) => docs_sync(args.check),
        Commands::CheckFilePolicy => check_file_policy(),
        Commands::Pr => pr(),
    }
}

fn badges(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)?;

    ensure_test_efficiency_report(&workspace_root)?;
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

fn ensure_test_efficiency_report(workspace_root: &Path) -> Result<()> {
    let report = workspace_root.join("target/ripr/reports/test-efficiency.json");
    if report.exists() {
        return Ok(());
    }

    write_test_efficiency_report(workspace_root)
}

fn test_efficiency_report() -> Result<()> {
    let workspace_root = workspace_root_path()?;
    write_test_efficiency_report(&workspace_root)?;
    println!("test-efficiency-report: wrote target/ripr/reports/test-efficiency.json");
    Ok(())
}

fn write_test_efficiency_report(workspace_root: &Path) -> Result<()> {
    let report = workspace_root.join("target/ripr/reports/test-efficiency.json");
    if let Some(parent) = report.parent() {
        fs::create_dir_all(parent)?;
    }

    let value = serde_json::json!({
        "schema_version": "0.1",
        "tests": [],
        "metrics": {
            "tests_scanned": 0,
            "reason_counts": {}
        }
    });
    write_json_pretty(&report, &value)
}

fn ripr_plus_badge(workspace_root: &Path) -> Result<ShieldsEndpointBadge> {
    let ripr_bin = std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg("repo-badge-plus-shields")
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to run {ripr_bin}; install ripr or set RIPR_BIN"))?;

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

fn ripr_pr(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_PR_DIR);

    if check {
        check_ripr_pr_contract(&out_dir)?;
        println!("ripr-pr: output contract is current");
        return Ok(());
    }

    fs::create_dir_all(&out_dir)?;
    let ripr_bin = ripr_bin();
    let json_path = out_dir.join("repo-exposure.json");
    let md_path = out_dir.join("repo-exposure.md");

    run_ripr_to_file(
        &ripr_bin,
        &workspace_root,
        &[
            "check",
            "--root",
            workspace_root.as_os_str().to_string_lossy().as_ref(),
            "--format",
            "repo-exposure-json",
        ],
        &json_path,
    )?;
    run_ripr_to_file(
        &ripr_bin,
        &workspace_root,
        &[
            "check",
            "--root",
            workspace_root.as_os_str().to_string_lossy().as_ref(),
            "--format",
            "repo-exposure-md",
        ],
        &md_path,
    )?;

    check_ripr_pr_contract(&out_dir)
}

fn ripr_review_comments(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let json_path = out_dir.join("comments.json");
    let md_path = out_dir.join("comments.md");

    if check {
        check_json_file(&json_path)?;
        check_nonempty_file(&md_path)?;
        println!("ripr-review-comments: output contract is current");
        return Ok(());
    }

    fs::create_dir_all(&out_dir)?;
    let ripr_bin = ripr_bin();
    let base = std::env::var("RIPR_BASE").unwrap_or_else(|_| "origin/main".to_string());
    let head = std::env::var("RIPR_HEAD").unwrap_or_else(|_| "HEAD".to_string());
    let output = Command::new(&ripr_bin)
        .arg("review-comments")
        .arg("--root")
        .arg(&workspace_root)
        .arg("--base")
        .arg(&base)
        .arg("--head")
        .arg(&head)
        .arg("--out")
        .arg(&json_path)
        .current_dir(&workspace_root)
        .output()
        .with_context(|| format!("failed to run {ripr_bin} review-comments"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} review-comments failed for {base}..{head}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    check_json_file(&json_path)?;
    check_nonempty_file(&md_path)?;
    Ok(())
}

fn docs_sync(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let docs_readme = workspace_root.join("docs/README.md");
    let body = fs::read_to_string(&docs_readme)
        .with_context(|| format!("missing {}", display_path(&docs_readme)))?;
    if !body.contains("[VERIFICATION.md](VERIFICATION.md)") {
        bail!("docs/README.md must link docs/VERIFICATION.md");
    }
    if check {
        println!("docs-sync: documentation index is current");
    }
    Ok(())
}

fn check_file_policy() -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let policy = workspace_root.join("policy/non-rust-allowlist.toml");
    if !policy.exists() {
        println!("check-file-policy: no non-Rust allowlist is configured");
        return Ok(());
    }

    let body = fs::read_to_string(&policy)?;
    if !body.contains(r#"glob = "badges/*.json""#) {
        bail!("policy/non-rust-allowlist.toml must own generated badges/*.json endpoints");
    }
    println!("check-file-policy: generated badge endpoints are owned");
    Ok(())
}

fn pr() -> Result<()> {
    let workspace_root = workspace_root_path()?;
    run(Command::new("cargo")
        .arg("test")
        .current_dir(&workspace_root))?;
    docs_sync(true)?;
    badges(true)?;
    check_file_policy()?;
    Ok(())
}

fn run_ripr_to_file(
    ripr_bin: &str,
    workspace_root: &Path,
    args: &[&str],
    path: &Path,
) -> Result<()> {
    let output = Command::new(ripr_bin)
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to run {ripr_bin}"))?;
    if !output.status.success() {
        bail!(
            "{ripr_bin} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(path, output.stdout)?;
    Ok(())
}

fn check_ripr_pr_contract(out_dir: &Path) -> Result<()> {
    check_json_file(&out_dir.join("repo-exposure.json"))?;
    check_nonempty_file(&out_dir.join("repo-exposure.md"))?;
    Ok(())
}

fn check_json_file(path: &Path) -> Result<()> {
    let body = fs::read(path).with_context(|| format!("missing {}", display_path(path)))?;
    if body.is_empty() {
        bail!("{} is empty", display_path(path));
    }
    let _: serde_json::Value = serde_json::from_slice(&body)
        .with_context(|| format!("{} is invalid JSON", display_path(path)))?;
    Ok(())
}

fn check_nonempty_file(path: &Path) -> Result<()> {
    let body =
        fs::read_to_string(path).with_context(|| format!("missing {}", display_path(path)))?;
    if body.trim().is_empty() {
        bail!("{} is empty", display_path(path));
    }
    Ok(())
}

fn write_json_pretty<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let body = serde_json::to_string_pretty(value)? + "\n";
    fs::write(path, body).with_context(|| format!("write {}", display_path(path)))?;
    Ok(())
}

fn compare_files(committed: &Path, generated: &Path) -> Result<()> {
    let committed_body = fs::read(committed).with_context(|| {
        format!(
            "missing committed badge endpoint {}",
            display_path(committed)
        )
    })?;
    let generated_body = fs::read(generated).with_context(|| {
        format!(
            "missing generated badge endpoint {}",
            display_path(generated)
        )
    })?;
    if committed_body != generated_body {
        bail!(
            "badge endpoint drift: {} differs from {}; run `cargo xtask badges`",
            display_path(committed),
            display_path(generated)
        );
    }
    Ok(())
}

fn run(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {program}"))?;
    if !status.success() {
        bail!("{program} failed with {status}");
    }
    Ok(())
}

fn ripr_bin() -> String {
    std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string())
}

fn workspace_root_path() -> Result<PathBuf> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .output()
        .context("failed to run cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let root = metadata
        .get("workspace_root")
        .and_then(|value| value.as_str())
        .context("cargo metadata did not include workspace_root")?;
    Ok(PathBuf::from(root))
}

fn display_path(path: &Path) -> String {
    workspace_root_path()
        .ok()
        .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
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

    #[test]
    fn badge_shape_rejects_wrong_label() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "coverage".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }
}
