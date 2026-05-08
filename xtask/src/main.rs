use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    let result = match command.as_str() {
        "check-lint-policy" => check_lint_policy(),
        "check-file-policy" => check_file_policy(),
        "check-no-panic-family" => check_no_panic_family(),
        "policy-report" => policy_report(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!(
            "unknown xtask command `{other}`\n\nRun `cargo xtask --help` for available commands."
        )),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("runbook-rs xtask commands:");
    println!("  check-lint-policy  Verify Clippy/MSRV/debt/suppression policy");
    println!("  check-file-policy  Verify non-Rust policy allowlist metadata");
    println!("  check-no-panic-family  Verify panic-family allowlist receipts");
    println!("  policy-report      Print a compact policy summary");
}

fn check_lint_policy() -> Result<(), String> {
    let root =
        env::current_dir().map_err(|error| format!("failed to read current dir: {error}"))?;
    let mut errors = Vec::new();

    let cargo = read_to_string(root.join("Cargo.toml"), &mut errors);
    let policy = read_to_string(root.join("policy/clippy-lints.toml"), &mut errors);
    let clippy = read_to_string(root.join("clippy.toml"), &mut errors);
    let debt = read_to_string(root.join("policy/clippy-debt.toml"), &mut errors);

    if let (Some(cargo), Some(policy)) = (cargo.as_deref(), policy.as_deref()) {
        let cargo_msrv = table_value(cargo, "workspace.package", "rust-version");
        let policy_msrv = top_level_value(policy, "msrv");
        if cargo_msrv != policy_msrv {
            errors.push(format!(
                "workspace.package.rust-version ({cargo_msrv:?}) must match policy/clippy-lints.toml msrv ({policy_msrv:?})"
            ));
        }

        for member in workspace_members(cargo) {
            let manifest_path = root.join(&member).join("Cargo.toml");
            match fs::read_to_string(&manifest_path) {
                Ok(manifest) => {
                    if table_value(&manifest, "lints", "workspace") != Some("true".to_string()) {
                        errors.push(format!(
                            "workspace member `{member}` must inherit `[lints] workspace = true`"
                        ));
                    }
                }
                Err(error) => errors.push(format!(
                    "failed to read workspace member manifest `{}`: {error}",
                    manifest_path.display()
                )),
            }
        }

        let active = active_lints(policy);
        for lint in &active {
            if !root_lint_matches(cargo, lint) {
                errors.push(format!(
                    "active lint `{}` at level `{}` is missing from root Cargo.toml",
                    lint.name, lint.level
                ));
            }
        }

        for planned in planned_lints(policy) {
            if version_less(
                policy_msrv.as_deref().unwrap_or_default(),
                &planned.activate_when_msrv,
            ) && root_has_lint(cargo, &planned.name)
            {
                errors.push(format!(
                    "planned lint `{}` for MSRV {} must not be active while policy MSRV is {}",
                    planned.name,
                    planned.activate_when_msrv,
                    policy_msrv.as_deref().unwrap_or("<missing>")
                ));
            }
        }

        if !policy.contains("panic_free_tests = true") {
            errors.push("policy/clippy-lints.toml must set panic_free_tests = true".to_string());
        }
        if !policy.contains("allow_test_carveouts = false") {
            errors
                .push("policy/clippy-lints.toml must set allow_test_carveouts = false".to_string());
        }
        if !policy.contains("suppression_style = \"expect-with-reason\"") {
            errors.push(
                "policy/clippy-lints.toml must set suppression_style = \"expect-with-reason\""
                    .to_string(),
            );
        }
    }

    if let Some(clippy) = clippy.as_deref() {
        for carveout in [
            "allow-unwrap-in-tests",
            "allow-expect-in-tests",
            "allow-panic-in-tests",
            "allow-indexing-slicing-in-tests",
            "allow-dbg-in-tests",
        ] {
            if config_sets_true(clippy, carveout) {
                errors.push(format!("clippy.toml must not enable `{carveout}`"));
            }
        }
    }

    if let Some(debt) = debt.as_deref() {
        validate_blocks(
            debt,
            "debt",
            &["lint", "path", "owner", "reason", "expires"],
            "policy/clippy-debt.toml",
            &mut errors,
        );
    }

    check_suppressions(&root, &mut errors);
    check_no_panic_family_inner(&root, &mut errors);
    check_file_policy_inner(&root, &mut errors);

    if errors.is_empty() {
        println!("lint policy ok");
        Ok(())
    } else {
        Err(format_errors("lint policy check failed", errors))
    }
}

fn check_no_panic_family() -> Result<(), String> {
    let root =
        env::current_dir().map_err(|error| format!("failed to read current dir: {error}"))?;
    let mut errors = Vec::new();
    check_no_panic_family_inner(&root, &mut errors);
    if errors.is_empty() {
        println!("no-panic policy ok");
        Ok(())
    } else {
        Err(format_errors("no-panic policy check failed", errors))
    }
}

