use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_PROSE_CHARS: usize = 512;
const MAX_DECL_NAME_BYTES: usize = 128;
const MAX_SCHEMA_DEPTH: usize = 16;
const MAX_SCHEMA_NODES: usize = 1_024;
const MAX_TOTAL_PROPERTIES: usize = 512;
const MAX_TOTAL_REQUIRED_NAMES: usize = 256;
const MAX_FIELD_NAME_BYTES: usize = 512;
const MAX_SCHEMA_STRING_BYTES: usize = 64 * 1_024;
const MAX_ENUM_ITEMS: usize = 256;
const MAX_ENUM_STRING_BYTES: usize = 4 * 1_024;
const MAX_SANITIZED_SCHEMA_BYTES: usize = 96 * 1_024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SchemaCompatibilityError {
    pub(super) reason: &'static str,
    pub(super) observed: usize,
    pub(super) limit: usize,
}

impl SchemaCompatibilityError {
    fn new(reason: &'static str, observed: usize, limit: usize) -> Self {
        Self {
            reason,
            observed,
            limit,
        }
    }
}

/// Normalize server-authored prose and keep it within setup-safe bounds.
pub(super) fn bounded_prose(value: &str) -> String {
    let mut bounded = String::with_capacity(MAX_PROSE_CHARS + 3);
    let mut count = 0usize;
    let mut pending_space = false;
    let mut truncated = false;
    for character in value.chars() {
        if character.is_whitespace() || character.is_control() {
            pending_space = !bounded.is_empty();
            continue;
        }
        if pending_space {
            if count == MAX_PROSE_CHARS {
                truncated = true;
                break;
            }
            bounded.push(' ');
            count += 1;
            pending_space = false;
        }
        if count == MAX_PROSE_CHARS {
            truncated = true;
            break;
        }
        bounded.push(character);
        count += 1;
    }
    if truncated {
        bounded.push('…');
    }
    bounded
}

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
        let keep = MAX_DECL_NAME_BYTES - suffix.len();
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

/// Reduce MCP JSON-Schema to the supported provider-wire subset.
pub(super) fn sanitize_schema(schema: &Value) -> Result<Value, SchemaCompatibilityError> {
    let mut budget = SchemaBudget::default();
    let sanitized = sanitize_schema_at_depth(schema, 0, &mut budget)?;
    let output_bytes = serde_json::to_vec(&sanitized)
        .map_err(|_| SchemaCompatibilityError::new("schema_serialization", 1, 0))?
        .len();
    if output_bytes > MAX_SANITIZED_SCHEMA_BYTES {
        return Err(SchemaCompatibilityError::new(
            "sanitized_schema_bytes",
            output_bytes,
            MAX_SANITIZED_SCHEMA_BYTES,
        ));
    }
    Ok(sanitized)
}

#[derive(Default)]
struct SchemaBudget {
    nodes: usize,
    properties: usize,
    required_names: usize,
    string_bytes: usize,
}

impl SchemaBudget {
    fn charge(
        current: &mut usize,
        amount: usize,
        limit: usize,
        reason: &'static str,
    ) -> Result<(), SchemaCompatibilityError> {
        let observed = current.checked_add(amount).unwrap_or(usize::MAX);
        if observed > limit {
            return Err(SchemaCompatibilityError::new(reason, observed, limit));
        }
        *current = observed;
        Ok(())
    }

    fn charge_string(&mut self, value: &str) -> Result<(), SchemaCompatibilityError> {
        Self::charge(
            &mut self.string_bytes,
            value.len(),
            MAX_SCHEMA_STRING_BYTES,
            "schema_string_bytes",
        )
    }
}

