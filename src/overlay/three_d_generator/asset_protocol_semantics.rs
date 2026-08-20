use std::borrow::Cow;
use std::collections::VecDeque;

use serde_json::{Map, Value};

use super::{
    AccessorInfo, BufferViewInfo, MAX_GLTF_ABSOLUTE_RENDERER_VALUE, MAX_GLTF_INDICES,
    MAX_GLTF_MATERIALS, MAX_GLTF_MESHES, MAX_GLTF_MORPH_ELEMENTS, MAX_GLTF_MORPH_TARGETS,
    MAX_GLTF_NODE_DEPTH, MAX_GLTF_NODES, MAX_GLTF_PRIMITIVES, MAX_GLTF_SCENES, MAX_GLTF_VERTICES,
    MAX_MORPH_ATTRIBUTES, MAX_PRIMITIVE_ATTRIBUTES,
};

pub(super) fn validate(
    root: &Map<String, Value>,
    accessors: &[AccessorInfo],
) -> Result<(), String> {
    let mesh_costs = validate_meshes(root, accessors)?;
    validate_node_graph(root, &mesh_costs)
}

pub(super) fn validate_indices(
    root: &Map<String, Value>,
    accessors: &[AccessorInfo],
    views: &[BufferViewInfo],
    buffers: &[Cow<'_, [u8]>],
) -> Result<(), String> {
    let invalid = || "The model result description is invalid.".to_string();
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
            let position_count = attributes
                .get("POSITION")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| accessors.get(index))
                .map(|accessor| accessor.count)
                .ok_or_else(invalid)?;
            let Some(index) = primitive
                .get("indices")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
            else {
                continue;
            };
            let accessor = accessors.get(index).ok_or_else(invalid)?;
            let view = views.get(accessor.buffer_view).ok_or_else(invalid)?;
            let buffer = buffers.get(view.buffer).ok_or_else(invalid)?;
            let component_bytes = match accessor.component_type {
                5121 => 1,
                5123 => 2,
                5125 => 4,
                _ => return Err(invalid()),
            };
            let stride = accessor.byte_stride.unwrap_or(component_bytes);
            for element in 0..accessor.count {
                let start = view
                    .byte_offset
                    .checked_add(accessor.byte_offset)
                    .and_then(|value| {
                        element
                            .checked_mul(stride)
                            .and_then(|offset| value.checked_add(offset))
                    })
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(invalid)?;
                let end = start
                    .checked_add(component_bytes as usize)
                    .ok_or_else(invalid)?;
                let bytes = buffer.get(start..end).ok_or_else(invalid)?;
                let value = match component_bytes {
                    1 => u64::from(bytes[0]),
                    2 => u64::from(u16::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                    4 => u64::from(u32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                    _ => unreachable!(),
                };
                if value >= position_count {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct MeshCost {
    vertices: u64,
    indices: u64,
    morph_elements: u64,
    morph_targets: usize,
}

fn validate_meshes(
    root: &Map<String, Value>,
    accessors: &[AccessorInfo],
) -> Result<Vec<MeshCost>, String> {
    let invalid = || "The model result description is invalid.".to_string();
    let material_count = match root.get("materials") {
        Some(value) => {
            let materials = value
                .as_array()
                .filter(|items| items.len() <= MAX_GLTF_MATERIALS)
                .ok_or_else(invalid)?;
            if materials.iter().any(|material| !material.is_object()) {
                return Err(invalid());
            }
            materials.len()
        }
        None => 0,
    };
    let meshes = root
        .get("meshes")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_GLTF_MESHES)
        .ok_or_else(|| "The model result contains no geometry.".to_string())?;
    let mut primitive_total = 0_usize;
    let mut vertex_total = 0_u64;
    let mut index_total = 0_u64;
    let mut morph_target_total = 0_usize;
    let mut morph_element_total = 0_u64;
    let mut mesh_costs = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let mut mesh_cost = MeshCost::default();
        let mesh = mesh.as_object().ok_or_else(invalid)?;
        let primitives = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .ok_or_else(invalid)?;
        let mut mesh_target_count = None;
        primitive_total = primitive_total
            .checked_add(primitives.len())
            .filter(|total| *total <= MAX_GLTF_PRIMITIVES)
            .ok_or_else(invalid)?;
        for primitive in primitives {
            let primitive = primitive.as_object().ok_or_else(invalid)?;
            // Triangles carry the surface. Lines carry the original face loops
            // of a quad mesh, which is the only way a viewer can show a quad as
            // one face: glTF has no quad primitive. No other mode is produced.
            let mode = match primitive.get("mode") {
                Some(value) => value.as_u64().ok_or_else(invalid)?,
                None => 4,
            };
            let vertices_per_element: u64 = match mode {
                4 => 3,
                1 => 2,
                _ => return Err(invalid()),
            };
            let attributes = primitive
                .get("attributes")
                .and_then(Value::as_object)
                .filter(|items| !items.is_empty() && items.len() <= MAX_PRIMITIVE_ATTRIBUTES)
                .ok_or_else(invalid)?;
            let position = accessor_at(attributes.get("POSITION"), accessors)?;
            if position.component_type != 5126 || position.component_count != 3 {
                return Err(invalid());
            }
            for (semantic, value) in attributes {
                let attribute = accessor_at(Some(value), accessors)?;
                if attribute.count != position.count || !valid_vertex_attribute(semantic, attribute)
                {
                    return Err(invalid());
                }
            }
            vertex_total = vertex_total
                .checked_add(position.count)
                .filter(|total| *total <= MAX_GLTF_VERTICES)
                .ok_or_else(invalid)?;
            mesh_cost.vertices = mesh_cost
                .vertices
                .checked_add(position.count)
                .ok_or_else(invalid)?;
            if let Some(value) = primitive.get("indices") {
                let indices = accessor_at(Some(value), accessors)?;
                if indices.component_count != 1
                    || !matches!(indices.component_type, 5121 | 5123 | 5125)
                    || indices.byte_stride.is_some()
                    || indices.count < vertices_per_element
                    || indices.count % vertices_per_element != 0
                {
                    return Err(invalid());
                }
                index_total = index_total
                    .checked_add(indices.count)
                    .filter(|total| *total <= MAX_GLTF_INDICES)
                    .ok_or_else(invalid)?;
                mesh_cost.indices = mesh_cost
                    .indices
                    .checked_add(indices.count)
                    .ok_or_else(invalid)?;
            } else if position.count < vertices_per_element
                || position.count % vertices_per_element != 0
            {
                return Err(invalid());
            }
            if let Some(value) = primitive.get("material")
                && value
                    .as_u64()
                    .and_then(|index| usize::try_from(index).ok())
                    .is_none_or(|index| index >= material_count)
            {
                return Err(invalid());
            }
            let targets = match primitive.get("targets") {
                Some(value) => value
                    .as_array()
                    .filter(|targets| !targets.is_empty())
                    .ok_or_else(invalid)?
                    .as_slice(),
                None => &[],
            };
            if mesh_target_count
                .replace(targets.len())
                .is_some_and(|count| count != targets.len())
            {
                return Err(invalid());
            }
            if !targets.is_empty() {
                morph_target_total = morph_target_total
                    .checked_add(targets.len())
                    .filter(|total| *total <= MAX_GLTF_MORPH_TARGETS)
                    .ok_or_else(invalid)?;
                for target in targets {
                    let target = target
                        .as_object()
                        .filter(|items| !items.is_empty() && items.len() <= MAX_MORPH_ATTRIBUTES)
                        .ok_or_else(invalid)?;
                    for (semantic, value) in target {
                        let accessor = accessor_at(Some(value), accessors)?;
                        if !matches!(semantic.as_str(), "POSITION" | "NORMAL" | "TANGENT")
                            || accessor.component_type != 5126
                            || accessor.component_count != 3
                            || accessor.count != position.count
                            || !accessor.absolute_offset.is_multiple_of(4)
                        {
                            return Err(invalid());
                        }
                        morph_element_total = morph_element_total
                            .checked_add(accessor.count)
                            .filter(|total| *total <= MAX_GLTF_MORPH_ELEMENTS)
                            .ok_or_else(invalid)?;
                        mesh_cost.morph_elements = mesh_cost
                            .morph_elements
                            .checked_add(accessor.count)
                            .ok_or_else(invalid)?;
                    }
                }
            }
        }
        mesh_cost.morph_targets = mesh_target_count.unwrap_or(0);
        validate_weights(mesh.get("weights"), mesh_cost.morph_targets)?;
        mesh_costs.push(mesh_cost);
    }
    Ok(mesh_costs)
}

fn valid_vertex_attribute(semantic: &str, accessor: AccessorInfo) -> bool {
    if !accessor.absolute_offset.is_multiple_of(4) {
        return false;
    }
    match semantic {
        "POSITION" | "NORMAL" => accessor.component_type == 5126 && accessor.component_count == 3,
        "TANGENT" => accessor.component_type == 5126 && accessor.component_count == 4,
        "COLOR_0" => {
            matches!(accessor.component_count, 3 | 4) && valid_float_or_normalized_integer(accessor)
        }
        "TEXCOORD_0" | "TEXCOORD_1" | "TEXCOORD_2" | "TEXCOORD_3" => {
            accessor.component_count == 2 && valid_float_or_normalized_integer(accessor)
        }
        value if value.starts_with('_') => accessor.component_count <= 4,
        _ => false,
    }
}

fn valid_float_or_normalized_integer(accessor: AccessorInfo) -> bool {
    accessor.component_type == 5126
        || accessor.normalized && matches!(accessor.component_type, 5120..=5123)
}

fn accessor_at(value: Option<&Value>, accessors: &[AccessorInfo]) -> Result<AccessorInfo, String> {
    value
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| accessors.get(index))
        .copied()
        .ok_or_else(|| "The model result description is invalid.".to_string())
}

fn validate_node_graph(root: &Map<String, Value>, mesh_costs: &[MeshCost]) -> Result<(), String> {
    let invalid = || "The model result description is invalid.".to_string();
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_GLTF_NODES)
        .ok_or_else(invalid)?;
    let mut children = Vec::with_capacity(nodes.len());
    let mut incoming = vec![0_u8; nodes.len()];
    let mut has_mesh = Vec::with_capacity(nodes.len());
    let mut instanced_cost = MeshCost::default();
    for node in nodes {
        let node = node.as_object().ok_or_else(invalid)?;
        validate_node_transform(node)?;
        if let Some(mesh) = node.get("mesh") {
            let cost = mesh
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| mesh_costs.get(index))
                .ok_or_else(invalid)?;
            instanced_cost.vertices =
                bounded_add(instanced_cost.vertices, cost.vertices, MAX_GLTF_VERTICES)?;
            instanced_cost.indices =
                bounded_add(instanced_cost.indices, cost.indices, MAX_GLTF_INDICES)?;
            instanced_cost.morph_elements = bounded_add(
                instanced_cost.morph_elements,
                cost.morph_elements,
                MAX_GLTF_MORPH_ELEMENTS,
            )?;
            validate_weights(node.get("weights"), cost.morph_targets)?;
        } else if node.contains_key("weights") {
            return Err(invalid());
        }
        has_mesh.push(node.contains_key("mesh"));
        let mut node_children = Vec::new();
        if let Some(values) = node.get("children") {
            for value in values.as_array().ok_or_else(invalid)? {
                let child = value
                    .as_u64()
                    .and_then(|index| usize::try_from(index).ok())
                    .filter(|index| *index < nodes.len())
                    .ok_or_else(invalid)?;
                incoming[child] = incoming[child].checked_add(1).ok_or_else(invalid)?;
                if incoming[child] > 1 {
                    return Err(invalid());
                }
                node_children.push(child);
            }
        }
        children.push(node_children);
    }
    let parented: Vec<bool> = incoming.iter().map(|count| *count > 0).collect();
    let mut ready: VecDeque<usize> = incoming
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    let mut depth = vec![1_usize; nodes.len()];
    let mut visited = 0_usize;
    while let Some(node) = ready.pop_front() {
        if depth[node] > MAX_GLTF_NODE_DEPTH {
            return Err(invalid());
        }
        visited += 1;
        for child in &children[node] {
            depth[*child] = depth[node].checked_add(1).ok_or_else(invalid)?;
            incoming[*child] -= 1;
            if incoming[*child] == 0 {
                ready.push_back(*child);
            }
        }
    }
    if visited != nodes.len() {
        return Err(invalid());
    }
    validate_scenes(root, &children, &parented, &has_mesh)
}

