use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CLIPPY_LINTS: &str = "policy/clippy-lints.toml";
const CLIPPY_DEBT: &str = "policy/clippy-debt.toml";
const CLIPPY_TOML: &str = "clippy.toml";
const NO_PANIC_ALLOWLIST: &str = "policy/no-panic-allowlist.toml";
const NON_RUST_ALLOWLIST: &str = "policy/non-rust-allowlist.toml";

fn main() {
    let result = run();
    if let Err(error) = result {
        eprintln!("xtask failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };

    match command.as_str() {
        "check-lint-policy" => check_lint_policy(),
        "check-file-policy" => check_file_policy(),
        "check-no-panic-family" => check_no_panic_family(),
        "policy-report" => policy_report(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown xtask command `{other}`")),
    }
}

fn print_help() {
    println!("cargo xtask <command>");
    println!("  check-lint-policy      verify Cargo/Clippy policy ledgers");
    println!("  check-file-policy      verify non-Rust file allowlist receipts");
    println!("  check-no-panic-family  verify panic-family allowlist receipts");
    println!("  policy-report          summarize policy surfaces");
}

fn check_lint_policy() -> Result<(), String> {
    let mut errors = Vec::new();
    let cargo = read_to_string("Cargo.toml", &mut errors);
    let ledger = read_to_string(CLIPPY_LINTS, &mut errors);
    let clippy = read_to_string(CLIPPY_TOML, &mut errors);
    let debt = read_to_string(CLIPPY_DEBT, &mut errors);

    if let (Some(cargo), Some(ledger)) = (cargo.as_ref(), ledger.as_ref()) {
        let rust_version = manifest_value(cargo, "[workspace.package]", "rust-version");
        let ledger_msrv = top_level_value(ledger, "msrv");
        if rust_version != ledger_msrv {
            errors.push(format!(
                "workspace.package.rust-version ({}) must match {CLIPPY_LINTS} msrv ({})",
                display_opt(rust_version.as_deref()),
                display_opt(ledger_msrv.as_deref())
            ));
        }

        let active = workspace_lints(cargo);
        let planned = ledger_lints(ledger, "planned");
        let active_ledger = ledger_lints(ledger, "active");

        for (name, level) in &active {
            match active_ledger.get(name) {
                Some(ledger_level) if ledger_level == level => {}
                Some(ledger_level) => errors.push(format!(
                    "active lint `{name}` has level `{level}` in Cargo.toml but `{ledger_level}` in {CLIPPY_LINTS}"
                )),
                None => errors.push(format!(
                    "active lint `{name}` from Cargo.toml is missing from {CLIPPY_LINTS}"
                )),
            }
        }

        for name in active_ledger.keys() {
            if !active.contains_key(name) {
                errors.push(format!(
                    "active lint `{name}` is in {CLIPPY_LINTS} but missing from Cargo.toml"
                ));
            }
        }

        for name in planned.keys() {
            if active.contains_key(name) {
                errors.push(format!(
                    "planned lint `{name}` is active before the recorded MSRV flip"
                ));
            }
        }
    }

    if let Some(cargo) = cargo.as_ref() {
        for manifest in workspace_member_manifests(cargo) {
            let text = read_path_to_string(&manifest, &mut errors);
            if let Some(text) = text {
                if !text.contains("[lints]") || !text.contains("workspace = true") {
                    errors.push(format!(
                        "{} must inherit workspace lints with [lints] workspace = true",
                        manifest.display()
                    ));
                }
            }
        }
    }

    if let Some(clippy) = clippy.as_ref() {
        for carveout in [
            "allow-unwrap-in-tests",
            "allow-expect-in-tests",
            "allow-panic-in-tests",
            "allow-indexing-slicing-in-tests",
            "allow-dbg-in-tests",
        ] {
            if clippy.contains(carveout) {
                errors.push(format!("{CLIPPY_TOML} must not set `{carveout}`"));
            }
        }
    }

    if let Some(debt) = debt.as_ref() {
        validate_debt(debt, &mut errors);
    }

    scan_rust_suppressions(&mut errors)?;
    finish("lint policy", errors)
}

fn check_file_policy() -> Result<(), String> {
    let mut errors = Vec::new();
    let allowlist = read_to_string(NON_RUST_ALLOWLIST, &mut errors);
    let rules = allowlist
        .as_ref()
        .map(|text| parse_file_allowlist(text, &mut errors))
        .unwrap_or_default();

    let files = repo_files()?;
    let mut used = BTreeSet::new();
    for file in files {
        let path = file.to_string_lossy().replace('\\', "/");
        if is_ignored_for_file_policy(&path) || is_rust_source_or_manifest(&path) {
            continue;
        }
        let mut matched = false;
        for rule in &rules {
            if rule.matches(&path) {
                matched = true;
                used.insert(rule.pattern.clone());
                break;
            }
        }
        if !matched {
            errors.push(format!(
                "non-Rust file `{path}` is not covered by {NON_RUST_ALLOWLIST}"
            ));
        }
    }

    for rule in &rules {
        if !used.contains(&rule.pattern) {
            errors.push(format!(
                "non-Rust allowlist entry `{}` did not match any tracked file",
                rule.pattern
            ));
        }
    }

    finish("file policy", errors)
}

fn check_no_panic_family() -> Result<(), String> {
    let mut errors = Vec::new();
    let allowlist = read_to_string(NO_PANIC_ALLOWLIST, &mut errors);
    let allowed = allowlist
        .as_ref()
        .map(|text| parse_panic_allowlist(text, &mut errors))
        .unwrap_or_default();
    let mut seen = BTreeSet::new();

    for file in rust_files()? {
        let text = read_path_to_string(&file, &mut errors);
        let Some(text) = text else { continue };
        let rel = file.to_string_lossy().replace('\\', "/");
        for finding in panic_findings(&rel, &text) {
            if allowed.contains(&finding.identity) {
                seen.insert(finding.identity);
            } else {
                errors.push(format!(
                    "panic-family `{}` at {}:{} needs a semantic receipt in {NO_PANIC_ALLOWLIST}",
                    finding.family, rel, finding.line
                ));
            }
        }
    }

    for identity in &allowed {
        if !seen.contains(identity) {
            errors.push(format!(
                "stale panic allowlist selector `{identity}` did not match a current finding"
            ));
        }
    }

    finish("no-panic family", errors)
}

fn policy_report() -> Result<(), String> {
    let mut errors = Vec::new();
    let lint_text = read_to_string(CLIPPY_LINTS, &mut errors).unwrap_or_default();
    let debt_text = read_to_string(CLIPPY_DEBT, &mut errors).unwrap_or_default();
    let panic_text = read_to_string(NO_PANIC_ALLOWLIST, &mut errors).unwrap_or_default();
    let file_text = read_to_string(NON_RUST_ALLOWLIST, &mut errors).unwrap_or_default();

    println!("policy report");
    println!(
        "  active lints: {}",
        ledger_lints(&lint_text, "active").len()
    );
    println!(
        "  planned lints: {}",
        ledger_lints(&lint_text, "planned").len()
    );
    println!(
        "  clippy debt entries: {}",
        count_tables(&debt_text, "[[debt]]")
    );
    println!(
        "  panic allowlist entries: {}",
        count_tables(&panic_text, "[[allow]]")
    );
    println!(
        "  non-Rust allowlist entries: {}",
        count_tables(&file_text, "[[allow]]")
    );

    finish("policy report", errors)
}

fn read_to_string(path: &str, errors: &mut Vec<String>) -> Option<String> {
    read_path_to_string(Path::new(path), errors)
}

fn read_path_to_string(path: &Path, errors: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            errors.push(format!("failed to read {}: {error}", path.display()));
            None
        }
    }
}

fn finish(name: &str, errors: Vec<String>) -> Result<(), String> {
    if errors.is_empty() {
        println!("{name}: ok");
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn display_opt(value: Option<&str>) -> String {
    value.map_or_else(|| "<missing>".to_string(), ToString::to_string)
}

fn top_level_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            return None;
        }
        if let Some(value) = parse_key_value(trimmed, key) {
            return Some(value);
        }
    }
    None
}

