pub(crate) fn random_id(prefix: &str) -> Result<String, String> {
    if prefix.is_empty()
        || prefix.len() > 64
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Creation identity prefix is invalid.".to_string());
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| "Creation identity is unavailable.".to_string())?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{prefix}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_identifiers_are_fixed_format_and_distinct() {
        let first = random_id("image_").unwrap();
        let second = random_id("image_").unwrap();
        assert_ne!(first, second);
        assert_eq!(first.len(), "image_".len() + 32);
        assert!(first.starts_with("image_"));
        assert!(
            first["image_".len()..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }
}