fn validate_weights(value: Option<&Value>, expected: usize) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let weights = value
        .as_array()
        .filter(|weights| expected > 0 && weights.len() == expected)
        .ok_or_else(|| "The model result description is invalid.".to_string())?;
    if weights.iter().any(|weight| {
        weight.as_f64().is_none_or(|value| {
            !value.is_finite() || value.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE
        })
    }) {
        return Err("The model result description is invalid.".to_string());
    }
    Ok(())
}

fn validate_node_transform(node: &Map<String, Value>) -> Result<(), String> {
    let invalid = || "The model result description is invalid.".to_string();
    if node.contains_key("matrix") {
        if ["translation", "rotation", "scale"]
            .iter()
            .any(|key| node.contains_key(*key))
        {
            return Err(invalid());
        }
        validate_number_array(node.get("matrix"), 16)?;
    }
    for (key, length) in [("translation", 3), ("rotation", 4), ("scale", 3)] {
        if node.contains_key(key) {
            validate_number_array(node.get(key), length)?;
        }
    }
    Ok(())
}

fn validate_number_array(value: Option<&Value>, length: usize) -> Result<(), String> {
    value
        .and_then(Value::as_array)
        .filter(|values| {
            values.len() == length
                && values.iter().all(|value| {
                    value.as_f64().is_some_and(|value| {
                        value.is_finite() && value.abs() <= MAX_GLTF_ABSOLUTE_RENDERER_VALUE
                    })
                })
        })
        .map(|_| ())
        .ok_or_else(|| "The model result description is invalid.".to_string())
}

