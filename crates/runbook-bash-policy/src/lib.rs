use serde_json::Value;

const BUILT_IN_DENY_PATTERNS: [&str; 9] = [
    "rm -rf",
    "rm -r ",
    "mkfs",
    "dd if=",
    "shutdown",
    "reboot",
    "git push --force",
    "git push -f",
    "git reset --hard",
];

/// Extracts `tool_input.command` from a Claude Code PreToolUse payload.
pub fn extract_bash_command(payload: &Value) -> Option<String> {
    payload
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Returns the built-in deny patterns for destructive bash commands.
pub fn built_in_deny_patterns() -> Vec<String> {
    BUILT_IN_DENY_PATTERNS
        .iter()
        .map(|pattern| pattern.to_string())
        .collect()
}

/// Case-insensitive substring matching across policy patterns.
pub fn matches_any_pattern(cmd: &str, patterns: &[String]) -> bool {
    let lower = cmd.to_lowercase();
    patterns
        .iter()
        .any(|pattern| lower.contains(&pattern.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_command() {
        let payload = serde_json::json!({
            "tool_input": {
                "command": "git status"
            }
        });
        assert_eq!(
            extract_bash_command(&payload),
            Some("git status".to_string())
        );
    }

    #[test]
    fn denies_known_destructive_pattern_case_insensitively() {
        let patterns = built_in_deny_patterns();
        assert!(matches_any_pattern(
            "GIT PUSH --FORCE origin main",
            &patterns
        ));
    }

    #[test]
    fn allows_safe_command() {
        let patterns = built_in_deny_patterns();
        assert!(!matches_any_pattern("cargo test", &patterns));
    }
}
