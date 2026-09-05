use super::{AccessorInfo, BufferViewInfo, MAX_GLTF_ABSOLUTE_RENDERER_VALUE};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::HashSet;

pub(super) const MAX_CLIPS: usize = 64;
pub(super) const MAX_CHANNELS: usize = 2048;
pub(super) const MAX_KEYS: u64 = 1_000_000;
pub(super) const MAX_COMPONENTS: u64 = 6_000_000;
pub(super) const MAX_DURATION: f64 = 3600.0;

pub(super) fn validate(
    root: &Map<String, Value>,
    accessors: &[AccessorInfo],
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    let Some(clips) = root.get("animations") else {
        return Ok(());
    };
    let clips = clips
        .as_array()
        .filter(|v| v.len() <= MAX_CLIPS)
        .ok_or_else(invalid)?;
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let mut channels_used = 0;
    let mut keys_used = 0;
    let mut components_used = 0;
    for clip in clips {
        let samplers = table(clip, "samplers")?;
        let channels = table(clip, "channels")?;
        channels_used += channels.len();
        if channels_used > MAX_CHANNELS || samplers.len() > MAX_CHANNELS {
            return Err(invalid());
        }
        let mut targets = HashSet::new();
        let mut used = HashSet::new();
        for channel in channels {
            let sampler_index = index(channel.get("sampler"), samplers.len())?;
            used.insert(sampler_index);
            let sampler = &samplers[sampler_index];
            let target = channel.get("target").ok_or_else(invalid)?;
            let node_index = index(target.get("node"), nodes.len())?;
            let node = &nodes[node_index];
            let path = target
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !targets.insert((node_index, path)) {
                return Err(invalid());
            }
            let width = match path {
                "translation" | "scale" => 3,
                "rotation" => 4,
                "weights" => morph_width(root, node)?,
                _ => return Err(invalid()),
            };
            if path != "weights" && node.get("matrix").is_some() {
                return Err(invalid());
            }
            let input_index = index(sampler.get("input"), accessors.len())?;
            let output_index = index(sampler.get("output"), accessors.len())?;
            let input = accessors[input_index];
            let output = accessors[output_index];
            let interpolation = sampler
                .get("interpolation")
                .map(Value::as_str)
                .unwrap_or(Some("LINEAR"))
                .ok_or_else(invalid)?;
            let factor = match interpolation {
                "LINEAR" | "STEP" => 1,
                "CUBICSPLINE" => 3,
                _ => return Err(invalid()),
            };
            let output_width = if path == "weights" { 1 } else { width };
            if input.count == 0
                || input.component_type != 5126
                || input.component_count != 1
                || input.normalized
                || input.byte_stride.is_some()
                || output.component_type != 5126
                || output.component_count != output_width
                || output.normalized
                || output.byte_stride.is_some()
            {
                return Err(invalid());
            }
            keys_used += input.count;
            let count = input
                .count
                .checked_mul(width)
                .and_then(|v| v.checked_mul(factor))
                .ok_or_else(invalid)?;
            components_used += count;
            if keys_used > MAX_KEYS
                || components_used > MAX_COMPONENTS
                || output.count.checked_mul(output_width) != Some(count)
            {
                return Err(invalid());
            }
            let read_time = |key| scalar(input, key, views, buffers);
            let read_output = |key, component, slot| {
                scalar(
                    output,
                    (key * factor + slot) * width + component,
                    views,
                    buffers,
                )
            };
            let mut previous = None;
            for key in 0..input.count {
                let time = read_time(key)?;
                if !(0.0..=MAX_DURATION).contains(&time) || previous.is_some_and(|p| time <= p) {
                    return Err(invalid());
                }
                let value_slot = u64::from(factor == 3);
                let mut quaternion_length = 0.0;
                for component in 0..width {
                    let value = read_output(key, component, value_slot)?;
                    quaternion_length += value * value;
                    if factor == 3 {
                        let incoming = read_output(key, component, 0)?;
                        read_output(key, component, 2)?;
                        if let Some(previous_time) = previous {
                            let interval = time - previous_time;
                            let previous_value = read_output(key - 1, component, 1)?;
                            let outgoing = read_output(key - 1, component, 2)?;
                            // Hermite's equivalent Bezier hull bounds every interpolated value.
                            bounded(previous_value + outgoing * interval / 3.0)?;
                            bounded(value - incoming * interval / 3.0)?;
                        }
                    }
                }
                if path == "rotation" && (quaternion_length - 1.0).abs() > 0.02 {
                    return Err(invalid());
                }
                previous = Some(time);
            }
        }
        if used.len() != samplers.len() {
            return Err(invalid());
        }
    }
    Ok(())
}