fn check_no_panic_family_inner(root: &Path, errors: &mut Vec<String>) {
    let path = root.join("policy/no-panic-allowlist.toml");
    let allowlist = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            errors.push(format!("failed to read `{}`: {error}", path.display()));
            return;
        }
    };
    validate_blocks(
        &allowlist,
        "allow",
        &["path", "family", "classification", "owner", "explanation"],
        "policy/no-panic-allowlist.toml",
        errors,
    );
    let allowed = panic_allow_entries(&allowlist);
    let found = panic_findings(root);

    for finding in &found {
        if !allowed.iter().any(|entry| entry.matches(finding)) {
            errors.push(format!(
                "{}:{} has unallowlisted panic-family `{}` in `{}`",
                finding.path, finding.line, finding.family, finding.container
            ));
        }
    }
    for entry in &allowed {
        if !found.iter().any(|finding| entry.matches(finding)) {
            errors.push(format!(
                "policy/no-panic-allowlist.toml has stale selector for `{}` `{}` in `{}`",
                entry.family, entry.callee, entry.path
            ));
        }
    }
}

fn check_file_policy() -> Result<(), String> {
    let root =
        env::current_dir().map_err(|error| format!("failed to read current dir: {error}"))?;
    let mut errors = Vec::new();
    check_file_policy_inner(&root, &mut errors);
    if errors.is_empty() {
        println!("file policy ok");
        Ok(())
    } else {
        Err(format_errors("file policy check failed", errors))
    }
}

fn policy_report() -> Result<(), String> {
    let root =
        env::current_dir().map_err(|error| format!("failed to read current dir: {error}"))?;
    let policy = fs::read_to_string(root.join("policy/clippy-lints.toml"))
        .map_err(|error| format!("failed to read policy/clippy-lints.toml: {error}"))?;
    let debt = fs::read_to_string(root.join("policy/clippy-debt.toml"))
        .map_err(|error| format!("failed to read policy/clippy-debt.toml: {error}"))?;
    let non_rust = fs::read_to_string(root.join("policy/non-rust-allowlist.toml"))
        .map_err(|error| format!("failed to read policy/non-rust-allowlist.toml: {error}"))?;
    let panic = fs::read_to_string(root.join("policy/no-panic-allowlist.toml"))
        .map_err(|error| format!("failed to read policy/no-panic-allowlist.toml: {error}"))?;

    println!("policy report");
    println!("  active lints: {}", active_lints(&policy).len());
    println!("  planned lints: {}", planned_lints(&policy).len());
    println!("  clippy debt entries: {}", count_blocks(&debt, "debt"));
    println!("  panic allow entries: {}", count_blocks(&panic, "allow"));
    println!(
        "  non-rust allow entries: {}",
        count_blocks(&non_rust, "allow")
    );
    Ok(())
}

#[derive(Debug)]
struct LintEntry {
    name: String,
    level: String,
}

#[derive(Debug)]
struct PlannedLint {
    name: String,
    activate_when_msrv: String,
}

fn read_to_string(path: PathBuf, errors: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(&path) {
        Ok(contents) => Some(contents),
        Err(error) => {
            errors.push(format!("failed to read `{}`: {error}", path.display()));
            None
        }
    }
}

fn active_lints(policy: &str) -> Vec<LintEntry> {
    blocks(policy, "lint")
        .into_iter()
        .filter(|block| value_in_block(block, "status") == Some("active".to_string()))
        .filter_map(|block| {
            let name = value_in_block(&block, "name")?;
            let level = value_in_block(&block, "level")?;
            Some(LintEntry { name, level })
        })
        .collect()
}

fn planned_lints(policy: &str) -> Vec<PlannedLint> {
    blocks(policy, "planned")
        .into_iter()
        .filter_map(|block| {
            let name = value_in_block(&block, "name")?;
            let activate_when_msrv = value_in_block(&block, "activate_when_msrv")?;
            Some(PlannedLint {
                name,
                activate_when_msrv,
            })
        })
        .collect()
}

fn root_lint_matches(cargo: &str, lint: &LintEntry) -> bool {
    let Some((section, key)) = lint.name.split_once("::") else {
        return false;
    };
    let table = match section {
        "rust" => "workspace.lints.rust",
        "clippy" => "workspace.lints.clippy",
        _ => return false,
    };
    table_value(cargo, table, key).as_deref() == Some(lint.level.as_str())
}

fn root_has_lint(cargo: &str, lint_name: &str) -> bool {
    let Some((section, key)) = lint_name.split_once("::") else {
        return false;
    };
    let table = match section {
        "rust" => "workspace.lints.rust",
        "clippy" => "workspace.lints.clippy",
        _ => return false,
    };
    table_value(cargo, table, key).is_some()
}

