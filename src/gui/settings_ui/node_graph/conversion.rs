use super::node::ChainNode;
use crate::config::ProcessingBlock;
use eframe::egui;
use egui_snarl::{InPinId, NodeId, OutPinId, Snarl};

/// Invalid saved graphs remain intact and cannot be edited until restored.
pub fn graph_connections_valid(blocks: &[ProcessingBlock], connections: &[(usize, usize)]) -> bool {
    use std::collections::HashSet;

    let adapters: Vec<_> = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| (block.block_type == "input_adapter").then_some(index))
        .collect();
    if adapters.len() > 1 || adapters.first().is_some_and(|index| *index != 0) {
        return false;
    }
    let mut edges = HashSet::new();
    let mut inputs = HashSet::new();
    connections.iter().all(|&(from, to)| {
        from < blocks.len()
            && to < blocks.len()
            && from != to
            && blocks[to].block_type != "input_adapter"
            && (!adapters.is_empty() || to != 0)
            && edges.insert((from, to))
            && inputs.insert(to)
    })
}

/// Preserve original fields that are not represented by the graph controls.
pub fn save_graph_to_preset(
    preset: &mut crate::config::Preset,
    snarl: &mut Snarl<ChainNode>,
) -> bool {
    if !graph_connections_valid(&preset.blocks, &preset.block_connections) {
        return false;
    }
    let GraphProjection {
        blocks,
        mut connections,
        node_indices: indices,
    } = snarl_to_graph_with_indices(snarl);
    if !graph_connections_valid(&blocks, &connections) {
        return false;
    }
    let source_to_new: std::collections::HashMap<_, _> = snarl
        .node_ids()
        .filter_map(|(id, node)| Some((node.source_index()?, *indices.get(&id)?)))
        .collect();
    let mut remaining: std::collections::HashSet<_> = connections.drain(..).collect();
    for &(from, to) in &preset.block_connections {
        if let (Some(&from), Some(&to)) = (source_to_new.get(&from), source_to_new.get(&to))
            && remaining.remove(&(from, to))
        {
            connections.push((from, to));
        }
    }
    let mut new_edges: Vec<_> = remaining.into_iter().collect();
    new_edges.sort_unstable();
    connections.extend(new_edges);
    for (id, index) in indices {
        if let Some(node) = snarl.get_node_mut(id) {
            node.set_source(index, &blocks[index]);
        }
    }
    preset.blocks = blocks;
    preset.block_connections = connections;
    preset.graph_explicit = true;
    true
}

