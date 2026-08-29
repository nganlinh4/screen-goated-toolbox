use std::borrow::Cow;
use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{
    AccessorInfo, BufferViewInfo, MAX_GLTF_ABSOLUTE_RENDERER_VALUE, MAX_GLTF_JOINTS_PER_SKIN,
    MAX_GLTF_SKINS, MAX_GLTF_TOTAL_JOINTS,
};

pub(super) fn validate(
    root: &Map<String, Value>,
    accessors: &[AccessorInfo],
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let meshes = root
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let Some(skins) = root.get("skins") else {
        return reject_orphan_skin_work(nodes, meshes);
    };
    let skins = skins
        .as_array()
        .filter(|items| items.len() <= MAX_GLTF_SKINS)
        .ok_or_else(invalid)?;
    if skins.is_empty() {
        return reject_orphan_skin_work(nodes, meshes);
    }

    let mut joint_counts = Vec::with_capacity(skins.len());
    let mut total_joints = 0_usize;
    for skin in skins {
        let skin = skin.as_object().ok_or_else(invalid)?;
        let joints = skin
            .get("joints")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty() && items.len() <= MAX_GLTF_JOINTS_PER_SKIN)
            .ok_or_else(invalid)?;
        total_joints = total_joints
            .checked_add(joints.len())
            .filter(|total| *total <= MAX_GLTF_TOTAL_JOINTS)
            .ok_or_else(invalid)?;
        let mut unique = HashSet::with_capacity(joints.len());
        for joint in joints {
            let index = table_index(joint, nodes.len())?;
            if !unique.insert(index) {
                return Err(invalid());
            }
        }
        if let Some(skeleton) = skin.get("skeleton") {
            table_index(skeleton, nodes.len())?;
        }
        if let Some(value) = skin.get("inverseBindMatrices") {
            let index = table_index(value, accessors.len())?;
            let accessor = accessors[index];
            if accessor.count != joints.len() as u64
                || accessor.component_type != 5126
                || accessor.component_count != 16
                || accessor.normalized
            {
                return Err(invalid());
            }
            scan_finite_floats(accessor, views, buffers)?;
        }
        joint_counts.push(joints.len());
    }

    let mut referenced = vec![false; skins.len()];
    let mut mesh_skin = vec![None; meshes.len()];
    for (node_index, node) in nodes.iter().enumerate() {
        let node = node.as_object().ok_or_else(invalid)?;
        let Some(value) = node.get("skin") else {
            continue;
        };
        let skin = table_index(value, skins.len())?;
        referenced[skin] = true;
        assign_skin_scope(node_index, skin, nodes, &mut mesh_skin)?;
    }
    if referenced.iter().any(|value| !value) {
        return Err(invalid());
    }

    let mut scanned_joints = HashSet::new();
    let mut scanned_weights = HashSet::new();
    for (mesh_index, mesh) in meshes.iter().enumerate() {
        let primitives = mesh
            .as_object()
            .and_then(|mesh| mesh.get("primitives"))
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        for primitive in primitives {
            let attributes = primitive
                .as_object()
                .and_then(|primitive| primitive.get("attributes"))
                .and_then(Value::as_object)
                .ok_or_else(invalid)?;
            let joints = attributes.get("JOINTS_0");
            let weights = attributes.get("WEIGHTS_0");
            if joints.is_some() != weights.is_some() {
                return Err(invalid());
            }
            let Some(joints) = joints else {
                if mesh_skin[mesh_index].is_some() {
                    return Err(invalid());
                }
                continue;
            };
            let skin = mesh_skin[mesh_index].ok_or_else(invalid)?;
            let joint_index = table_index(joints, accessors.len())?;
            let weight_index = table_index(weights.ok_or_else(invalid)?, accessors.len())?;
            let joint_accessor = accessors[joint_index];
            let weight_accessor = accessors[weight_index];
            if joint_accessor.count != weight_accessor.count
                || joint_accessor.component_count != 4
                || !matches!(joint_accessor.component_type, 5121 | 5123)
                || joint_accessor.normalized
                || weight_accessor.component_count != 4
                || !(weight_accessor.component_type == 5126
                    || weight_accessor.normalized
                        && matches!(weight_accessor.component_type, 5121 | 5123))
            {
                return Err(invalid());
            }
            if scanned_joints.insert((joint_index, joint_counts[skin])) {
                scan_joints(joint_accessor, joint_counts[skin], views, buffers)?;
            }
            if scanned_weights.insert(weight_index) {
                scan_weights(weight_accessor, views, buffers)?;
            }
        }
    }
    Ok(())
}

