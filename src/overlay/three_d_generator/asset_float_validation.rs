use std::borrow::Cow;
use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{AccessorInfo, BufferViewInfo, MAX_GLTF_ABSOLUTE_RENDERER_VALUE};

pub(super) const POSITION_BOUNDS_ABSOLUTE_TOLERANCE: f64 = 1.0 / 32_768.0;
pub(super) const POSITION_BOUNDS_RELATIVE_TOLERANCE: f64 = 4.0 * f32::EPSILON as f64;

pub(super) fn validate(
    root: &Map<String, Value>,
    accessors: &[AccessorInfo],
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    let mut position_accessors = HashSet::new();
    let mut renderer_accessors = HashSet::new();
    for mesh in root
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?
    {
        for primitive in mesh
            .as_object()
            .and_then(|mesh| mesh.get("primitives"))
            .and_then(Value::as_array)
            .ok_or_else(invalid)?
        {
            let primitive = primitive.as_object().ok_or_else(invalid)?;
            let attributes = primitive
                .get("attributes")
                .and_then(Value::as_object)
                .ok_or_else(invalid)?;
            for (semantic, value) in attributes {
                let index = index_at(value, accessors.len())?;
                renderer_accessors.insert(index);
                if semantic == "POSITION" {
                    position_accessors.insert(index);
                }
            }
            if let Some(targets) = primitive.get("targets") {
                for target in targets.as_array().ok_or_else(invalid)? {
                    for (semantic, value) in target.as_object().ok_or_else(invalid)? {
                        let index = index_at(value, accessors.len())?;
                        renderer_accessors.insert(index);
                        if semantic == "POSITION" {
                            position_accessors.insert(index);
                        }
                    }
                }
            }
        }
    }
    for index in renderer_accessors {
        let accessor = accessors.get(index).ok_or_else(invalid)?;
        if accessor.component_type == 5126 {
            scan_float_accessor(
                root,
                index,
                *accessor,
                views,
                buffers,
                position_accessors.contains(&index),
            )?;
        }
    }
    Ok(())
}

fn scan_float_accessor(
    root: &Map<String, Value>,
    index: usize,
    accessor: AccessorInfo,
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
    position: bool,
) -> Result<(), String> {
    let view = views.get(accessor.buffer_view).ok_or_else(invalid)?;
    let buffer = buffers.get(view.buffer).ok_or_else(invalid)?;
    let stride = accessor.byte_stride.unwrap_or(
        accessor
            .component_count
            .checked_mul(4)
            .ok_or_else(invalid)?,
    );
    let declared_bounds = if position {
        let accessor_json = root
            .get("accessors")
            .and_then(Value::as_array)
            .and_then(|accessors| accessors.get(index))
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        match (accessor_json.get("min"), accessor_json.get("max")) {
            (Some(minimum), Some(maximum)) => Some((
                number_array(minimum, accessor.component_count as usize)?,
                number_array(maximum, accessor.component_count as usize)?,
            )),
            (None, None) => None,
            _ => return Err(invalid()),
        }
    } else {
        None
    };
    for element in 0..accessor.count {
        for component in 0..accessor.component_count {
            let start = accessor
                .absolute_offset
                .checked_add(element.checked_mul(stride).ok_or_else(invalid)?)
                .and_then(|value| value.checked_add(component * 4))
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(invalid)?;
            let end = start.checked_add(4).ok_or_else(invalid)?;
            let bytes = buffer.get(start..end).ok_or_else(invalid)?;
            let value = f32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?) as f64;
            if !value.is_finite() || value.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE {
                return Err(invalid());
            }
            if let Some((minimum, maximum)) = &declared_bounds {
                let component = component as usize;
                if !position_bound_contains(value, minimum[component], maximum[component]) {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

fn position_bound_contains(value: f64, minimum: f64, maximum: f64) -> bool {
    let minimum_tolerance = position_bound_tolerance(value, minimum);
    let maximum_tolerance = position_bound_tolerance(value, maximum);
    value >= minimum - minimum_tolerance && value <= maximum + maximum_tolerance
}

fn position_bound_tolerance(value: f64, bound: f64) -> f64 {
    let relative = value
        .abs()
        .max(bound.abs())
        .mul_add(POSITION_BOUNDS_RELATIVE_TOLERANCE, 0.0);
    POSITION_BOUNDS_ABSOLUTE_TOLERANCE.max(relative)
}

fn index_at(value: &Value, length: usize) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < length)
        .ok_or_else(invalid)
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
