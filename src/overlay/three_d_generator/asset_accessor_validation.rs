use serde_json::{Map, Value};

use super::{
    AccessorInfo, BufferViewInfo, MAX_GLTF_ABSOLUTE_RENDERER_VALUE, MAX_GLTF_ACCESSOR_ELEMENTS,
    MAX_GLTF_ACCESSORS,
};

pub(super) fn validate(
    root: &Map<String, Value>,
    views: &[BufferViewInfo],
) -> Result<Vec<AccessorInfo>, String> {
    let invalid = || "The model result description is invalid.".to_string();
    let accessors = root
        .get("accessors")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if accessors.is_empty() || accessors.len() > MAX_GLTF_ACCESSORS {
        return Err(invalid());
    }
    let mut aggregate_elements = 0_u64;
    let mut validated = Vec::with_capacity(accessors.len());
    for accessor in accessors {
        let accessor = accessor.as_object().ok_or_else(invalid)?;
        if accessor.contains_key("sparse") {
            return Err(invalid());
        }
        let view = accessor
            .get("bufferView")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < views.len())
            .ok_or_else(invalid)?;
        let component_type = accessor
            .get("componentType")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        let component_size = match component_type {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(invalid()),
        };
        let accessor_type = accessor
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        let component_count = match accessor_type {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" | "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            _ => return Err(invalid()),
        };
        let count = accessor
            .get("count")
            .and_then(Value::as_u64)
            .filter(|count| *count > 0)
            .ok_or_else(invalid)?;
        aggregate_elements = aggregate_elements
            .checked_add(count)
            .filter(|total| *total <= MAX_GLTF_ACCESSOR_ELEMENTS)
            .ok_or_else(invalid)?;
        let normalized = match accessor.get("normalized") {
            Some(value) => value.as_bool().ok_or_else(invalid)?,
            None => false,
        };
        if normalized && matches!(component_type, 5125 | 5126) {
            return Err(invalid());
        }
        validate_bounds(accessor, component_type, component_count)?;
        let element_size = element_size(accessor_type, component_size, component_count)?;
        let stride = views[view].byte_stride.unwrap_or(element_size);
        if stride < element_size || stride > 252 || !stride.is_multiple_of(component_size) {
            return Err(invalid());
        }
        let byte_offset = accessor
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let absolute_offset = views[view]
            .byte_offset
            .checked_add(byte_offset)
            .ok_or_else(invalid)?;
        if byte_offset % component_size != 0 || absolute_offset % component_size != 0 {
            return Err(invalid());
        }
        let occupied = if views[view].byte_stride.is_some() {
            if byte_offset % stride + element_size > stride {
                return Err(invalid());
            }
            byte_offset
                .checked_div(stride)
                .and_then(|slice| slice.checked_mul(stride))
                .and_then(|start| {
                    count
                        .checked_mul(stride)
                        .and_then(|size| start.checked_add(size))
                })
        } else {
            count
                .checked_mul(element_size)
                .and_then(|size| byte_offset.checked_add(size))
        }
        .filter(|occupied| *occupied <= views[view].length)
        .ok_or_else(invalid)?;
        debug_assert!(occupied > 0);
        validated.push(AccessorInfo {
            count,
            component_type,
            component_count,
            buffer_view: view,
            byte_offset,
            absolute_offset,
            byte_stride: views[view].byte_stride,
            normalized,
        });
    }
    Ok(validated)
}

fn element_size(
    accessor_type: &str,
    component_size: u64,
    component_count: u64,
) -> Result<u64, String> {
    if matches!(accessor_type, "MAT2" | "MAT3") {
        let rows = if accessor_type == "MAT2" { 2 } else { 3 };
        let column = rows * component_size;
        let aligned = column
            .checked_add(3)
            .map(|value| value & !3)
            .ok_or_else(invalid)?;
        return aligned.checked_mul(rows).ok_or_else(invalid);
    }
    component_size
        .checked_mul(component_count)
        .ok_or_else(invalid)
}

fn validate_bounds(
    accessor: &Map<String, Value>,
    component_type: u64,
    component_count: u64,
) -> Result<(), String> {
    let (minimum, maximum) = match (accessor.get("min"), accessor.get("max")) {
        (None, None) => return Ok(()),
        (Some(minimum), Some(maximum)) => (minimum, maximum),
        _ => return Err(invalid()),
    };
    let minimum = number_array(minimum, component_count as usize)?;
    let maximum = number_array(maximum, component_count as usize)?;
    for (minimum, maximum) in minimum.into_iter().zip(maximum) {
        if minimum > maximum
            || component_type == 5126
                && (minimum.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE
                    || maximum.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn number_array(value: &Value, length: usize) -> Result<Vec<f64>, String> {
    value
        .as_array()
        .filter(|values| values.len() == length)
        .ok_or_else(invalid)?
        .iter()
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(invalid)
        })
        .collect()
}

fn invalid() -> String {
    "The model result description is invalid.".to_string()
}