fn reject_orphan_skin_work(nodes: &[Value], meshes: &[Value]) -> Result<(), String> {
    if nodes.iter().any(|node| {
        node.as_object()
            .is_none_or(|node| node.contains_key("skin"))
    }) || meshes.iter().any(mesh_has_skin_attributes)
    {
        return Err(invalid());
    }
    Ok(())
}

fn mesh_has_skin_attributes(mesh: &Value) -> bool {
    mesh.as_object()
        .and_then(|mesh| mesh.get("primitives"))
        .and_then(Value::as_array)
        .is_none_or(|primitives| {
            primitives.iter().any(|primitive| {
                primitive
                    .as_object()
                    .and_then(|primitive| primitive.get("attributes"))
                    .and_then(Value::as_object)
                    .is_none_or(|attributes| {
                        attributes.contains_key("JOINTS_0") || attributes.contains_key("WEIGHTS_0")
                    })
            })
        })
}

fn assign_skin_scope(
    root: usize,
    skin: usize,
    nodes: &[Value],
    mesh_skin: &mut [Option<usize>],
) -> Result<(), String> {
    let mut stack = vec![root];
    let mut found_geometry = false;
    while let Some(index) = stack.pop() {
        let node = nodes[index].as_object().ok_or_else(invalid)?;
        if index != root && node.contains_key("skin") {
            return Err(invalid());
        }
        if let Some(mesh) = node.get("mesh") {
            let mesh = table_index(mesh, mesh_skin.len())?;
            match mesh_skin[mesh] {
                Some(existing) if existing != skin => return Err(invalid()),
                _ => mesh_skin[mesh] = Some(skin),
            }
            found_geometry = true;
        }
        if let Some(children) = node.get("children") {
            for child in children.as_array().ok_or_else(invalid)? {
                stack.push(table_index(child, nodes.len())?);
            }
        }
    }
    if !found_geometry {
        return Err(invalid());
    }
    Ok(())
}

fn scan_joints(
    accessor: AccessorInfo,
    joint_count: usize,
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    let component_bytes = if accessor.component_type == 5121 {
        1
    } else {
        2
    };
    for element in 0..accessor.count {
        for component in 0..4 {
            let bytes = component_bytes_at(
                accessor,
                element,
                component,
                component_bytes,
                views,
                buffers,
            )?;
            let value = if component_bytes == 1 {
                usize::from(bytes[0])
            } else {
                usize::from(u16::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))
            };
            if value >= joint_count {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

fn scan_weights(
    accessor: AccessorInfo,
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    let component_bytes = if accessor.component_type == 5121 {
        1
    } else if accessor.component_type == 5123 {
        2
    } else {
        4
    };
    for element in 0..accessor.count {
        let mut sum = 0.0_f64;
        for component in 0..4 {
            let bytes = component_bytes_at(
                accessor,
                element,
                component,
                component_bytes,
                views,
                buffers,
            )?;
            let value = match accessor.component_type {
                5121 => f64::from(bytes[0]) / f64::from(u8::MAX),
                5123 => {
                    f64::from(u16::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))
                        / f64::from(u16::MAX)
                }
                5126 => f32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?) as f64,
                _ => return Err(invalid()),
            };
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(invalid());
            }
            sum += value;
        }
        if (sum - 1.0).abs() > 0.01 {
            return Err(invalid());
        }
    }
    Ok(())
}

