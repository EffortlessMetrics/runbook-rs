use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde::Deserialize;

const TODAY: &str = "2026-05-06";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(errs) => {
            for err in errs {
                eprintln!("policy error: {err}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Vec<String>> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    match command.as_str() {
        "check-lint-policy" => collect(check_lint_policy),
        "check-no-panic-family" => collect(check_no_panic_family),
        "check-file-policy" => collect(check_file_policy),
        "policy-report" => policy_report(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(vec![format!("unknown xtask command `{other}`")]),
    }
}

fn collect(check: fn(&mut Vec<String>)) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    check(&mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn print_help() {
    println!("runbook xtask commands:");
    println!("  check-lint-policy      verify Cargo, Clippy, suppression, and debt policy");
    println!("  check-no-panic-family  verify panic-family allowlist entries");
    println!("  check-file-policy      verify non-Rust policy allowlist entries");
    println!("  policy-report          run all policy checks and print a summary");
}

fn policy_report() -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    check_lint_policy(&mut errors);
    check_no_panic_family(&mut errors);
    check_file_policy(&mut errors);
    if errors.is_empty() {
        println!("policy report: lint, panic-family, and non-Rust file policy checks passed");
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_lint_policy(errors: &mut Vec<String>) {
    let root = read_toml(Path::new("Cargo.toml"), errors);
    let policy = read_lint_policy(errors);

    let Some(root) = root else { return };
    let Some(policy) = policy else { return };

    let root_msrv = root
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("rust-version"))
        .and_then(toml::Value::as_str);
    if root_msrv != Some(policy.msrv.as_str()) {
        errors.push(format!(
            "workspace.package.rust-version ({root_msrv:?}) must match policy/clippy-lints.toml msrv ({})",
            policy.msrv
        ));
    }

    let active = lint_table(&root);
    for lint in &policy.lint {
        if lint.status == "active" {
            match active.get(lint.cargo_key()) {
                Some(level) if level == &lint.level => {}
                Some(level) => errors.push(format!(
                    "active lint {} is `{level}` in Cargo.toml but policy requires `{}`",
                    lint.name, lint.level
                )),
                None => errors.push(format!(
                    "active lint {} is missing from Cargo.toml",
                    lint.name
                )),
            }
        }
    }
    for planned in &policy.planned {
        if version_less(&policy.msrv, &planned.activate_when_msrv)
            && active.contains_key(planned.cargo_key())
        {
            errors.push(format!(
                "planned lint {} must not be active before MSRV {}",
                planned.name, planned.activate_when_msrv
            ));
        }
    }

    let members = root
        .get("workspace")
        .and_then(|v| v.get("members"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for member in members {
        let Some(member) = member.as_str() else {
            continue;
        };
        let path = Path::new(member).join("Cargo.toml");
        let Some(manifest) = read_toml(&path, errors) else {
            continue;
        };
        let inherits = manifest
            .get("lints")
            .and_then(|v| v.get("workspace"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        if !inherits {
            errors.push(format!(
                "workspace member {member} must set [lints] workspace = true"
            ));
        }
    }

    check_clippy_toml(errors);
    check_debt(errors);
    check_suppressions(errors);
}

fn check_clippy_toml(errors: &mut Vec<String>) {
    let contents = fs::read_to_string("clippy.toml").unwrap_or_default();
    for carveout in [
        "allow-unwrap-in-tests",
        "allow-expect-in-tests",
        "allow-panic-in-tests",
        "allow-indexing-slicing-in-tests",
        "allow-dbg-in-tests",
    ] {
        if contents.lines().any(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with('#') && trimmed.starts_with(carveout) && trimmed.contains("true")
        }) {
            errors.push(format!(
                "clippy.toml must not enable test carveout `{carveout}`"
            ));
        }
    }
}

fn check_debt(errors: &mut Vec<String>) {
    let Some(debt) = read_debt_policy(errors) else {
        return;
    };
    for item in debt.debt {
        require_non_empty(errors, &item.lint, "clippy debt lint");
        require_non_empty(errors, &item.path, "clippy debt path");
        require_non_empty(errors, &item.owner, "clippy debt owner");
        require_non_empty(errors, &item.reason, "clippy debt reason");
        require_non_empty(errors, &item.expires, "clippy debt expires");
        if item.expires.as_str() < TODAY {
            errors.push(format!(
                "clippy debt {} for {} expired on {}",
                item.lint, item.path, item.expires
            ));
        }
    }
}

fn check_suppressions(errors: &mut Vec<String>) {
    for path in rust_files() {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = contents.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[allow") || trimmed.starts_with("#![allow") {
                errors.push(format!(
                    "{}:{} uses #[allow]; use #[expect(..., reason = \"...\")] or policy debt",
                    path.display(),
                    idx.saturating_add(1)
                ));
            }
            if trimmed.starts_with("#[expect") || trimmed.starts_with("#![expect") {
                let mut attr = String::new();
                for attr_line in lines.iter().skip(idx) {
                    attr.push_str(attr_line);
                    if attr_line.trim_end().ends_with(']') {
                        break;
                    }
                }
                if !attr.contains("reason") {
                    errors.push(format!(
                        "{}:{} uses #[expect] without reason",
                        path.display(),
                        idx.saturating_add(1)
                    ));
                }
            }
        }
    }
}

fn check_no_panic_family(errors: &mut Vec<String>) {
    let Some(policy) = read_panic_policy(errors) else {
        return;
    };
    for entry in &policy.allow {
        require_non_empty(errors, &entry.path, "panic allow path");
        require_non_empty(errors, &entry.family, "panic allow family");
        require_non_empty(errors, &entry.classification, "panic allow classification");
        require_non_empty(errors, &entry.owner, "panic allow owner");
        require_non_empty(errors, &entry.explanation, "panic allow explanation");
        require_non_empty(errors, &entry.selector.kind, "panic allow selector.kind");
        if let Some(expires) = &entry.expires {
            if expires.as_str() < TODAY {
                errors.push(format!(
                    "panic allow {} in {} expired on {expires}",
                    entry.family, entry.path
                ));
            }
        }
        let text = fs::read_to_string(&entry.path).unwrap_or_default();
        if !text.contains(entry.selector.callee.as_deref().unwrap_or(&entry.family)) {
            errors.push(format!(
                "panic allow selector for {} in {} appears stale",
                entry.family, entry.path
            ));
        }
    }

    for path in rust_files() {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in contents.lines().enumerate() {
            for family in [
                "unwrap",
                "expect",
                "panic",
                "todo",
                "unimplemented",
                "unreachable",
            ] {
                if contains_family(line, family) && !panic_allowed(&policy, &path, family, line) {
                    errors.push(format!(
                        "{}:{} contains unallowlisted panic-family `{family}`",
                        path.display(),
                        idx.saturating_add(1)
                    ));
                }
            }
        }
    }
}

fn contains_family(line: &str, family: &str) -> bool {
    line.contains(&format!(".{family}("))
        || line.contains(&format!("{family}!("))
        || line.contains(&format!("{family}::"))
}

fn panic_allowed(policy: &PanicPolicy, path: &Path, family: &str, line: &str) -> bool {
    let path = path.to_string_lossy();
    policy.allow.iter().any(|entry| {
        entry.path == path
            && entry.family == family
            && entry
                .selector
                .callee
                .as_deref()
                .is_none_or(|callee| line.contains(callee))
            && entry
                .selector
                .receiver_fingerprint
                .as_deref()
                .is_none_or(|fingerprint| line.contains(fingerprint))
    })
}

fn check_file_policy(errors: &mut Vec<String>) {
    let Some(policy) = read_file_policy(errors) else {
        return;
    };
    for entry in &policy.allow {
        if entry.path.is_none() == entry.glob.is_none() {
            errors.push("non-Rust allow entry must set exactly one of path or glob".to_string());
        }
        require_non_empty(errors, &entry.kind, "non-Rust allow kind");
        require_non_empty(errors, &entry.owner, "non-Rust allow owner");
        require_non_empty(errors, &entry.reason, "non-Rust allow reason");
        require_non_empty(errors, &entry.surface, "non-Rust allow surface");
        require_non_empty(
            errors,
            &entry.classification,
            "non-Rust allow classification",
        );
        if matches!(
            entry.classification.as_str(),
            "production" | "test" | "tooling"
        ) && entry.covered_by.is_empty()
        {
            errors.push(format!(
                "non-Rust allow {:?} must define covered_by",
                entry.identity()
            ));
        }
        if let Some(expires) = &entry.expires {
            if expires.as_str() < TODAY {
                errors.push(format!(
                    "non-Rust allow {:?} expired on {expires}",
                    entry.identity()
                ));
            }
        }
    }

    for file in non_rust_programming_files() {
        if !file_allowed(&policy, &file) {
            errors.push(format!("{} is a non-Rust programming/config file without policy/non-rust-allowlist.toml coverage", file.display()));
        }
    }
}

fn file_allowed(policy: &FilePolicy, file: &Path) -> bool {
    let text = file.to_string_lossy();
    policy.allow.iter().any(|entry| {
        entry.path.as_deref() == Some(text.as_ref())
            || entry
                .glob
                .as_deref()
                .is_some_and(|glob| glob_match(glob, &text))
    })
}

fn glob_match(glob: &str, path: &str) -> bool {
    if glob == path || glob == "**" {
        return true;
    }

    if let Some((prefix, suffix)) = glob.split_once("**") {
        return path.starts_with(prefix) && path.ends_with(suffix.trim_start_matches(['*', '/']));
    }

    if let Some((prefix, suffix)) = glob.split_once('*') {
        let Some(rest) = path.strip_prefix(prefix) else {
            return false;
        };
        return rest.ends_with(suffix) && !rest.trim_end_matches(suffix).contains('/');
    }

    false
}

fn non_rust_programming_files() -> Vec<PathBuf> {
    walk(Path::new("."))
        .into_iter()
        .filter(|path| {
            let text = path.to_string_lossy();
            !text.starts_with("./target/")
                && !text.starts_with("./.git/")
                && matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("sh" | "json" | "yaml" | "yml" | "md" | "feature")
                )
        })
        .map(strip_dot)
        .collect()
}

fn rust_files() -> Vec<PathBuf> {
    walk(Path::new("."))
        .into_iter()
        .filter(|path| {
            let text = path.to_string_lossy();
            !text.starts_with("./target/")
                && !text.starts_with("./.git/")
                && path.extension().and_then(|s| s.to_str()) == Some("rs")
        })
        .map(strip_dot)
        .collect()
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(read_dir) = fs::read_dir(root) else {
        return files;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if matches!(name, ".git" | "target") {
            continue;
        }
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

fn strip_dot(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix("./") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

fn read_toml(path: &Path, errors: &mut Vec<String>) -> Option<toml::Value> {
    match fs::read_to_string(path) {
        Ok(contents) => match contents.parse::<toml::Value>() {
            Ok(value) => Some(value),
            Err(err) => {
                errors.push(format!("failed to parse {}: {err}", path.display()));
                None
            }
        },
        Err(err) => {
            errors.push(format!("failed to read {}: {err}", path.display()));
            None
        }
    }
}

fn lint_table(root: &toml::Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for group in ["rust", "clippy"] {
        if let Some(table) = root
            .get("workspace")
            .and_then(|v| v.get("lints"))
            .and_then(|v| v.get(group))
            .and_then(toml::Value::as_table)
        {
            for (key, value) in table {
                if let Some(level) = value.as_str() {
                    out.insert(format!("{group}::{key}"), level.to_string());
                }
            }
        }
    }
    out
}

fn read_lint_policy(errors: &mut Vec<String>) -> Option<LintPolicy> {
    read_policy("policy/clippy-lints.toml", errors)
}

fn read_debt_policy(errors: &mut Vec<String>) -> Option<DebtPolicy> {
    read_policy("policy/clippy-debt.toml", errors)
}

fn read_panic_policy(errors: &mut Vec<String>) -> Option<PanicPolicy> {
    read_policy("policy/no-panic-allowlist.toml", errors)
}

fn read_file_policy(errors: &mut Vec<String>) -> Option<FilePolicy> {
    read_policy("policy/non-rust-allowlist.toml", errors)
}

fn read_policy<T: for<'de> Deserialize<'de>>(path: &str, errors: &mut Vec<String>) -> Option<T> {
    match fs::read_to_string(path) {
        Ok(contents) => match toml::from_str(&contents) {
            Ok(value) => Some(value),
            Err(err) => {
                errors.push(format!("failed to parse {path}: {err}"));
                None
            }
        },
        Err(err) => {
            errors.push(format!("failed to read {path}: {err}"));
            None
        }
    }
}

fn require_non_empty(errors: &mut Vec<String>, value: &str, label: &str) {
    if value.trim().is_empty() {
        errors.push(format!("{label} must not be empty"));
    }
}

fn version_less(a: &str, b: &str) -> bool {
    parse_version(a) < parse_version(b)
}

fn parse_version(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

#[derive(Debug, Deserialize)]
struct LintPolicy {
    msrv: String,
    #[serde(default)]
    lint: Vec<LintEntry>,
    #[serde(default)]
    planned: Vec<PlannedLint>,
}

#[derive(Debug, Deserialize)]
struct LintEntry {
    name: String,
    level: String,
    status: String,
}

impl LintEntry {
    fn cargo_key(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Deserialize)]
struct PlannedLint {
    name: String,
    #[serde(rename = "activate_when_msrv")]
    activate_when_msrv: String,
}

impl PlannedLint {
    fn cargo_key(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Deserialize)]
struct DebtPolicy {
    #[serde(default)]
    debt: Vec<DebtEntry>,
}

#[derive(Debug, Deserialize)]
struct DebtEntry {
    lint: String,
    path: String,
    owner: String,
    reason: String,
    expires: String,
}

#[derive(Debug, Deserialize)]
struct PanicPolicy {
    #[serde(default)]
    allow: Vec<PanicAllow>,
}

#[derive(Debug, Deserialize)]
struct PanicAllow {
    path: String,
    family: String,
    classification: String,
    owner: String,
    explanation: String,
    expires: Option<String>,
    selector: PanicSelector,
}

#[derive(Debug, Deserialize)]
struct PanicSelector {
    kind: String,
    callee: Option<String>,
    receiver_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FilePolicy {
    #[serde(default)]
    allow: Vec<FileAllow>,
}

#[derive(Debug, Deserialize)]
struct FileAllow {
    path: Option<String>,
    glob: Option<String>,
    kind: String,
    owner: String,
    reason: String,
    surface: String,
    classification: String,
    #[serde(default)]
    covered_by: Vec<String>,
    expires: Option<String>,
}

impl FileAllow {
    fn identity(&self) -> Option<&str> {
        self.path.as_deref().or(self.glob.as_deref())
    }
}
