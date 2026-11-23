use serde_json::{Value, json};

mod composition;

const MAX_PROSE_CHARS: usize = 512;
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

/// Reduce MCP JSON-Schema to the supported provider-wire subset.
pub(super) fn sanitize_schema(schema: &Value) -> Result<Value, SchemaCompatibilityError> {
    let mut budget = SchemaBudget::default();
    let sanitized = sanitize_schema_at_depth(schema, 0, schema, &mut budget)?;
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
/// `root` is the tool's whole input schema, against which local `$ref`s resolve.
fn sanitize_schema_at_depth(
    schema: &Value,
    depth: usize,
    root: &Value,
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
    if let Some(expanded) = composition::expand(map, root)? {
        return sanitize_schema_at_depth(&expanded, depth + 1, root, budget);
    }
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
                sanitize_schema_at_depth(property_schema, depth + 1, root, budget)?,
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
            sanitize_schema_at_depth(value, depth + 1, root, budget)?,
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
mod tests;
