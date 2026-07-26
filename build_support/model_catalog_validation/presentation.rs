use std::collections::HashSet;

pub(super) fn validate_variants(
    models: &[serde_json::Value],
    variants: &serde_json::Map<String, serde_json::Value>,
) {
    assert!(
        !variants.is_empty(),
        "presentation_variants must not be empty"
    );
    for (key, value) in variants {
        let variant = value
            .as_object()
            .unwrap_or_else(|| panic!("presentation variant {key:?} must be an object"));
        assert!(
            variant.len() == 3
                && ["suffix_vi", "suffix_ko", "suffix_en"]
                    .iter()
                    .all(|field| variant.contains_key(*field)),
            "presentation variant {key:?} must define only the three localized suffixes"
        );
        for field in ["suffix_vi", "suffix_ko", "suffix_en"] {
            let suffix = super::string(variant, field);
            assert!(
                suffix
                    .strip_prefix(" (")
                    .and_then(|value| value.strip_suffix(')'))
                    .is_some_and(|value| !value.is_empty() && value.trim() == value),
                "{field} for presentation variant {key:?} must be a parenthesized suffix"
            );
        }
    }

    let mut used = HashSet::new();
    for item in models {
        let model = item.as_object().expect("model entries must be objects");
        let Some(value) = model.get("presentation_variant") else {
            continue;
        };
        let key = value.as_str().unwrap_or_else(|| {
            panic!(
                "presentation_variant for {:?} must be a string",
                super::string(model, "id")
            )
        });
        assert!(
            variants.contains_key(key),
            "unknown presentation variant {key:?} for {:?}",
            super::string(model, "id")
        );
        used.insert(key);
        let sibling_count = models
            .iter()
            .filter_map(serde_json::Value::as_object)
            .filter(|candidate| {
                ["provider", "full_name", "model_type"]
                    .iter()
                    .all(|field| candidate.get(*field) == model.get(*field))
            })
            .count();
        assert!(
            sibling_count > 1,
            "presentation variant on {:?} requires a behavioral sibling using the same endpoint",
            super::string(model, "id")
        );
    }
    assert_eq!(
        used.len(),
        variants.len(),
        "presentation_variants contains an unreferenced variant"
    );
}

pub(super) fn localized_name(
    model: &serde_json::Map<String, serde_json::Value>,
    profile: &serde_json::Map<String, serde_json::Value>,
    variants: &serde_json::Map<String, serde_json::Value>,
    language: &str,
) -> String {
    let mut name = super::string(profile, language).to_string();
    let Some(key) = model
        .get("presentation_variant")
        .and_then(serde_json::Value::as_str)
    else {
        return name;
    };
    let suffix_field = match language {
        "name_vi" => "suffix_vi",
        "name_ko" => "suffix_ko",
        "name_en" => "suffix_en",
        _ => unreachable!(),
    };
    let variant = variants[key].as_object().unwrap();
    name.push_str(super::string(variant, suffix_field));
    name
}