fn manifest_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section;
            continue;
        }
        if in_section {
            if let Some(value) = parse_key_value(trimmed, key) {
                return Some(value);
            }
        }
    }
    None
}

fn parse_key_value(line: &str, key: &str) -> Option<String> {
    let (left, right) = line.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    Some(clean_value(right))
}

fn clean_value(raw: &str) -> String {
    raw.split('#')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches('"')
        .to_string()
}

fn workspace_lints(cargo: &str) -> BTreeMap<String, String> {
    let mut section = String::new();
    let mut lints = BTreeMap::new();
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace.lints.rust]" {
            section = "rust".to_string();
            continue;
        }
        if trimmed == "[workspace.lints.clippy]" {
            section = "clippy".to_string();
            continue;
        }
        if trimmed.starts_with('[') {
            section.clear();
            continue;
        }
        if section.is_empty() || trimmed.starts_with('#') || !trimmed.contains('=') {
            continue;
        }
        if let Some((key, level)) = trimmed.split_once('=') {
            lints.insert(format!("{}::{}", section, key.trim()), clean_value(level));
        }
    }
    lints
}

fn ledger_lints(text: &str, status: &str) -> BTreeMap<String, String> {
    let mut lints = BTreeMap::new();
    let mut in_lint = false;
    let mut name: Option<String> = None;
    let mut level: Option<String> = None;
    let mut entry_status: Option<String> = None;

    for line in text.lines().chain(std::iter::once("[[lint]]")) {
        let trimmed = line.trim();
        if trimmed == "[[lint]]" {
            if in_lint && entry_status.as_deref() == Some(status) {
                if let (Some(entry_name), Some(entry_level)) = (name.take(), level.take()) {
                    lints.insert(entry_name, entry_level);
                }
            }
            in_lint = true;
            name = None;
            level = None;
            entry_status = None;
            continue;
        }
        if !in_lint {
            continue;
        }
        if let Some(value) = parse_key_value(trimmed, "name") {
            name = Some(value);
        } else if let Some(value) = parse_key_value(trimmed, "level") {
            level = Some(value);
        } else if let Some(value) = parse_key_value(trimmed, "status") {
            entry_status = Some(value);
        }
    }

    lints
}

