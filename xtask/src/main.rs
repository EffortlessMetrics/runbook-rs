use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const REQUIRED_RUST_LINTS: &[&str] = &[
    "unsafe_code",
    "unsafe_op_in_unsafe_fn",
    "unused_must_use",
    "unexpected_cfgs",
    "const_item_interior_mutations",
    "function_casts_as_integer",
];

const REQUIRED_CLIPPY_LINTS: &[&str] = &[
    "dbg_macro",
    "todo",
    "unimplemented",
    "panic",
    "unreachable",
    "unwrap_used",
    "expect_used",
    "get_unwrap",
    "unwrap_in_result",
    "panic_in_result_fn",
    "string_slice",
    "indexing_slicing",
    "out_of_bounds_indexing",
    "unchecked_time_subtraction",
    "char_indices_as_byte_indices",
    "sliced_string_as_bytes",
    "index_refutable_slice",
    "let_underscore_future",
    "let_underscore_must_use",
    "let_underscore_lock",
    "unused_result_ok",
    "map_err_ignore",
    "assertions_on_result_states",
    "lines_filter_map_ok",
    "await_holding_lock",
    "await_holding_refcell_ref",
    "await_holding_invalid_type",
    "future_not_send",
    "large_futures",
    "arc_with_non_send_sync",
    "rc_mutex",
    "mut_mutex_lock",
    "readonly_write_lock",
    "mem_forget",
    "forget_non_drop",
    "drop_non_drop",
    "undocumented_unsafe_blocks",
    "multiple_unsafe_ops_per_block",
    "repr_packed_without_abi",
    "float_cmp",
    "float_cmp_const",
    "float_equality_without_abs",
    "lossy_float_literal",
    "cast_sign_loss",
    "cast_possible_wrap",
    "cast_possible_truncation",
    "cast_precision_loss",
    "invalid_upcast_comparisons",
    "cast_abs_to_unsigned",
    "cast_enum_truncation",
    "cast_nan_to_int",
    "manual_midpoint",
    "manual_is_multiple_of",
    "manual_div_ceil",
    "arithmetic_side_effects",
    "suspicious_open_options",
    "nonsensical_open_options",
    "ineffective_open_options",
    "path_buf_push_overwrite",
    "join_absolute_paths",
    "read_line_without_trim",
    "exit",
    "iter_not_returning_iterator",
    "expl_impl_clone_on_copy",
    "infallible_try_from",
    "fallible_impl_from",
    "error_impl_error",
    "result_unit_err",
    "result_large_err",
    "format_in_format_args",
    "to_string_in_format_args",
    "unused_format_specs",
    "unnecessary_debug_formatting",
    "uninlined_format_args",
    "manual_let_else",
    "manual_ok_or",
    "manual_strip",
    "manual_split_once",
    "manual_is_variant_and",
    "filter_map_next",
    "flat_map_option",
    "match_result_ok",
    "cloned_instead_of_copied",
    "iter_cloned_collect",
    "iter_overeager_cloned",
    "needless_collect",
    "redundant_closure",
    "redundant_closure_for_method_calls",
    "missing_panics_doc",
    "missing_errors_doc",
    "allow_attributes",
    "allow_attributes_without_reason",
    "blanket_clippy_restriction_lints",
    "ignore_without_reason",
    "should_panic_without_expect",
];