/// Convert blocks to snarl graph with intelligent layout
pub fn blocks_to_snarl(
    blocks: &[ProcessingBlock],
    connections: &[(usize, usize)],
    graph_explicit: bool,
    preset_type: &str,
) -> Snarl<ChainNode> {
    let mut snarl = Snarl::new();
    let mut node_ids = Vec::new();
    let mut occupied_inputs = std::collections::HashSet::new();
    if !blocks
        .iter()
        .any(|block| block.block_type == "input_adapter")
        && !blocks.is_empty()
    {
        occupied_inputs.insert(0);
    }
    let safe_connections: Vec<_> = connections
        .iter()
        .copied()
        .filter(|&(from, to)| {
            from < blocks.len()
                && to < blocks.len()
                && from != to
                && blocks[to].block_type != "input_adapter"
                && occupied_inputs.insert(to)
        })
        .collect();

    // Default layout parameters
    let start_x = 50.0;
    let start_y = 300.0; // Center vertically
    let spacing_x = 250.0; // Increased to widen the graph
    let spacing_y = 225.0; // Increased to prevent vertical overlap (nodes are tall)

    // Calculate positions based on graph structure
    let positions: Vec<egui::Pos2> = if !connections.is_empty() && !blocks.is_empty() {
        use std::collections::{HashMap, VecDeque};

        // 1. Build adjacency
        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(from, to) in &safe_connections {
            adj.entry(from).or_default().push(to);
        }

        // 2. Compute depth (layer) for each node via BFS
        let mut depths = vec![0; blocks.len()];
        let mut layer_nodes: HashMap<usize, Vec<usize>> = HashMap::new();

        let mut queue = VecDeque::new();
        queue.push_back((0, 0)); // Start BFS from node 0 (input)

        // Track visited to prevent cycles infinite loop (though unlikely in current DAG)
        let mut visited = vec![false; blocks.len()];
        visited[0] = true;

        while let Some((u, d)) = queue.pop_front() {
            depths[u] = d;
            layer_nodes.entry(d).or_default().push(u);

            if let Some(children) = adj.get(&u) {
                for &v in children {
                    if v < blocks.len() && !visited[v] {
                        visited[v] = true;
                        queue.push_back((v, d + 1));
                    }
                }
            }
        }

        // Handle disconnected nodes (put them at depth 0 or end? let's put at end)
        // Actually, let's just stick to default linear if not reachable, or append

        // 3. Assign positions
        let mut pos_map = vec![egui::pos2(0.0, 0.0); blocks.len()];

        for (depth, nodes) in layer_nodes.iter() {
            let count = nodes.len();
            let layer_height = (count as f32) * spacing_y;
            let layer_start_y = start_y - (layer_height / 2.0) + (spacing_y / 2.0);

            for (i, &node_idx) in nodes.iter().enumerate() {
                let x = start_x + (*depth as f32) * spacing_x;
                let y = layer_start_y + (i as f32) * spacing_y;
                pos_map[node_idx] = egui::pos2(x, y);
            }
        }

        // Fallback for unreachable nodes (if any) -> just place them linearly far away
        for i in 0..blocks.len() {
            if !visited[i] {
                pos_map[i] = egui::pos2(start_x + i as f32 * spacing_x, start_y + 300.0);
            }
        }

        pos_map
    } else {
        // Legacy linear layout
        blocks
            .iter()
            .enumerate()
            .map(|(i, _)| egui::pos2(start_x + i as f32 * spacing_x, start_y))
            .collect()
    };

    // 3. Create nodes
    // Check for input adapter
    let has_input_adapter = blocks.iter().any(|b| b.block_type == "input_adapter");

    // Legacy migration: If no input adapter, inject one virtually?
    // Actually, let's just insert nodes based on blocks.
    // If user opens a legacy preset, blocks[0] is NOT input_adapter.
    // So blocks[0] will be treated as Special.
    // And there will be NO Input Node.
    // This is bad because user can't connect anything to start.
    // So we MUST check if we need to insert a virtual Input Node.

    let mut virtual_input_id: Option<NodeId> = None;

    if !has_input_adapter {
        // Create virtual input node
        let input_block = ProcessingBlock {
            block_type: preset_type.to_string(), // Use preset_type for the virtual input node
            // "input_adapter" is generic, but using preset_type helps with UI logic
            show_overlay: false, // Input adapters should be hidden by default
            ..Default::default()
        };
        let node = ChainNode::from_block(&input_block, "input");
        let pos = egui::pos2(start_x, start_y);
        virtual_input_id = Some(snarl.insert_node(pos, node));
    }

    for (i, block) in blocks.iter().enumerate() {
        let role = if block.block_type == "input_adapter" {
            "input"
        } else {
            // Determine if this is a "first-level" node connected to input
            let is_connected_to_input = safe_connections.iter().any(|(from, to)| {
                *to == i
                    && blocks
                        .get(*from)
                        .is_some_and(|block| block.block_type == "input_adapter")
            });

            let is_legacy_first = i == 0 && !has_input_adapter;

            if is_connected_to_input || is_legacy_first {
                if preset_type == "text" {
                    "process"
                } else {
                    "special"
                }
            } else {
                "process"
            }
        };

        // Adjust position if we added virtual input
        let mut pos = positions[i];
        if virtual_input_id.is_some() {
            // Shift all nodes right
            pos.x += spacing_x;
        }

        let mut node = ChainNode::from_block(block, role);
        node.set_source(i, block);
        let node_id = snarl.insert_node(pos, node);
        node_ids.push(node_id);
    }

    // Connect virtual input if exists
    if let Some(v_id) = virtual_input_id {
        // Connect to legacy first block (index 0)
        if !node_ids.is_empty() {
            // We need to inject this connection into Snarl
            let from = OutPinId {
                node: v_id,
                output: 0,
            };
            let to = InPinId {
                node: node_ids[0],
                input: 0,
            };
            snarl.connect(from, to);
        }
    }

    // 4. Create connections
    if !connections.is_empty() {
        for &(from_idx, to_idx) in &safe_connections {
            if from_idx < node_ids.len() && to_idx < node_ids.len() {
                let from = OutPinId {
                    node: node_ids[from_idx],
                    output: 0,
                };
                let to = InPinId {
                    node: node_ids[to_idx],
                    input: 0,
                };
                snarl.connect(from, to);
            }
        }
    } else if !graph_explicit && blocks.len() > 1 {
        // Legacy fallback
        for i in 0..node_ids.len() - 1 {
            let from = OutPinId {
                node: node_ids[i],
                output: 0,
            };
            let to = InPinId {
                node: node_ids[i + 1],
                input: 0,
            };
            snarl.connect(from, to);
        }
    }

    snarl
}

struct GraphProjection {
    blocks: Vec<ProcessingBlock>,
    connections: Vec<(usize, usize)>,
    node_indices: std::collections::HashMap<NodeId, usize>,
}

/// Convert editor nodes and wires while retaining their output indices.
fn snarl_to_graph_with_indices(snarl: &Snarl<ChainNode>) -> GraphProjection {
    use std::collections::HashMap;

    let mut blocks = Vec::new();
    let mut connections = Vec::new();
    let mut node_to_idx: HashMap<NodeId, usize> = HashMap::new();

    // Keep the root first and retain authored order, including disconnected
    // drafts. Newly inserted nodes follow existing nodes in stable widget order.
    let mut nodes: Vec<_> = snarl.node_ids().collect();
    nodes.sort_unstable_by_key(|(id, node)| {
        (
            !node.is_input(),
            node.source_index().is_none(),
            node.source_index().unwrap_or(usize::MAX),
            id.0,
        )
    });
    for (node_id, node) in nodes {
        node_to_idx.insert(node_id, blocks.len());
        blocks.push(node.to_block());
    }

    for (from, to) in snarl.wires() {
        if let (Some(&from_idx), Some(&to_idx)) =
            (node_to_idx.get(&from.node), node_to_idx.get(&to.node))
        {
            connections.push((from_idx, to_idx));
        }
    }

    GraphProjection {
        blocks,
        connections,
        node_indices: node_to_idx,
    }
}

#[cfg(test)]
#[path = "conversion_tests.rs"]
mod tests;
