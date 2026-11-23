use super::*;
#[test]
fn schema_sanitizing_keeps_supported_structure_recursively() {
    let schema = json!({
        "type": "object",
        "required": ["items"],
        "properties": {"items": {"type": "array", "items": {
            "type": "string", "format": "ignored", "description": "kept"
        }}},
        "title": "ignored"
    });
    let sanitized = sanitize_schema(&schema).unwrap();
    assert_eq!(sanitized["required"], json!(["items"]));
    assert_eq!(
        sanitized["properties"]["items"]["items"],
        json!({"type": "string", "description": "kept"})
    );
    assert!(sanitized.get("title").is_none());
}
#[test]
fn nullable_union_maps_to_provider_schema_and_other_unions_fail_closed() {
    for union in [json!(["string", "null"]), json!(["null", "string"])] {
        let schema = sanitize_schema(&json!({"type": union})).unwrap();
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["nullable"], true);
    }
    assert_eq!(
        sanitize_schema(&json!({"type": "null"})).unwrap()["type"],
        "null"
    );
    let error = sanitize_schema(&json!({"type": ["string", "number"]})).unwrap_err();
    assert_eq!(error.reason, "schema_type_union");
}
#[test]
fn server_prose_and_enum_payloads_are_bounded_without_losing_required_fields() {
    let long = "word ".repeat(600);
    let enum_values: Vec<_> = (0..100).map(|index| format!("value-{index}")).collect();
    let schema = json!({
        "type": "object",
        "description": format!("{long}\u{0001}"),
        "required": ["must_keep"],
        "properties": {
            "must_keep": {"type": "string", "description": long},
            "choice": {"type": "string", "enum": enum_values}
        }
    });
    let sanitized = sanitize_schema(&schema).expect("bounded ordinary schema");
    assert_eq!(sanitized["required"], json!(["must_keep"]));
    assert_eq!(sanitized["properties"]["must_keep"]["type"], "string");
    assert!(
        sanitized["description"]
            .as_str()
            .is_some_and(|value| value.chars().count() <= MAX_PROSE_CHARS + 1)
    );
    assert_eq!(
        sanitized["properties"]["choice"]["enum"]
            .as_array()
            .map(Vec::len),
        Some(100)
    );
}

#[test]
fn excessive_required_names_fail_closed() {
    let count = MAX_TOTAL_REQUIRED_NAMES + 1;
    let required = (0..count)
        .map(|index| format!("field_{index}"))
        .collect::<Vec<_>>();
    let properties = required
        .iter()
        .map(|name| (name.clone(), json!({"type": "string"})))
        .collect::<serde_json::Map<_, _>>();
    let error = sanitize_schema(&json!({
        "type": "object",
        "required": required,
        "properties": properties,
    }))
    .expect_err("required-name overflow must quarantine the tool");
    assert_eq!(error.reason, "required_names");
    assert_eq!(error.observed, count);
    assert_eq!(error.limit, MAX_TOTAL_REQUIRED_NAMES);
}