fn workspace_members(cargo: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut in_members = false;
    for raw in cargo.lines() {
        let line = raw.trim();
        if line.starts_with("members") && line.contains('[') {
            in_members = true;
            continue;
        }
        if in_members && line.starts_with(']') {
            break;
        }
        if in_members {
            let value = line.trim_end_matches(',').trim();
            if value.starts_with('"') && value.ends_with('"') {
                members.push(value.trim_matches('"').to_string());
            }
        }
    }
    members
}

fn table_value(contents: &str, table: &str, key: &str) -> Option<String> {
    let mut in_table = false;
    for raw in contents.lines() {
        let line = strip_comment(raw).trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_table = line == format!("[{table}]");
            continue;
        }
        if in_table {
            if let Some(value) = key_value(line, key) {
                return Some(value);
            }
        }
    }
    None
}

fn top_level_value(contents: &str, key: &str) -> Option<String> {
    for raw in contents.lines() {
        let line = strip_comment(raw).trim();
        if line.starts_with('[') {
            return None;
        }
        if let Some(value) = key_value(line, key) {
            return Some(value);
        }
    }
    None
}

fn value_in_block(block: &str, key: &str) -> Option<String> {
    for raw in block.lines() {
        let line = strip_comment(raw).trim();
        if let Some(value) = key_value(line, key) {
            return Some(value);
        }
    }
    None
}

fn key_value(line: &str, key: &str) -> Option<String> {
    let (left, right) = line.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    Some(right.trim().trim_matches('"').to_string())
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn blocks(contents: &str, name: &str) -> Vec<String> {
    let marker = format!("[[{name}]]");
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut active = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == marker {
            if active {
                out.push(current.join("\n"));
                current.clear();
            }
            active = true;
        } else if active && trimmed.starts_with("[[") {
            out.push(current.join("\n"));
            current.clear();
            active = false;
        } else if active {
            current.push(line);
        }
    }
    if active {
        out.push(current.join("\n"));
    }
    out
}

fn count_blocks(contents: &str, name: &str) -> usize {
    contents
        .lines()
        .filter(|line| line.trim() == format!("[[{name}]]"))
        .count()
}

fn validate_blocks(
    contents: &str,
    block_name: &str,
    required: &[&str],
    label: &str,
    errors: &mut Vec<String>,
) {
    for (idx, block) in blocks(contents, block_name).iter().enumerate() {
        for field in required {
            if value_in_block(block, field).is_none() {
                errors.push(format!(
                    "{label} [[{block_name}]] entry {} is missing `{field}`",
                    idx + 1
                ));
            }
        }
        if let Some(expires) = value_in_block(block, "expires") {
            if expires <= "2026-05-06".to_string() {
                errors.push(format!(
                    "{label} [[{block_name}]] entry {} expired on {expires}",
                    idx + 1
                ));
            }
        }
    }
}

fn check_suppressions(root: &Path, errors: &mut Vec<String>) {
    for path in rust_files(root) {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in contents.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[allow") {
                errors.push(format!(
                    "{}:{} uses #[allow]; use #[expect(..., reason = \"...\")] or policy debt instead",
                    display_path(root, &path),
                    idx + 1
                ));
            }
            if trimmed.starts_with("#[expect") && !trimmed.contains("reason") {
                errors.push(format!(
                    "{}:{} uses #[expect] without a reason",
                    display_path(root, &path),
                    idx + 1
                ));
            }
        }
    }
}

fn check_file_policy_inner(root: &Path, errors: &mut Vec<String>) {
    let path = root.join("policy/non-rust-allowlist.toml");
    let allowlist = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            errors.push(format!("failed to read `{}`: {error}", path.display()));
            return;
        }
    };
    validate_blocks(
        &allowlist,
        "allow",
        &["kind", "owner", "reason", "surface", "classification"],
        "policy/non-rust-allowlist.toml",
        errors,
    );

    let entries = file_policy_entries(&allowlist);
    for (idx, entry) in entries.iter().enumerate() {
        if entry.path.is_none() && entry.glob.is_none() {
            errors.push(format!(
                "policy/non-rust-allowlist.toml [[allow]] entry {} needs `path` or `glob`",
                idx + 1
            ));
        }
        if entry.covered_by_count == 0
            && matches!(
                entry.classification.as_deref(),
                Some("production" | "test" | "tooling" | "config")
            )
        {
            errors.push(format!(
                "policy/non-rust-allowlist.toml [[allow]] entry {} needs covered_by commands",
                idx + 1
            ));
        }
    }

    for file in governed_non_rust_files(root) {
        let rel = display_path(root, &file);
        if !entries.iter().any(|entry| entry.matches(&rel)) {
            errors.push(format!(
                "non-Rust governed file `{rel}` is missing from policy/non-rust-allowlist.toml"
            ));
        }
    }
}

