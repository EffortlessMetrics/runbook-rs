/// Built-in deny patterns for bash command policy enforcement.
pub fn built_in_bash_deny_patterns() -> Vec<String> {
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

/// Case-insensitive substring match against a set of patterns.
pub fn matches_any_pattern(command: &str, patterns: &[String]) -> bool {
    let lower_command = command.to_lowercase();
    patterns
        .iter()
        .any(|pattern| lower_command.contains(&pattern.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_patterns_include_known_destructive_commands() {
        let patterns = built_in_bash_deny_patterns();
        assert!(patterns.iter().any(|p| p == "rm -rf"));
        assert!(patterns.iter().any(|p| p == "git reset --hard"));
    }

    #[test]
    fn pattern_matching_is_case_insensitive() {
        let patterns = vec!["git push --force".to_string()];
        assert!(matches_any_pattern(
            "GiT PuSh --FoRcE origin main",
            &patterns
        ));
    }

    #[test]
    fn pattern_matching_returns_false_without_match() {
        let patterns = vec!["cargo test".to_string()];
        assert!(!matches_any_pattern("cargo build --release", &patterns));
    }
}