fn bounded_add(current: u64, amount: u64, maximum: u64) -> Result<u64, String> {
    current
        .checked_add(amount)
        .filter(|total| *total <= maximum)
        .ok_or_else(|| "The model result description is invalid.".to_string())
}

fn validate_scenes(
    root: &Map<String, Value>,
    children: &[Vec<usize>],
    parented: &[bool],
    has_mesh: &[bool],
) -> Result<(), String> {
    let invalid = || "The model result description is invalid.".to_string();
    let scenes = root
        .get("scenes")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_GLTF_SCENES)
        .ok_or_else(invalid)?;
    let selected_scene = match root.get("scene") {
        Some(scene) => scene
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < scenes.len())
            .ok_or_else(invalid)?,
        None => 0,
    };
    let mut root_total = 0_usize;
    let mut scene_roots = vec![false; children.len()];
    let mut selected_has_geometry = false;
    for (scene_index, scene) in scenes.iter().enumerate() {
        let scene = scene.as_object().ok_or_else(invalid)?;
        let roots: &[Value] = match scene.get("nodes") {
            Some(nodes) => nodes.as_array().ok_or_else(invalid)?.as_slice(),
            None => &[],
        };
        if scene_index == selected_scene && roots.is_empty() {
            return Err(invalid());
        }
        if !roots.is_empty() {
            root_total = root_total
                .checked_add(roots.len())
                .filter(|total| *total <= MAX_GLTF_NODES)
                .ok_or_else(invalid)?;
            for node in roots {
                let index = node
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .filter(|index| *index < children.len())
                    .ok_or_else(invalid)?;
                if parented[index] || std::mem::replace(&mut scene_roots[index], true) {
                    return Err(invalid());
                }
                if scene_index == selected_scene {
                    let mut pending = vec![index];
                    while let Some(node) = pending.pop() {
                        selected_has_geometry |= has_mesh[node];
                        pending.extend(children[node].iter().copied());
                    }
                }
            }
        }
    }
    selected_has_geometry.then_some(()).ok_or_else(invalid)
}