const FORBIDDEN_CLIPPY_CARVEOUTS: &[&str] = &[
    "allow-unwrap-in-tests",
    "allow-expect-in-tests",
    "allow-panic-in-tests",
    "allow-indexing-slicing-in-tests",
    "allow-dbg-in-tests",
];

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());

    let result = match command.as_str() {
        "check-lint-policy" => check_lint_policy(),
        "policy-report" => policy_report(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        unknown => Err(format!("unknown xtask command: {unknown}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("usage: cargo xtask <command>");
    println!();
    println!("commands:");
    println!("  check-lint-policy  verify workspace Clippy policy and debt ledgers");
    println!("  policy-report       print a short policy inventory");
}

fn check_lint_policy() -> Result<(), String> {
    let root = env::current_dir().map_err(|err| format!("read current dir: {err}"))?;
    let cargo_toml = read_to_string(root.join("Cargo.toml"))?;
    let lint_policy = read_to_string(root.join("policy/clippy-lints.toml"))?;

    let workspace_msrv = required_value(&cargo_toml, "rust-version")
        .ok_or("Cargo.toml is missing workspace.package.rust-version")?;
    let policy_msrv =
        required_value(&lint_policy, "msrv").ok_or("policy/clippy-lints.toml is missing msrv")?;
    if workspace_msrv != policy_msrv {
        return Err(format!(
            "workspace MSRV {workspace_msrv} does not match policy MSRV {policy_msrv}"
        ));
    }

    require_section(&cargo_toml, "[workspace.lints.rust]")?;
    require_section(&cargo_toml, "[workspace.lints.clippy]")?;
    require_lints(&cargo_toml, REQUIRED_RUST_LINTS, "workspace Rust lint")?;
    require_lints(&cargo_toml, REQUIRED_CLIPPY_LINTS, "workspace Clippy lint")?;
    require_policy_flags(&lint_policy)?;
    check_workspace_members_inherit_lints(&root, &cargo_toml)?;
    check_clippy_toml(&root)?;
    check_planned_lints_are_not_early(&cargo_toml, &lint_policy, &workspace_msrv)?;
    check_debt(&root)?;
    check_rust_suppressions(&root)?;

    println!("lint policy check passed");
    Ok(())
}

fn policy_report() -> Result<(), String> {
    let root = env::current_dir().map_err(|err| format!("read current dir: {err}"))?;
    let debt = read_to_string(root.join("policy/clippy-debt.toml"))?;
    let lint_policy = read_to_string(root.join("policy/clippy-lints.toml"))?;
    let planned_count = lint_policy.matches("[[planned]]").count();
    let debt_count = debt.matches("[[debt]]").count();

    println!("planned lint flips: {planned_count}");
    println!("clippy debt entries: {debt_count}");
    Ok(())
}

fn require_section(contents: &str, section: &str) -> Result<(), String> {
    if contents.contains(section) {
        Ok(())
    } else {
        Err(format!("missing required section {section}"))
    }
}

fn require_lints(contents: &str, lints: &[&str], label: &str) -> Result<(), String> {
    let mut missing = Vec::new();
    for lint in lints {
        let needle = format!("{lint} = ");
        if !contents.contains(&needle) {
            missing.push(*lint);
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("missing {label}s: {}", missing.join(", ")))
    }
}

fn require_policy_flags(contents: &str) -> Result<(), String> {
    let required = [
        "panic_free_tests = true",
        "allow_test_carveouts = false",
        "suppression_style = \"expect-with-reason\"",
        "blanket_categories = false",
    ];
    for flag in required {
        if !contents.contains(flag) {
            return Err(format!("policy/clippy-lints.toml missing `{flag}`"));
        }
    }
    Ok(())
}

fn check_workspace_members_inherit_lints(root: &Path, cargo_toml: &str) -> Result<(), String> {
    let members = workspace_members(cargo_toml)?;
    let mut missing = Vec::new();
    for member in members {
        let manifest = root.join(&member).join("Cargo.toml");
        let contents = read_to_string(&manifest)?;
        if !(contents.contains("[lints]") && contents.contains("workspace = true")) {
            missing.push(member);
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "workspace members missing `[lints] workspace = true`: {}",
            missing.join(", ")
        ))
    }
}

fn workspace_members(cargo_toml: &str) -> Result<Vec<String>, String> {
    let members_start = cargo_toml
        .find("members = [")
        .ok_or("Cargo.toml is missing workspace members")?;
    let after_start = &cargo_toml[members_start..];
    let list_end = after_start
        .find(']')
        .ok_or("Cargo.toml workspace members list is not closed")?;
    let list = &after_start[..list_end];

    let members = list
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_end_matches(',').trim();
            if trimmed.starts_with('"') && trimmed.ends_with('"') {
                Some(trimmed.trim_matches('"').to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if members.is_empty() {
        Err("Cargo.toml workspace members list is empty".to_string())
    } else {
        Ok(members)
    }
}

fn check_clippy_toml(root: &Path) -> Result<(), String> {
    let path = root.join("clippy.toml");
    let contents = read_to_string(&path)?;
    for carveout in FORBIDDEN_CLIPPY_CARVEOUTS {
        let enabled = format!("{carveout} = true");
        if contents.contains(&enabled) {
            return Err(format!(
                "forbidden Clippy test carveout in clippy.toml: {enabled}"
            ));
        }
    }
    Ok(())
}

fn check_planned_lints_are_not_early(
    cargo_toml: &str,
    lint_policy: &str,
    workspace_msrv: &str,
) -> Result<(), String> {
    for block in lint_policy.split("[[planned]]").skip(1) {
        let name = required_value(block, "name").ok_or("planned lint is missing name")?;
        let activate_when = required_value(block, "activate_when_msrv")
            .ok_or_else(|| format!("planned lint {name} is missing activate_when_msrv"))?;
        if version_lt(workspace_msrv, &activate_when) {
            let cargo_key = name.strip_prefix("clippy::").unwrap_or(&name);
            let active_needle = format!("{cargo_key} = ");
            if cargo_toml.contains(&active_needle) {
                return Err(format!(
                    "planned lint {name} activates at MSRV {activate_when} but is active at {workspace_msrv}"
                ));
            }
        }
    }
    Ok(())
}

fn check_debt(root: &Path) -> Result<(), String> {
    let contents = read_to_string(root.join("policy/clippy-debt.toml"))?;
    let today = current_utc_date()?;

    for (index, block) in contents.split("[[debt]]").skip(1).enumerate() {
        for key in ["lint", "path", "owner", "reason", "expires"] {
            if required_value(block, key).is_none() {
                return Err(format!("debt entry {} is missing {key}", index + 1));
            }
        }
        let expires = required_value(block, "expires").expect("expires checked above");
        if expires.as_str() < today.as_str() {
            return Err(format!(
                "debt entry {} expired on {expires}; today is {today}",
                index + 1
            ));
        }
    }
    Ok(())
}

fn check_rust_suppressions(root: &Path) -> Result<(), String> {
    let mut rust_files = Vec::new();
    collect_rust_files(root, &mut rust_files)?;

    for file in rust_files {
        let contents = read_to_string(&file)?;
        check_allow_attributes(&file, &contents)?;
        check_expect_reasons(&file, &contents)?;
    }

    Ok(())
}

fn check_allow_attributes(path: &Path, contents: &str) -> Result<(), String> {
    for (line_index, line) in contents.lines().enumerate() {
        if line.trim_start().starts_with("#[allow") {
            return Err(format!(
                "{}:{} uses #[allow]; use #[expect(..., reason = \"...\")] or policy debt instead",
                path.display(),
                line_index + 1
            ));
        }
    }
    Ok(())
}

fn check_expect_reasons(path: &Path, contents: &str) -> Result<(), String> {
    let mut in_expect = false;
    let mut attr = String::new();
    let mut start_line = 0usize;

    for (line_index, line) in contents.lines().enumerate() {
        if !in_expect && line.trim_start().starts_with("#[expect") {
            in_expect = true;
            attr.clear();
            start_line = line_index + 1;
        }

        if in_expect {
            attr.push_str(line);
            attr.push('\n');
            if line.contains(']') {
                if !attr.contains("reason =") {
                    return Err(format!(
                        "{}:{start_line} has #[expect] without a reason",
                        path.display()
                    ));
                }
                in_expect = false;
            }
        }
    }

    if in_expect {
        return Err(format!(
            "{}:{start_line} has an unterminated #[expect] attribute",
            path.display()
        ));
    }

    Ok(())
}

fn collect_rust_files(dir: &Path, rust_files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|err| format!("read {}: {err}", dir.display()))? {
        let entry = entry.map_err(|err| format!("read entry in {}: {err}", dir.display()))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if file_name == "target" || file_name == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_rust_files(&path, rust_files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            rust_files.push(path);
        }
    }
    Ok(())
}

fn required_value(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = ");
    contents.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix(&prefix)?;
        Some(value.trim().trim_matches('"').to_string())
    })
}

fn read_to_string(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))
}

fn version_lt(left: &str, right: &str) -> bool {
    let left_parts = semver_parts(left);
    let right_parts = semver_parts(right);
    left_parts < right_parts
}

fn semver_parts(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn current_utc_date() -> Result<String, String> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time before UNIX epoch: {err}"))?;
    let days = (duration.as_secs() / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}