/// Deep structures are rejected once the depth budget is exceeded. Replacing
/// them with an empty object would discard nested `required` constraints, so the
/// caller quarantines the whole tool instead of emitting a weakened contract.
fn sanitize_schema_at_depth(
    schema: &Value,
    depth: usize,
    budget: &mut SchemaBudget,
) -> Result<Value, SchemaCompatibilityError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(SchemaCompatibilityError::new(
            "schema_depth",
            depth,
            MAX_SCHEMA_DEPTH,
        ));
    }
    SchemaBudget::charge(&mut budget.nodes, 1, MAX_SCHEMA_NODES, "schema_nodes")?;
    let Value::Object(map) = schema else {
        return Ok(json!({"type": "object", "properties": {}}));
    };
    let mut out = serde_json::Map::new();
    let mut nullable = false;
    if let Some(value) = map.get("type") {
        let (schema_type, type_nullable) = sanitize_type(value, budget)?;
        out.insert("type".to_string(), schema_type);
        nullable = type_nullable;
    }
    if let Some(value) = map.get("nullable") {
        nullable |= value
            .as_bool()
            .ok_or_else(|| SchemaCompatibilityError::new("nullable_shape", 1, 0))?;
    }
    if nullable {
        out.insert("nullable".to_string(), Value::Bool(nullable));
    }
    if let Some(value) = map.get("description") {
        let description = value
            .as_str()
            .ok_or_else(|| SchemaCompatibilityError::new("description_shape", 1, 0))?;
        let description = bounded_prose(description);
        budget.charge_string(&description)?;
        out.insert("description".to_string(), Value::String(description));
    }
    if let Some(value) = map.get("enum") {
        out.insert("enum".to_string(), sanitize_enum(value, budget)?);
    }

    let required = sanitize_required(map.get("required"), budget)?;
    if map.contains_key("required") {
        out.insert(
            "required".to_string(),
            Value::Array(required.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(value) = map.get("properties") {
        let properties = value
            .as_object()
            .ok_or_else(|| SchemaCompatibilityError::new("properties_shape", 1, 0))?;
        SchemaBudget::charge(
            &mut budget.properties,
            properties.len(),
            MAX_TOTAL_PROPERTIES,
            "properties",
        )?;
        let missing_required = required
            .iter()
            .filter(|name| !properties.contains_key(name.as_str()))
            .count();
        if missing_required > 0 {
            return Err(SchemaCompatibilityError::new(
                "required_without_property",
                missing_required,
                0,
            ));
        }
        let mut sanitized = serde_json::Map::new();
        for (name, property_schema) in properties {
            if name.len() > MAX_FIELD_NAME_BYTES {
                return Err(SchemaCompatibilityError::new(
                    "property_name_bytes",
                    name.len(),
                    MAX_FIELD_NAME_BYTES,
                ));
            }
            budget.charge_string(name)?;
            sanitized.insert(
                name.clone(),
                sanitize_schema_at_depth(property_schema, depth + 1, budget)?,
            );
        }
        out.insert("properties".to_string(), Value::Object(sanitized));
    } else if !required.is_empty() {
        return Err(SchemaCompatibilityError::new(
            "required_without_property",
            required.len(),
            0,
        ));
    }
    if let Some(value) = map.get("items") {
        out.insert(
            "items".to_string(),
            sanitize_schema_at_depth(value, depth + 1, budget)?,
        );
    }
    if out.get("type").and_then(Value::as_str) == Some("object") && !out.contains_key("properties")
    {
        out.insert("properties".to_string(), json!({}));
    }
    if !out.contains_key("type") {
        out.insert("type".to_string(), json!("object"));
        out.entry("properties").or_insert_with(|| json!({}));
    }
    Ok(Value::Object(out))
}

fn sanitize_required(
    value: Option<&Value>,
    budget: &mut SchemaBudget,
) -> Result<Vec<String>, SchemaCompatibilityError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| SchemaCompatibilityError::new("required_shape", 1, 0))?;
    SchemaBudget::charge(
        &mut budget.required_names,
        values.len(),
        MAX_TOTAL_REQUIRED_NAMES,
        "required_names",
    )?;
    let mut required = Vec::with_capacity(values.len());
    for value in values {
        let name = value
            .as_str()
            .ok_or_else(|| SchemaCompatibilityError::new("required_name_shape", 1, 0))?;
        if name.len() > MAX_FIELD_NAME_BYTES {
            return Err(SchemaCompatibilityError::new(
                "required_name_bytes",
                name.len(),
                MAX_FIELD_NAME_BYTES,
            ));
        }
        budget.charge_string(name)?;
        required.push(name.to_string());
    }
    Ok(required)
}

fn sanitize_type(
    value: &Value,
    budget: &mut SchemaBudget,
) -> Result<(Value, bool), SchemaCompatibilityError> {
    const ALLOWED: &[&str] = &[
        "object", "array", "string", "number", "integer", "boolean", "null",
    ];
    match value {
        Value::String(value) if ALLOWED.contains(&value.as_str()) => {
            budget.charge_string(value)?;
            Ok((Value::String(value.clone()), false))
        }
        Value::Array(values) if values.len() == 2 => {
            let first = values[0]
                .as_str()
                .ok_or_else(|| SchemaCompatibilityError::new("schema_type_union", 2, 1))?;
            let second = values[1]
                .as_str()
                .ok_or_else(|| SchemaCompatibilityError::new("schema_type_union", 2, 1))?;
            let concrete = match (first, second) {
                ("null", concrete) | (concrete, "null")
                    if concrete != "null" && ALLOWED.contains(&concrete) =>
                {
                    concrete
                }
                _ => {
                    return Err(SchemaCompatibilityError::new(
                        "schema_type_union",
                        values.len(),
                        1,
                    ));
                }
            };
            budget.charge_string(concrete)?;
            Ok((Value::String(concrete.to_string()), true))
        }
        _ => Err(SchemaCompatibilityError::new("schema_type", 1, 0)),
    }
}

fn sanitize_enum(
    value: &Value,
    budget: &mut SchemaBudget,
) -> Result<Value, SchemaCompatibilityError> {
    let values = value
        .as_array()
        .ok_or_else(|| SchemaCompatibilityError::new("enum_shape", 1, 0))?;
    if values.len() > MAX_ENUM_ITEMS {
        return Err(SchemaCompatibilityError::new(
            "enum_items",
            values.len(),
            MAX_ENUM_ITEMS,
        ));
    }
    for value in values {
        match value {
            Value::String(text) => {
                if text.len() > MAX_ENUM_STRING_BYTES {
                    return Err(SchemaCompatibilityError::new(
                        "enum_string_bytes",
                        text.len(),
                        MAX_ENUM_STRING_BYTES,
                    ));
                }
                budget.charge_string(text)?;
            }
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Array(_)
            | Value::Object(_) => {
                return Err(SchemaCompatibilityError::new("enum_value_shape", 1, 0));
            }
        }
    }
    Ok(Value::Array(values.clone()))
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
}
