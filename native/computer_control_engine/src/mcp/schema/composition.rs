//! Lowering of JSON-Schema composition (`$ref`, `anyOf`, `oneOf`, `allOf`) to
//! the provider-wire subset. Only exact equivalents are lowered; every other
//! composition quarantines the tool rather than emitting a weakened contract.

use serde_json::{Map, Value};

use super::SchemaCompatibilityError;

const COMPOSITION_KEYS: &[&str] = &["$ref", "anyOf", "oneOf", "allOf"];
const UNSUPPORTED_KEYS: &[&str] = &["not", "if", "then", "else", "$dynamicRef", "$recursiveRef"];
/// Keys that may accompany a composition without changing the accepted values.
/// Any other sibling would have to be intersected with the branch, which the
/// provider subset cannot express exactly.
const ANNOTATION_SIBLINGS: &[&str] = &[
    "description",
    "title",
    "default",
    "examples",
    "nullable",
    "deprecated",
    "$comment",
    "$schema",
    "$id",
    "$defs",
    "definitions",
];
const DEFINITION_ROOTS: &[&str] = &["/$defs/", "/definitions/"];

/// Return the equivalent schema node with one level of composition lowered,
/// or `None` when `map` uses no composition keyword. The caller sanitizes the
/// result one level deeper, so reference cycles end at the schema depth cap.
pub(super) fn expand(
    map: &Map<String, Value>,
    root: &Value,
) -> Result<Option<Value>, SchemaCompatibilityError> {
    if UNSUPPORTED_KEYS.iter().any(|key| map.contains_key(*key)) {
        return Err(unsupported());
    }
    let present: Vec<&str> = COMPOSITION_KEYS
        .iter()
        .copied()
        .filter(|key| map.contains_key(*key))
        .collect();
    let keyword = match present.as_slice() {
        [] => return Ok(None),
        [keyword] => *keyword,
        _ => return Err(unsupported()),
    };
    if map
        .keys()
        .any(|key| key != keyword && !ANNOTATION_SIBLINGS.contains(&key.as_str()))
    {
        return Err(unsupported());
    }
    let (branch, nullable) = match keyword {
        "$ref" => (resolve_local_ref(&map[keyword], root)?, false),
        _ => single_or_nullable_branch(&map[keyword])?,
    };
    let mut expanded = branch
        .as_object()
        .cloned()
        .ok_or_else(|| SchemaCompatibilityError::new("schema_composition_branch", 1, 0))?;
    for (key, value) in map {
        if key != keyword && key != "$defs" && key != "definitions" {
            expanded.insert(key.clone(), value.clone());
        }
    }
    if nullable {
        expanded.insert("nullable".to_string(), Value::Bool(true));
    }
    Ok(Some(Value::Object(expanded)))
}

fn unsupported() -> SchemaCompatibilityError {
    SchemaCompatibilityError::new("schema_composition", 1, 0)
}

/// A one-branch composition is that branch. A two-branch `anyOf`/`oneOf` whose
/// other branch is exactly `{"type": "null"}` is that branch made nullable.
fn single_or_nullable_branch(value: &Value) -> Result<(&Value, bool), SchemaCompatibilityError> {
    let branches = value
        .as_array()
        .ok_or_else(|| SchemaCompatibilityError::new("schema_composition_shape", 1, 0))?;
    match branches.as_slice() {
        [only] => Ok((only, false)),
        [first, second] if is_null_branch(second) && !is_null_branch(first) => Ok((first, true)),
        [first, second] if is_null_branch(first) && !is_null_branch(second) => Ok((second, true)),
        _ => Err(SchemaCompatibilityError::new(
            "schema_composition",
            branches.len(),
            1,
        )),
    }
}

fn is_null_branch(value: &Value) -> bool {
    value.as_object().is_some_and(|branch| {
        branch.get("type").and_then(Value::as_str) == Some("null")
            && branch
                .keys()
                .all(|key| key == "type" || ANNOTATION_SIBLINGS.contains(&key.as_str()))
    })
}

/// Resolve a reference to a named definition in the same document. Remote,
/// nested-pointer and missing references are rejected.
fn resolve_local_ref<'a>(
    value: &Value,
    root: &'a Value,
) -> Result<&'a Value, SchemaCompatibilityError> {
    let reference = value
        .as_str()
        .ok_or_else(|| SchemaCompatibilityError::new("schema_ref", 1, 0))?;
    let pointer = reference
        .strip_prefix('#')
        .filter(|pointer| {
            DEFINITION_ROOTS.iter().any(|prefix| {
                pointer
                    .strip_prefix(prefix)
                    .is_some_and(|name| !name.is_empty() && !name.contains('/'))
            })
        })
        .ok_or_else(|| SchemaCompatibilityError::new("schema_ref", 1, 0))?;
    root.pointer(pointer)
        .ok_or_else(|| SchemaCompatibilityError::new("schema_ref", 1, 0))
}