fn scan_finite_floats(
    accessor: AccessorInfo,
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    for element in 0..accessor.count {
        for component in 0..accessor.component_count {
            let bytes = component_bytes_at(accessor, element, component, 4, views, buffers)?;
            let value = f32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?) as f64;
            if !value.is_finite() || value.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

fn component_bytes_at<'a>(
    accessor: AccessorInfo,
    element: u64,
    component: u64,
    component_bytes: u64,
    views: &[BufferViewInfo],
    buffers: &'a [Cow<'_, [u8]>],
) -> Result<&'a [u8], String> {
    let view = views.get(accessor.buffer_view).ok_or_else(invalid)?;
    let stride = accessor
        .byte_stride
        .unwrap_or(accessor.component_count * component_bytes);
    let start = accessor
        .absolute_offset
        .checked_add(element.checked_mul(stride).ok_or_else(invalid)?)
        .and_then(|value| value.checked_add(component * component_bytes))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(invalid)?;
    let end = start
        .checked_add(component_bytes as usize)
        .ok_or_else(invalid)?;
    buffers
        .get(view.buffer)
        .and_then(|buffer| buffer.get(start..end))
        .ok_or_else(invalid)
}

fn table_index(value: &Value, length: usize) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < length)
        .ok_or_else(invalid)
}

fn invalid() -> String {
    "The model result description is invalid.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Value, Vec<AccessorInfo>, Vec<BufferViewInfo>, Vec<u8>) {
        let root = serde_json::json!({
            "skins": [{"joints": [0], "inverseBindMatrices": 2}],
            "nodes": [
                {},
                {"skin": 0, "children": [2]},
                {"mesh": 0}
            ],
            "meshes": [{"primitives": [{"attributes": {
                "POSITION": 3,
                "JOINTS_0": 0,
                "WEIGHTS_0": 1
            }}]}]
        });
        let mut bytes = vec![0_u8; 124];
        for vertex in 0..3 {
            bytes[12 + vertex * 16..16 + vertex * 16].copy_from_slice(&1.0_f32.to_le_bytes());
        }
        for diagonal in 0..4 {
            let offset = 60 + diagonal * 20;
            bytes[offset..offset + 4].copy_from_slice(&1.0_f32.to_le_bytes());
        }
        let views = vec![
            BufferViewInfo {
                buffer: 0,
                byte_offset: 0,
                length: 12,
                byte_stride: None,
            },
            BufferViewInfo {
                buffer: 0,
                byte_offset: 12,
                length: 48,
                byte_stride: None,
            },
            BufferViewInfo {
                buffer: 0,
                byte_offset: 60,
                length: 64,
                byte_stride: None,
            },
        ];
        let accessors = vec![
            AccessorInfo {
                count: 3,
                component_type: 5121,
                component_count: 4,
                buffer_view: 0,
                byte_offset: 0,
                absolute_offset: 0,
                byte_stride: None,
                normalized: false,
            },
            AccessorInfo {
                count: 3,
                component_type: 5126,
                component_count: 4,
                buffer_view: 1,
                byte_offset: 0,
                absolute_offset: 12,
                byte_stride: None,
                normalized: false,
            },
            AccessorInfo {
                count: 1,
                component_type: 5126,
                component_count: 16,
                buffer_view: 2,
                byte_offset: 0,
                absolute_offset: 60,
                byte_stride: None,
                normalized: false,
            },
            AccessorInfo::default(),
        ];
        (root, accessors, views, bytes)
    }

    #[test]
    fn bounded_descendant_skin_is_accepted() {
        let (root, accessors, views, bytes) = fixture();
        assert!(
            validate(
                root.as_object().unwrap(),
                &accessors,
                &views,
                &[Cow::Borrowed(&bytes)]
            )
            .is_ok()
        );
    }

    #[test]
    fn out_of_range_joints_and_unpaired_weights_are_rejected() {
        let (mut root, accessors, views, mut bytes) = fixture();
        bytes[0] = 1;
        assert!(
            validate(
                root.as_object().unwrap(),
                &accessors,
                &views,
                &[Cow::Borrowed(&bytes)]
            )
            .is_err()
        );

        root["meshes"][0]["primitives"][0]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove("WEIGHTS_0");
        bytes[0] = 0;
        assert!(
            validate(
                root.as_object().unwrap(),
                &accessors,
                &views,
                &[Cow::Borrowed(&bytes)]
            )
            .is_err()
        );
    }
}
