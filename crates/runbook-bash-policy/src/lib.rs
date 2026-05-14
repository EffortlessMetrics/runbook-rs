//! Bash command policy helpers shared by Runbook binaries.

/// Default deny patterns used when filtering dangerous Bash commands.
pub fn built_in_deny_patterns() -> &'static [&'static str] {
    &[
        "rm -rf",
        "rm -r ",
        "mkfs",
        "dd if=",
        "shutdown",
        "reboot",
        "git push --force",
        "git push -f",
        "git reset --hard",
    ]
}

/// Returns true when `cmd` matches any deny `patterns` (case-insensitive substring).
pub fn matches_any_pattern<S: AsRef<str>>(cmd: &str, patterns: &[S]) -> bool {
    let lower = cmd.to_lowercase();
    patterns
        .iter()
        .any(|p| lower.contains(&p.as_ref().to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_include_rm_rf() {
        assert!(built_in_deny_patterns().contains(&"rm -rf"));
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(matches_any_pattern(
            "Git PUSH --FORCE origin",
            &["git push --force"]
        ));
    }

    #[test]
    fn non_matching_command_returns_false() {
        assert!(!matches_any_pattern("cargo test", &["rm -rf", "mkfs"]));
    }
}