fn workspace_member_manifests(cargo: &str) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
    let mut in_members = false;
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("members") && trimmed.contains('[') {
            in_members = true;
            continue;
        }
        if in_members && trimmed.starts_with(']') {
            break;
        }
        if in_members {
            let member = trimmed.trim_end_matches(',').trim().trim_matches('"');
            if !member.is_empty() {
                manifests.push(PathBuf::from(member).join("Cargo.toml"));
            }
        }
    }
    manifests
}

fn validate_debt(text: &str, errors: &mut Vec<String>) {
    let today = "2026-05-06";
    let required = ["lint", "path", "owner", "reason", "expires"];
    for (index, table) in split_tables(text, "[[debt]]").iter().enumerate() {
        for key in required {
            if top_table_value(table, key).is_none() {
                errors.push(format!("debt entry {} is missing `{key}`", index + 1));
            }
        }
        if let Some(expires) = top_table_value(table, "expires") {
            if expires.as_str() < today {
                errors.push(format!("debt entry {} expired on {expires}", index + 1));
            }
        }
    }
}

fn scan_rust_suppressions(errors: &mut Vec<String>) -> Result<(), String> {
    for file in rust_files()? {
        let text = fs::read_to_string(&file)
            .map_err(|error| format!("failed to read {}: {error}", file.display()))?;
        let rel = file.to_string_lossy();
        for (line_index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[allow") {
                errors.push(format!(
                    "{}:{} uses #[allow]; use #[expect(..., reason = \"...\")] or policy debt instead",
                    rel,
                    line_index + 1
                ));
            }
            if trimmed.starts_with("#[expect") && !trimmed.contains("reason") {
                errors.push(format!(
                    "{}:{} uses #[expect] without a reason",
                    rel,
                    line_index + 1
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct FileRule {
    pattern: String,
    is_glob: bool,
}

impl FileRule {
    fn matches(&self, path: &str) -> bool {
        if self.is_glob {
            glob_match(&self.pattern, path)
        } else {
            self.pattern == path
        }
    }
}

fn parse_file_allowlist(text: &str, errors: &mut Vec<String>) -> Vec<FileRule> {
    let mut rules = Vec::new();
    let required = ["kind", "owner", "reason", "surface", "classification"];
    for (index, table) in split_tables(text, "[[allow]]").iter().enumerate() {
        let path = top_table_value(table, "path");
        let glob = top_table_value(table, "glob");
        if path.is_some() == glob.is_some() {
            errors.push(format!(
                "non-Rust allowlist entry {} must set exactly one of `path` or `glob`",
                index + 1
            ));
            continue;
        }
        for key in required {
            if top_table_value(table, key).is_none() {
                errors.push(format!(
                    "non-Rust allowlist entry {} is missing `{key}`",
                    index + 1
                ));
            }
        }
        if !table.contains("covered_by") {
            errors.push(format!(
                "non-Rust allowlist entry {} is missing `covered_by`",
                index + 1
            ));
        }
        if let Some(pattern) = glob {
            rules.push(FileRule {
                pattern,
                is_glob: true,
            });
        } else if let Some(pattern) = path {
            rules.push(FileRule {
                pattern,
                is_glob: false,
            });
        }
    }
    rules
}

fn parse_panic_allowlist(text: &str, errors: &mut Vec<String>) -> BTreeSet<String> {
    let mut allowed = BTreeSet::new();
    let required = ["path", "family", "classification", "owner", "explanation"];
    for (index, table) in split_tables(text, "[[allow]]").iter().enumerate() {
        for key in required {
            if top_table_value(table, key).is_none() {
                errors.push(format!(
                    "panic allowlist entry {} is missing `{key}`",
                    index + 1
                ));
            }
        }
        if !table.contains("[allow.selector]") {
            errors.push(format!(
                "panic allowlist entry {} is missing [allow.selector]",
                index + 1
            ));
        }
        let path = top_table_value(table, "path");
        let family = top_table_value(table, "family");
        let kind = table_value_after_header(table, "[allow.selector]", "kind");
        let container = table_value_after_header(table, "[allow.selector]", "container");
        let callee = table_value_after_header(table, "[allow.selector]", "callee");
        if let (Some(path), Some(family), Some(kind), Some(container), Some(callee)) =
            (path, family, kind, container, callee)
        {
            allowed.insert(format!("{path}|{family}|{kind}|{container}|{callee}"));
        }
    }
    allowed
}

#[derive(Debug)]
struct PanicFinding {
    identity: String,
    family: String,
    line: usize,
}

fn panic_findings(path: &str, text: &str) -> Vec<PanicFinding> {
    let mut findings = Vec::new();
    let mut container = "module".to_string();
    for (line_index, line) in text.lines().enumerate() {
        if let Some(name) = fn_name(line) {
            container = name;
        }
        for (needle, family, kind, callee) in [
            (".unwrap(", "unwrap", "method_call", "unwrap"),
            (".expect(", "expect", "method_call", "expect"),
            ("panic!(", "panic", "macro_call", "panic"),
            ("todo!(", "todo", "macro_call", "todo"),
            (
                "unimplemented!(",
                "unimplemented",
                "macro_call",
                "unimplemented",
            ),
            ("unreachable!(", "unreachable", "macro_call", "unreachable"),
        ] {
            if line.contains(needle) {
                findings.push(PanicFinding {
                    identity: format!("{path}|{family}|{kind}|{container}|{callee}"),
                    family: family.to_string(),
                    line: line_index + 1,
                });
            }
        }
    }
    findings
}

fn fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("fn ")
        .or_else(|| trimmed.strip_prefix("pub fn "))
        .or_else(|| trimmed.strip_prefix("async fn "))
        .or_else(|| trimmed.strip_prefix("pub async fn "))?;
    let name = rest.split('(').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn split_tables(text: &str, marker: &str) -> Vec<String> {
    let mut tables = Vec::new();
    let mut current = Vec::new();
    let mut in_table = false;
    for line in text.lines() {
        if line.trim() == marker {
            if in_table {
                tables.push(current.join("\n"));
                current.clear();
            }
            in_table = true;
        }
        if in_table {
            current.push(line.to_string());
        }
    }
    if in_table {
        tables.push(current.join("\n"));
    }
    tables
}

fn top_table_value(table: &str, key: &str) -> Option<String> {
    let mut nested = false;
    for line in table.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[')
            && trimmed != "[[allow]]"
            && trimmed != "[[lint]]"
            && trimmed != "[[debt]]"
        {
            nested = true;
            continue;
        }
        if nested {
            continue;
        }
        if let Some(value) = parse_key_value(trimmed, key) {
            return Some(value);
        }
    }
    None
}

fn table_value_after_header(table: &str, header: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in table.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == header;
            continue;
        }
        if in_section {
            if let Some(value) = parse_key_value(trimmed, key) {
                return Some(value);
            }
        }
    }
    None
}

fn count_tables(text: &str, marker: &str) -> usize {
    text.lines().filter(|line| line.trim() == marker).count()
}

fn rust_files() -> Result<Vec<PathBuf>, String> {
    repo_files().map(|files| {
        files
            .into_iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
            .collect()
    })
}

fn repo_files() -> Result<Vec<PathBuf>, String> {
    let output = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .map_err(|error| format!("failed to run git ls-files: {error}"))?;
    if !output.status.success() {
        return Err("git ls-files failed".to_string());
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("git ls-files output was not utf8: {error}"))?;
    Ok(stdout.lines().map(PathBuf::from).collect())
}

fn is_ignored_for_file_policy(path: &str) -> bool {
    path == "Cargo.lock" || path == "LICENSE" || path == "README.md" || path.starts_with("target/")
}

fn is_rust_source_or_manifest(path: &str) -> bool {
    path.ends_with(".rs") || path.ends_with("Cargo.toml")
}

fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }
    glob_parts(pattern, path)
}

fn glob_parts(pattern: &str, path: &str) -> bool {
    if let Some((prefix, suffix)) = pattern.split_once("**/") {
        return path.starts_with(prefix) && glob_parts(suffix, path.trim_start_matches(prefix));
    }
    if let Some((prefix, suffix)) = pattern.split_once("**") {
        return path.starts_with(prefix) && path.ends_with(suffix.trim_start_matches('/'));
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        return path.starts_with(prefix) && path.ends_with(suffix);
    }
    pattern == path
}
