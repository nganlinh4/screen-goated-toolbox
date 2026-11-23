use std::collections::HashSet;

const MAX_DECL_NAME_BYTES: usize = 128;

/// Produce a bounded ASCII declaration name. The route digest makes normalized
/// collisions independent of catalog order; ToolRoute retains both exact names.
pub(super) fn unique_decl_name(id: &str, tool: &str, seen: &mut HashSet<String>) -> String {
    let digest = stable_route_digest(id, tool);
    let digest_suffix = format!("_h{digest:016x}");
    let stem_limit = MAX_DECL_NAME_BYTES - digest_suffix.len();
    let normalized = format!(
        "mcp__{}__{}",
        normalize_decl_component(id),
        normalize_decl_component(tool)
    );
    let stem = normalized
        .bytes()
        .take(stem_limit)
        .map(char::from)
        .collect::<String>();
    let base = format!("{stem}{digest_suffix}");
    if seen.insert(base.clone()) {
        return base;
    }
    let mut n = 2;
    loop {
        let suffix = format!("_{n}");
        // `base` is ASCII, so any byte length is a valid char boundary.
        let keep = (MAX_DECL_NAME_BYTES - suffix.len()).min(base.len());
        let candidate = format!("{}{suffix}", &base[..keep]);
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

fn normalize_decl_component(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len().min(MAX_DECL_NAME_BYTES));
    let mut previous_separator = false;
    for character in value.chars() {
        let mapped = if character.is_ascii_alphanumeric() || character == '_' {
            character
        } else {
            '_'
        };
        if mapped == '_' && previous_separator {
            continue;
        }
        if normalized.len() == MAX_DECL_NAME_BYTES {
            break;
        }
        normalized.push(mapped);
        previous_separator = mapped == '_';
    }
    if normalized.is_empty() {
        normalized.push_str("unnamed");
    }
    normalized
}

fn stable_route_digest(id: &str, tool: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in (id.len() as u64)
        .to_le_bytes()
        .into_iter()
        .chain(id.bytes())
        .chain((tool.len() as u64).to_le_bytes())
        .chain(tool.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_decl_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= MAX_DECL_NAME_BYTES
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
    }

    #[test]
    fn declared_names_are_bounded_ascii_and_collision_free() {
        let mut seen = HashSet::new();
        let first = unique_decl_name(
            " 123/集成 ",
            &format!("9 tool/{}", "x".repeat(300)),
            &mut seen,
        );
        let second = unique_decl_name(
            " 123/集成 ",
            &format!("9 tool/{}", "x".repeat(300)),
            &mut seen,
        );
        assert!(valid_decl_name(&first));
        assert!(valid_decl_name(&second));
        assert_ne!(first, second);
        assert!(second.ends_with("_2"));
    }

    #[test]
    fn short_name_collisions_get_suffixes_without_panicking() {
        let mut seen = HashSet::new();
        let names: Vec<_> = (0..3)
            .map(|_| unique_decl_name("a", "b", &mut seen))
            .collect();
        assert!(names.iter().all(|name| valid_decl_name(name)));
        assert_eq!(names[1], format!("{}_2", names[0]));
        assert_eq!(names[2], format!("{}_3", names[0]));
    }

    #[test]
    fn normalized_collision_mapping_is_independent_of_input_order() {
        let mut forward = HashSet::new();
        let a = unique_decl_name("provider/a", "tool name", &mut forward);
        let b = unique_decl_name("provider a", "tool/name", &mut forward);
        let mut reverse = HashSet::new();
        let reverse_b = unique_decl_name("provider a", "tool/name", &mut reverse);
        let reverse_a = unique_decl_name("provider/a", "tool name", &mut reverse);
        assert_eq!(
            (a.as_str(), b.as_str()),
            (reverse_a.as_str(), reverse_b.as_str())
        );
        assert_ne!(a, b);
        assert!(valid_decl_name(&a) && valid_decl_name(&b));
    }
}