#[derive(Debug)]
struct PanicAllowEntry {
    path: String,
    family: String,
    container: String,
    callee: String,
}

impl PanicAllowEntry {
    fn matches(&self, finding: &PanicFinding) -> bool {
        self.path == finding.path
            && self.family == finding.family
            && self.container == finding.container
            && self.callee == finding.callee
    }
}

#[derive(Debug)]
struct PanicFinding {
    path: String,
    family: String,
    container: String,
    callee: String,
    line: usize,
}

fn panic_allow_entries(contents: &str) -> Vec<PanicAllowEntry> {
    blocks(contents, "allow")
        .into_iter()
        .filter_map(|block| {
            Some(PanicAllowEntry {
                path: value_in_block(&block, "path")?,
                family: value_in_block(&block, "family")?,
                container: value_in_block(&block, "container")?,
                callee: value_in_block(&block, "callee")?,
            })
        })
        .collect()
}

fn panic_findings(root: &Path) -> Vec<PanicFinding> {
    let mut out = Vec::new();
    for path in rust_files(root) {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let rel = display_path(root, &path);
        let mut container = "<module>".to_string();
        for (idx, line) in contents.lines().enumerate() {
            if let Some(name) = function_name(line) {
                container = name;
            }
            for (needle, family, callee) in [
                (".unwrap()", "unwrap", "unwrap"),
                (".expect(", "expect", "expect"),
                ("panic!(", "panic", "panic"),
                ("todo!(", "todo", "todo"),
                ("unimplemented!(", "unimplemented", "unimplemented"),
                ("unreachable!(", "unreachable", "unreachable"),
            ] {
                if line.contains(needle) && !line.contains(&format!("\"{needle}\"")) {
                    out.push(PanicFinding {
                        path: rel.clone(),
                        family: family.to_string(),
                        container: container.clone(),
                        callee: callee.to_string(),
                        line: idx + 1,
                    });
                }
            }
        }
    }
    out
}

fn function_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("fn ")
        .or_else(|| trimmed.strip_prefix("async fn "))?;
    let name = rest.split_once('(')?.0.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[derive(Debug)]
struct FilePolicyEntry {
    path: Option<String>,
    glob: Option<String>,
    classification: Option<String>,
    covered_by_count: usize,
}

impl FilePolicyEntry {
    fn matches(&self, rel: &str) -> bool {
        self.path.as_deref() == Some(rel)
            || self
                .glob
                .as_deref()
                .is_some_and(|pattern| glob_matches(pattern, rel))
    }
}

fn file_policy_entries(contents: &str) -> Vec<FilePolicyEntry> {
    blocks(contents, "allow")
        .into_iter()
        .map(|block| FilePolicyEntry {
            path: value_in_block(&block, "path"),
            glob: value_in_block(&block, "glob"),
            classification: value_in_block(&block, "classification"),
            covered_by_count: usize::from(block.contains("covered_by")),
        })
        .collect()
}

fn governed_non_rust_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_files(root, &mut out, |path| {
        let rel = display_path(root, path);
        if rel.starts_with(".git/") || rel.starts_with("target/") {
            return false;
        }
        matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("json" | "yml" | "yaml" | "md" | "sh")
        )
    });
    out
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_files(root, &mut out, |path| {
        let rel = display_path(root, path);
        !rel.starts_with(".git/")
            && !rel.starts_with("target/")
            && path.extension().and_then(|ext| ext.to_str()) == Some("rs")
    });
    out
}

fn collect_files<F>(dir: &Path, out: &mut Vec<PathBuf>, keep: F)
where
    F: Fn(&Path) -> bool + Copy,
{
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if name != ".git" && name != "target" {
                collect_files(&path, out, keep);
            }
        } else if keep(&path) {
            out.push(path);
        }
    }
}

fn glob_matches(pattern: &str, rel: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return rel == prefix || rel.starts_with(&format!("{prefix}/"));
    }
    if let Some((prefix, suffix)) = pattern.split_once("**/*") {
        return rel.starts_with(prefix) && rel.ends_with(suffix);
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        return rel.starts_with(prefix) && rel.ends_with(suffix);
    }
    pattern == rel
}

fn config_sets_true(contents: &str, key: &str) -> bool {
    contents.lines().any(|line| {
        let line = strip_comment(line).trim();
        key_value(line, key).as_deref() == Some("true")
    })
}

fn version_less(left: &str, right: &str) -> bool {
    let parse = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    parse(left) < parse(right)
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn format_errors(title: &str, errors: Vec<String>) -> String {
    let mut message = format!("{title}:\n");
    for error in errors {
        message.push_str("  - ");
        message.push_str(&error);
        message.push('\n');
    }
    message
}