fn morph_width(root: &Map<String, Value>, node: &Value) -> Result<u64, String> {
    let meshes = root
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let mesh = &meshes[index(node.get("mesh"), meshes.len())?];
    let primitives = table(mesh, "primitives")?;
    let mut width = None;
    for primitive in primitives {
        let count = table(primitive, "targets")?.len() as u64;
        if count > 256 || width.is_some_and(|value| value != count) {
            return Err(invalid());
        }
        width = Some(count);
    }
    width.ok_or_else(invalid)
}

fn table<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    value
        .get(name)
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
        .map(Vec::as_slice)
        .ok_or_else(invalid)
}

fn index(value: Option<&Value>, length: usize) -> Result<usize, String> {
    value
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v < length)
        .ok_or_else(invalid)
}

fn scalar(
    accessor: AccessorInfo,
    index: u64,
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<f64, String> {
    let view = views.get(accessor.buffer_view).ok_or_else(invalid)?;
    let start = accessor
        .absolute_offset
        .checked_add(index.checked_mul(4).ok_or_else(invalid)?)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(invalid)?;
    let bytes = buffers
        .get(view.buffer)
        .and_then(|v| v.get(start..start.checked_add(4)?))
        .ok_or_else(invalid)?;
    let value = f32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?) as f64;
    bounded(value)?;
    Ok(value)
}

fn bounded(value: f64) -> Result<(), String> {
    if value.is_finite() && value.abs() <= MAX_GLTF_ABSOLUTE_RENDERER_VALUE {
        Ok(())
    } else {
        Err(invalid())
    }
}

fn invalid() -> String {
    "The model result contains invalid animation data.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_animation_cases_match_the_product_contract() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/image-to-3d/animation-contract.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let floats = |field: &str| {
                case[field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                check(
                    clip(case["interpolation"].as_str().unwrap(), "translation"),
                    &floats("times"),
                    &floats("values"),
                    3
                ),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"],
            );
        }
    }

    fn check(root: Value, times: &[f32], values: &[f32], width: u64) -> bool {
        let bytes: Vec<u8> = times
            .iter()
            .chain(values)
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let accessors = [
            AccessorInfo {
                count: times.len() as u64,
                component_type: 5126,
                component_count: 1,
                ..Default::default()
            },
            AccessorInfo {
                count: values.len() as u64 / width,
                component_type: 5126,
                component_count: width,
                absolute_offset: times.len() as u64 * 4,
                ..Default::default()
            },
        ];
        let views = [BufferViewInfo {
            buffer: 0,
            byte_offset: 0,
            length: bytes.len() as u64,
            byte_stride: None,
        }];
        validate(
            root.as_object().unwrap(),
            &accessors,
            &views,
            &[Cow::Borrowed(&bytes)],
        )
        .is_ok()
    }

    fn clip(interpolation: &str, path: &str) -> Value {
        json!({"nodes":[{}],"animations":[{
            "samplers":[{"input":0,"output":1,"interpolation":interpolation}],
            "channels":[{"sampler":0,"target":{"node":0,"path":path}}]
        }]})
    }

    #[test]
    fn finite_transform_tracks_accept_all_standard_interpolations() {
        for interpolation in ["LINEAR", "STEP"] {
            assert!(check(
                clip(interpolation, "translation"),
                &[0., 1.],
                &[0., 0., 0., 1., 0., 0.],
                3
            ));
        }
        assert!(check(
            clip("CUBICSPLINE", "translation"),
            &[0., 1.],
            &[0.; 18],
            3
        ));
    }

    #[test]
    fn malformed_timing_targets_counts_and_values_fail_closed() {
        for times in [[0., 0.], [1., 0.], [-1., 1.], [0., 3601.], [0., f32::NAN]] {
            assert!(!check(clip("LINEAR", "translation"), &times, &[0.; 6], 3));
        }
        assert!(!check(
            clip("LINEAR", "translation"),
            &[0., 1.],
            &[0.; 3],
            3
        ));
        assert!(!check(clip("LINEAR", "rotation"), &[0., 1.], &[0.; 8], 4));
        assert!(!check(
            clip("LINEAR", "translation"),
            &[0., 1.],
            &[f32::INFINITY; 6],
            3
        ));
        let mut root = clip("LINEAR", "translation");
        root["animations"][0]["channels"][0]["target"]["node"] = json!(1);
        assert!(!check(root, &[0., 1.], &[0.; 6], 3));
    }

    #[test]
    fn duplicate_tracks_and_cubic_overshoot_are_rejected() {
        let mut root = clip("LINEAR", "translation");
        let duplicate = root["animations"][0]["channels"][0].clone();
        root["animations"][0]["channels"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(!check(root, &[0., 1.], &[0.; 6], 3));
        let mut values = [0.; 18];
        values[6] = 10_000_000.;
        assert!(!check(
            clip("CUBICSPLINE", "translation"),
            &[0., 10.],
            &values,
            3
        ));
    }
}
