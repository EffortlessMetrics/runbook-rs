use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";

#[derive(Parser, Debug)]
#[command(about = "Repository automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate public Shields endpoint JSON under badges/.
    Badges(CheckFlag),
    /// Produce or verify PR-scoped RIPR repo exposure evidence.
    RiprPr(CheckFlag),
    /// Produce or verify RIPR review guidance.
    RiprReviewComments(CheckFlag),
    /// Check documentation index links.
    DocsSync(CheckFlag),
    /// Check non-Rust file policy coverage for generated badge endpoints.
    CheckFilePolicy,
    /// Run the fast local PR gate.
    Pr,
}

#[derive(Parser, Debug)]
struct CheckFlag {
    /// Verify generated output/contract without refreshing committed files.
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
        Commands::Badges(args) => badges(args.check),
        Commands::RiprPr(args) => ripr_pr(args.check),
        Commands::RiprReviewComments(args) => ripr_review_comments(args.check),
        Commands::DocsSync(args) => docs_sync(args.check),
        Commands::CheckFilePolicy => check_file_policy(),
        Commands::Pr => pr(),
    }
}

fn workspace_root_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives directly under the workspace root")
        .to_path_buf()
}

fn badges(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path();
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir).context("creating badge target directory")?;

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
    fs::create_dir_all(&committed_dir).context("creating committed badge endpoint directory")?;
    fs::copy(
        target_dir.join("ripr-plus.json"),
        committed_dir.join("ripr-plus.json"),
    )
    .context("refreshing committed ripr+ endpoint")?;

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
        .with_context(|| format!("running {ripr_bin} for repo-scoped badge evidence"))?;

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
    let workspace_root = workspace_root_path();
    let out_dir = workspace_root.join(RIPR_PR_DIR);

    if check {
        validate_ripr_pr_contract(&out_dir)?;
        println!("ripr-pr: output contract is valid");
        return Ok(());
    }

    fs::create_dir_all(&out_dir).context("creating RIPR PR evidence directory")?;
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
    validate_ripr_pr_contract(&out_dir)?;
    println!("ripr-pr: wrote target/ripr/pr evidence");
    Ok(())
}

fn ripr_review_comments(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path();
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let json_path = out_dir.join("comments.json");
    let md_path = out_dir.join("comments.md");

    if check {
        validate_json_file(&json_path)?;
        ensure_nonempty(&md_path)?;
        println!("ripr-review-comments: output contract is valid");
        return Ok(());
    }

    fs::create_dir_all(&out_dir).context("creating RIPR review guidance directory")?;
    let ripr_bin = std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .arg("review-comments")
        .arg("--root")
        .arg(".")
        .arg("--base")
        .arg("origin/main")
        .arg("--head")
        .arg("HEAD")
        .arg("--out")
        .arg(&json_path)
        .current_dir(&workspace_root)
        .output()
        .with_context(|| format!("running {ripr_bin} review-comments"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} review-comments failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    validate_json_file(&json_path)?;
    ensure_nonempty(&md_path)?;
    println!("ripr-review-comments: wrote target/ripr/review guidance");
    Ok(())
}

fn run_ripr_capture(workspace_root: &Path, args: &[&str], output_path: &Path) -> Result<()> {
    let ripr_bin = std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("running {ripr_bin} {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "{ripr_bin} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(output_path, output.stdout)
        .with_context(|| format!("writing {}", output_path.display()))?;
    Ok(())
}

fn docs_sync(_check: bool) -> Result<()> {
    let root = workspace_root_path();
    let docs_readme = root.join("docs/README.md");
    let body = fs::read_to_string(&docs_readme)
        .with_context(|| format!("reading {}", docs_readme.display()))?;
    for required in [
        "architecture.md",
        "protocol.md",
        "ci/coverage.md",
        "VERIFICATION.md",
    ] {
        if !body.contains(required) {
            bail!("docs/README.md does not reference {required}");
        }
    }
    println!("docs-sync: documentation index is current");
    Ok(())
}

fn check_file_policy() -> Result<()> {
    let root = workspace_root_path();
    let badge = root.join("badges/ripr-plus.json");
    if badge.exists() {
        validate_json_file(&badge)?;
    }
    println!("check-file-policy: generated badge endpoints are valid JSON");
    Ok(())
}

fn pr() -> Result<()> {
    let root = workspace_root_path();
    run_command(&root, "cargo", &["test", "--workspace"])?;
    docs_sync(true)?;
    check_file_policy()?;
    println!("pr: fast local gate completed");
    Ok(())
}

fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("running {program} {}", args.join(" ")))?;
    if !status.success() {
        bail!("{program} {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn validate_ripr_pr_contract(out_dir: &Path) -> Result<()> {
    validate_json_file(&out_dir.join("repo-exposure.json"))?;
    ensure_nonempty(&out_dir.join("repo-exposure.md"))?;
    Ok(())
}

fn validate_json_file(path: &Path) -> Result<()> {
    let body = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let _: serde_json::Value = serde_json::from_slice(&body)
        .with_context(|| format!("parsing JSON in {}", path.display()))?;
    Ok(())
}

fn ensure_nonempty(path: &Path) -> Result<()> {
    let body = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if body.trim().is_empty() {
        bail!("{} is empty", path.display());
    }
    Ok(())
}

fn write_json_pretty(path: &Path, value: &ShieldsEndpointBadge) -> Result<()> {
    let body =
        serde_json::to_string_pretty(value).context("serializing badge endpoint JSON")? + "\n";
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn compare_files(committed: &Path, generated: &Path) -> Result<()> {
    let committed_bytes = fs::read(committed)
        .with_context(|| format!("reading committed endpoint {}", committed.display()))?;
    let generated_bytes = fs::read(generated)
        .with_context(|| format!("reading generated endpoint {}", generated.display()))?;
    if committed_bytes != generated_bytes {
        bail!(
            "badge endpoint drift detected for {}; run `cargo xtask badges`",
            committed.display()
        );
    }
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

    #[test]
    fn rejects_empty_badge_message() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: " ".to_string(),
            color: "brightgreen".to_string(),
        };
        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }

    #[test]
    fn rejects_label_drift() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "coverage".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };
        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }
}
