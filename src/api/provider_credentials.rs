pub(crate) fn resolve(env_name: &str, saved: &str) -> String {
    resolve_value(std::env::var(env_name).ok(), saved)
}

fn resolve_value(environment: Option<String>, saved: &str) -> String {
    environment
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| saved.to_string())
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::resolve_value;

    #[test]
    fn nonempty_environment_value_has_priority() {
        assert_eq!(
            resolve_value(Some(" environment ".to_string()), "saved"),
            "environment"
        );
    }

    #[test]
    fn missing_or_blank_environment_value_uses_saved_value() {
        assert_eq!(resolve_value(None, " saved "), "saved");
        assert_eq!(resolve_value(Some("   ".to_string()), " saved "), "saved");
    }
}
