use serde_json::Value;

/// Extract a Bash command from a Claude Code `PreToolUse` hook payload.
pub fn extract_bash_command(payload: &Value) -> Option<String> {
    payload
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Built-in deny patterns for potentially destructive shell commands.
pub fn built_in_deny_patterns() -> Vec<String> {
    vec![
        "rm -rf".to_string(),
        "rm -r ".to_string(),
        "mkfs".to_string(),
        "dd if=".to_string(),
        "shutdown".to_string(),
        "reboot".to_string(),
        "git push --force".to_string(),
        "git push -f".to_string(),
        "git reset --hard".to_string(),
    ]
}

/// Case-insensitive substring match against a command and pattern list.
pub fn matches_any_pattern(cmd: &str, patterns: &[String]) -> bool {
    let lower = cmd.to_lowercase();
    patterns.iter().any(|p| lower.contains(&p.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bash_command_from_payload() {
        let payload = serde_json::json!({
            "tool_input": {
                "command": "rm -rf /tmp/demo"
            }
        });

        assert_eq!(
            extract_bash_command(&payload).as_deref(),
            Some("rm -rf /tmp/demo")
        );
    }

    #[test]
    fn missing_command_returns_none() {
        let payload = serde_json::json!({"tool_input": {}});
        assert_eq!(extract_bash_command(&payload), None);
    }

    #[test]
    fn pattern_matching_is_case_insensitive() {
        let patterns = vec!["git push --force".to_string()];
        assert!(matches_any_pattern(
            "GIT PUSH --FORCE origin main",
            &patterns
        ));
    }

    #[test]
    fn built_in_patterns_include_expected_defaults() {
        let patterns = built_in_deny_patterns();
        assert!(patterns.contains(&"rm -rf".to_string()));
        assert!(patterns.contains(&"git reset --hard".to_string()));
    }
}