#[test]
fn excessive_property_count_and_string_bytes_fail_closed() {
    let property_count = MAX_TOTAL_PROPERTIES + 1;
    let properties = (0..property_count)
        .map(|index| (format!("field_{index}"), json!({"type": "string"})))
        .collect::<serde_json::Map<_, _>>();
    let error = sanitize_schema(&json!({"type": "object", "properties": properties}))
        .expect_err("property overflow must quarantine the tool");
    assert_eq!(error.reason, "properties");
    let properties = (0..200)
        .map(|index| {
            (
                format!("field_{index:03}_{}", "x".repeat(380)),
                json!({"type": "string"}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let error = sanitize_schema(&json!({"type": "object", "properties": properties}))
        .expect_err("total schema string overflow must quarantine the tool");
    assert_eq!(error.reason, "schema_string_bytes");
    assert!(error.observed > error.limit);
}

#[test]
fn oversized_required_and_enum_strings_fail_closed() {
    let long_name = "r".repeat(MAX_FIELD_NAME_BYTES + 1);
    let error = sanitize_schema(&json!({
        "type": "object",
        "required": [long_name.clone()],
        "properties": {long_name: {"type": "string"}},
    }))
    .expect_err("oversized required name must quarantine the tool");
    assert_eq!(error.reason, "required_name_bytes");
    let error = sanitize_schema(&json!({
        "type": "string",
        "enum": ["e".repeat(MAX_ENUM_STRING_BYTES + 1)],
    }))
    .expect_err("oversized enum string must quarantine the tool");
    assert_eq!(error.reason, "enum_string_bytes");
    let error = sanitize_schema(&json!({"type": "string", "enum": [1]})).unwrap_err();
    assert_eq!(error.reason, "enum_value_shape");
}

#[test]
fn structures_beyond_depth_limit_are_rejected_not_weakened() {
    let mut schema = json!({"type": "string"});
    for _ in 0..=MAX_SCHEMA_DEPTH {
        schema = json!({
            "type": "object",
            "required": ["next"],
            "properties": {"next": schema},
        });
    }
    let error =
        sanitize_schema(&schema).expect_err("deep required constraints must not be truncated");
    assert_eq!(error.reason, "schema_depth");
    assert!(error.observed > error.limit);
}

#[test]
fn nullable_any_of_lowers_to_typed_nullable_field() {
    for key in ["anyOf", "oneOf"] {
        let schema = sanitize_schema(&json!({
            "type": "object",
            "properties": {"note": {
                key: [{"type": "string"}, {"type": "null"}],
                "default": null,
                "description": "optional note"
            }}
        }))
        .expect("Optional[str] is expressible");
        assert_eq!(
            schema["properties"]["note"],
            json!({"type": "string", "nullable": true, "description": "optional note"})
        );
    }
}

#[test]
fn local_definitions_are_resolved_with_every_field_kept() {
    let schema = sanitize_schema(&json!({
        "type": "object",
        "required": ["target"],
        "properties": {
            "target": {"$ref": "#/$defs/Target", "description": "where"},
            "fallback": {"anyOf": [{"$ref": "#/definitions/Target"}, {"type": "null"}]}
        },
        "$defs": {"Target": {
            "type": "object",
            "required": ["x"],
            "properties": {"x": {"type": "integer"}, "label": {"type": "string"}}
        }},
        "definitions": {"Target": {
            "type": "object",
            "properties": {"y": {"type": "number"}}
        }}
    }))
    .expect("local refs are expressible");
    let target = &schema["properties"]["target"];
    assert_eq!(target["type"], "object");
    assert_eq!(target["required"], json!(["x"]));
    assert_eq!(target["properties"]["x"]["type"], "integer");
    assert_eq!(target["properties"]["label"]["type"], "string");
    assert_eq!(target["description"], "where");
    let fallback = &schema["properties"]["fallback"];
    assert_eq!(fallback["nullable"], true);
    assert_eq!(fallback["properties"]["y"]["type"], "number");
    assert!(schema.get("$defs").is_none() && schema.get("definitions").is_none());
}

#[test]
fn root_reference_and_single_branch_all_of_are_exact_equivalents() {
    let schema = sanitize_schema(&json!({
        "$ref": "#/$defs/Args",
        "$defs": {
            "Args": {"type": "object", "properties": {"mode": {"allOf": [{"$ref": "#/$defs/Mode"}]}}},
            "Mode": {"type": "string", "enum": ["a", "b"]}
        }
    }))
    .unwrap();
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema["properties"]["mode"],
        json!({"type": "string", "enum": ["a", "b"]})
    );
}

#[test]
fn unsupported_compositions_quarantine_instead_of_weakening() {
    let cases = [
        json!({"anyOf": [{"type": "string"}, {"type": "integer"}]}),
        json!({"oneOf": [{"type": "string"}, {"type": "null"}, {"type": "integer"}]}),
        json!({"allOf": [{"type": "object"}, {"required": ["a"]}]}),
        json!({"anyOf": [{"type": "string"}, {"type": "null"}], "maxLength": 3, "enum": ["a"]}),
        json!({"$ref": "#/$defs/Missing", "$defs": {}}),
        json!({"$ref": "https://example.invalid/schema.json"}),
        json!({"$ref": "#/properties/a"}),
        json!({"not": {"type": "string"}}),
        json!({"type": "string", "$ref": "#/$defs/Mode", "$defs": {"Mode": {"type": "string"}}}),
    ];
    for case in cases {
        let error = sanitize_schema(&json!({"type": "object", "properties": {"field": case}}))
            .expect_err("unsupported composition must quarantine");
        assert!(
            error.reason.starts_with("schema_composition") || error.reason == "schema_ref",
            "{case}: {error:?}"
        );
    }
}

#[test]
fn reference_cycles_end_at_the_depth_cap() {
    let error = sanitize_schema(&json!({
        "$ref": "#/$defs/Node",
        "$defs": {"Node": {"type": "object", "properties": {
            "child": {"anyOf": [{"$ref": "#/$defs/Node"}, {"type": "null"}]}
        }}}
    }))
    .expect_err("an unbounded recursive model cannot be lowered");
    assert_eq!(error.reason, "schema_depth");
}
