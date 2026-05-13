use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";
const DEFAULT_BASE: &str = "origin/main";
const DEFAULT_HEAD: &str = "HEAD";

#[derive(Parser, Debug)]
#[command(author, version, about = "Repository automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Regenerate public Shields endpoint badge JSON.
    Badges(CheckFlag),
    /// Produce or validate PR-scoped RIPR repository exposure evidence.
    RiprPr(CheckFlag),
    /// Produce or validate RIPR review guidance comments.
    RiprReviewComments(CheckFlag),
    /// Check repository documentation links and generated surfaces.
    DocsSync(CheckFlag),
    /// Check non-Rust generated file policy surfaces.
    CheckFilePolicy,
    /// Fast local PR gate used by contributors and CI.
    Pr,
}

#[derive(Parser, Debug, Default)]
struct CheckFlag {
    /// Check generated output instead of refreshing committed files.
    #[arg(long)]
    check: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
        CommandKind::Badges(args) => badges(args.check),
        CommandKind::RiprPr(args) => ripr_pr(args.check),
        CommandKind::RiprReviewComments(args) => ripr_review_comments(args.check),
        CommandKind::DocsSync(args) => docs_sync(args.check),
        CommandKind::CheckFilePolicy => check_file_policy(),
        CommandKind::Pr => pr(),
    }
}

fn badges(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
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

fn ripr_pr(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let pr_dir = workspace_root.join(RIPR_PR_DIR);
    let json_path = pr_dir.join("repo-exposure.json");
    let md_path = pr_dir.join("repo-exposure.md");

    if check {
        validate_json_file(&json_path)?;
        require_nonempty(&md_path)?;
        println!("ripr-pr: output contract is valid");
        return Ok(());
    }

    fs::create_dir_all(&pr_dir)?;
    let ripr_bin = ripr_bin();
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(&workspace_root)
        .arg("--format")
        .arg("repo-exposure")
        .arg("--out")
        .arg(&json_path)
        .current_dir(&workspace_root)
        .output()
        .with_context(|| format!("failed to run {ripr_bin}; install ripr or set RIPR_BIN"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} repo-exposure failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if !json_path.exists() && !output.stdout.is_empty() {
        fs::write(&json_path, &output.stdout)?;
    }
    validate_json_file(&json_path)?;

    if !md_path.exists() {
        fs::write(
            &md_path,
            "# RIPR PR Evidence\n\nGenerated by `cargo xtask ripr-pr`. See `repo-exposure.json` for the machine-readable receipt.\n",
        )?;
    }
    require_nonempty(&md_path)?;
    println!("ripr-pr: wrote target/ripr/pr evidence");
    Ok(())
}

fn ripr_review_comments(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let review_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let json_path = review_dir.join("comments.json");
    let md_path = review_dir.join("comments.md");

    if check {
        validate_json_file(&json_path)?;
        require_nonempty(&md_path)?;
        println!("ripr-review-comments: output contract is valid");
        return Ok(());
    }

    fs::create_dir_all(&review_dir)?;
    let ripr_bin = ripr_bin();
    let base = std::env::var("RIPR_BASE").unwrap_or_else(|_| DEFAULT_BASE.to_string());
    let head = std::env::var("RIPR_HEAD").unwrap_or_else(|_| DEFAULT_HEAD.to_string());
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
        .with_context(|| format!("failed to run {ripr_bin}; install ripr or set RIPR_BIN"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} review-comments failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    validate_json_file(&json_path)?;
    if !md_path.exists() {
        fs::write(
            &md_path,
            "# RIPR Review Guidance\n\nNo line-placeable RIPR guidance was emitted.\n",
        )?;
    }
    require_nonempty(&md_path)?;
    println!("ripr-review-comments: wrote target/ripr/review guidance");
    Ok(())
}

fn docs_sync(check: bool) -> Result<()> {
    let root = workspace_root_path()?;
    let required = [
        "README.md",
        "badges/README.md",
        "docs/README.md",
        "docs/VERIFICATION.md",
        "docs/ci/coverage.md",
    ];
    for rel in required {
        require_nonempty(&root.join(rel))
            .with_context(|| format!("missing required docs file {rel}"))?;
    }
    let docs_readme = fs::read_to_string(root.join("docs/README.md"))?;
    if !docs_readme.contains("VERIFICATION.md") || !docs_readme.contains("ci/coverage.md") {
        bail!("docs/README.md is missing required verification or coverage entries");
    }
    if check {
        println!("docs-sync: documentation index is current");
    }
    Ok(())
}

fn check_file_policy() -> Result<()> {
    let root = workspace_root_path()?;
    for rel in ["badges/ripr-plus.json"] {
        validate_json_file(&root.join(rel))?;
    }
    println!("file-policy: generated badge endpoints are accounted for");
    Ok(())
}

fn pr() -> Result<()> {
    Command::new("cargo")
        .arg("test")
        .arg("--workspace")
        .current_dir(workspace_root_path()?)
        .status()
        .context("failed to run cargo test --workspace")?
        .success()
        .then_some(())
        .context("cargo test --workspace failed")?;
    docs_sync(true)?;
    check_file_policy()?;
    println!("pr: fast gate completed");
    Ok(())
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

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, [bytes, b"\n".to_vec()].concat())?;
    Ok(())
}

fn compare_files(committed: &Path, generated: &Path) -> Result<()> {
    let committed_bytes = fs::read(committed)
        .with_context(|| format!("missing committed badge endpoint {}", committed.display()))?;
    let generated_bytes = fs::read(generated)?;
    if committed_bytes != generated_bytes {
        bail!(
            "badge endpoint drift: {} differs from generated {}",
            committed.display(),
            generated.display()
        );
    }
    Ok(())
}

fn validate_json_file(path: &Path) -> Result<serde_json::Value> {
    require_nonempty(path)?;
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn require_nonempty(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| format!("missing {}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("{} is empty or not a file", path.display());
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
        .context("cargo metadata did not report workspace_root")?;
    Ok(PathBuf::from(root))
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
    fn scanner_safe_badge_shape_is_stable() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "fixtures".to_string(),
            message: "scanner-safe".to_string(),
            color: "brightgreen".to_string(),
        };

        validate_shields_badge(&badge, Some("fixtures")).unwrap();
    }

    #[test]
    fn rejects_wrong_label() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "coverage".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }
}
