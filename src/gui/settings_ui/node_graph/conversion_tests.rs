use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../parity-fixtures/preset-system/node-graph-editor.json"
    ))
    .unwrap()
}

fn block(id: &str) -> ProcessingBlock {
    ProcessingBlock {
        id: id.into(),
        block_type: if id == "input" {
            "input_adapter"
        } else {
            "text"
        }
        .into(),
        prompt: format!("authored prompt {id}"),
        ..Default::default()
    }
}

#[test]
fn shared_graph_cases_keep_drafts_and_preserve_invalid_saved_data() {
    for case in fixture()["cases"].as_array().unwrap() {
        let Some(ids) = case.get("blocks").and_then(|value| value.as_array()) else {
            continue;
        };
        let mut preset = crate::config::Preset {
            blocks: ids.iter().map(|id| block(id.as_str().unwrap())).collect(),
            block_connections: case["connections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|edge| {
                    (
                        edge[0].as_u64().unwrap() as usize,
                        edge[1].as_u64().unwrap() as usize,
                    )
                })
                .collect(),
            graph_explicit: case["graph_explicit"].as_bool().unwrap(),
            ..Default::default()
        };
        let original_blocks = preset.blocks.clone();
        let original_connections = preset.block_connections.clone();
        let original_explicit = preset.graph_explicit;
        if let Some(expected) = case
            .get("expected_initial_branch")
            .and_then(|value| value.as_str())
        {
            let actual = preset
                .first_reachable_processing_index()
                .map(|index| preset.blocks[index].id.as_str());
            assert_eq!(actual, Some(expected), "{}", case["name"]);
        }
        if let Some(expected) = case
            .get("expected_execution")
            .and_then(|value| value.as_array())
        {
            let expected_first = expected
                .iter()
                .filter_map(|id| id.as_str())
                .find(|id| *id != "input");
            let actual_first = preset
                .first_reachable_processing_index()
                .map(|index| preset.blocks[index].id.as_str());
            assert_eq!(actual_first, expected_first, "{}", case["name"]);
        }
        let mut snarl = blocks_to_snarl(
            &preset.blocks,
            &preset.block_connections,
            preset.graph_explicit,
            "text",
        );
        let editable = case
            .get("expected_graph_editable")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        assert_eq!(
            graph_connections_valid(&preset.blocks, &preset.block_connections),
            editable,
            "{}",
            case["name"]
        );
        if !editable {
            assert!(!save_graph_to_preset(&mut preset, &mut snarl));
            assert_eq!(preset.blocks, original_blocks);
            assert_eq!(preset.block_connections, original_connections);
            assert_eq!(preset.graph_explicit, original_explicit);
        } else if case.get("save_operation").and_then(|value| value.as_str())
            != Some("unrelated_setting")
        {
            assert!(save_graph_to_preset(&mut preset, &mut snarl));
            let saved_ids: Vec<_> = preset
                .blocks
                .iter()
                .map(|block| block.id.as_str())
                .collect();
            let expected: Vec<_> = case["expected_saved_blocks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect();
            assert_eq!(saved_ids, expected, "{}", case["name"]);
            assert_eq!(preset.blocks, original_blocks, "{}", case["name"]);
            if let Some(expected) = case
                .get("expected_saved_connections")
                .and_then(|value| value.as_array())
            {
                let expected: Vec<_> = expected
                    .iter()
                    .map(|edge| {
                        (
                            edge[0].as_u64().unwrap() as usize,
                            edge[1].as_u64().unwrap() as usize,
                        )
                    })
                    .collect();
                assert_eq!(preset.block_connections, expected, "{}", case["name"]);
            }
        } else {
            assert_eq!(snarl.wires().count(), 2);
            assert_eq!(preset.blocks, original_blocks);
            assert_eq!(preset.block_connections, original_connections);
        }
    }
}

#[test]
fn invalid_graph_forms_remain_read_only() {
    let blocks = [block("input"), block("one"), block("two")];
    for edges in [
        vec![(0, 0)],
        vec![(0, 1), (0, 1)],
        vec![(0, 2), (1, 2)],
        vec![(2, 0)],
    ] {
        assert!(!graph_connections_valid(&blocks, &edges));
        let _ = blocks_to_snarl(&blocks, &edges, true, "text");
    }
    assert!(!graph_connections_valid(
        &[block("one"), block("two")],
        &[(1, 0)]
    ));
}

#[test]
fn duplicate_ids_preserve_each_nodes_distinct_content() {
    let mut first = block("same");
    first.prompt = "first prompt".into();
    let mut second = block("same");
    second.prompt = "second prompt".into();
    let mut preset = crate::config::Preset {
        blocks: vec![block("input"), first.clone(), second.clone()],
        block_connections: vec![(0, 1)],
        graph_explicit: true,
        ..Default::default()
    };
    assert!(graph_connections_valid(
        &preset.blocks,
        &preset.block_connections
    ));
    let mut snarl = blocks_to_snarl(&preset.blocks, &preset.block_connections, true, "text");
    assert!(save_graph_to_preset(&mut preset, &mut snarl));
    assert_eq!(preset.blocks[1], first);
    assert_eq!(preset.blocks[2], second);
}

#[test]
fn missing_explicit_marker_remains_legacy_and_explicit_round_trips() {
    let original = crate::config::Preset {
        blocks: vec![block("input"), block("draft")],
        block_connections: vec![],
        graph_explicit: true,
        ..Default::default()
    };
    let mut encoded = serde_json::to_value(&original).unwrap();
    let decoded: crate::config::Preset = serde_json::from_value(encoded.clone()).unwrap();
    assert!(decoded.graph_explicit);
    encoded.as_object_mut().unwrap().remove("graph_explicit");
    let legacy: crate::config::Preset = serde_json::from_value(encoded).unwrap();
    assert!(!legacy.graph_explicit);
    assert_eq!(legacy.first_reachable_processing_index(), Some(1));
    assert_eq!(decoded.first_reachable_processing_index(), None);
}

#[test]
fn editing_one_node_preserves_unexposed_legacy_fields() {
    let mut input = block("input");
    input.prompt = "private input configuration".into();
    input.model = "saved input model".into();
    let mut draft = block("draft");
    draft.selected_language = "Korean".into();
    draft
        .language_vars
        .insert("language2".into(), "Vietnamese".into());
    let mut preset = crate::config::Preset {
        blocks: vec![input.clone(), draft.clone()],
        block_connections: vec![(0, 1)],
        graph_explicit: true,
        ..Default::default()
    };
    let mut snarl = blocks_to_snarl(&preset.blocks, &preset.block_connections, true, "text");
    let ids: Vec<_> = snarl.node_ids().map(|(id, _)| id).collect();
    match snarl.get_node_mut(ids[0]).unwrap() {
        ChainNode::Input { auto_copy, .. } => *auto_copy = true,
        _ => panic!("expected input"),
    }
    match snarl.get_node_mut(ids[1]).unwrap() {
        ChainNode::Process { prompt, .. } => *prompt = "edited".into(),
        _ => panic!("expected process"),
    }
    assert!(save_graph_to_preset(&mut preset, &mut snarl));
    input.auto_copy = true;
    draft.prompt = "edited".into();
    assert_eq!(preset.blocks, vec![input, draft]);
}

#[test]
fn graph_edits_preserve_audio_branch_order_across_repeated_saves() {
    let mut preset = crate::config::Preset {
        blocks: vec![
            block("input"),
            block("first"),
            block("second"),
            block("draft"),
        ],
        block_connections: vec![(0, 2), (0, 1)],
        graph_explicit: true,
        ..Default::default()
    };
    let mut snarl = blocks_to_snarl(&preset.blocks, &preset.block_connections, true, "text");
    for _ in 0..2 {
        assert!(save_graph_to_preset(&mut preset, &mut snarl));
        let targets: Vec<_> = preset
            .block_connections
            .iter()
            .map(|(_, to)| preset.blocks[*to].id.as_str())
            .collect();
        assert_eq!(targets, ["second", "first"]);
    }
}

#[test]
fn graph_save_keeps_authored_block_order_even_when_edges_point_backward() {
    let original = vec![
        block("input"),
        block("child"),
        block("parent"),
        block("draft"),
    ];
    let mut preset = crate::config::Preset {
        blocks: original.clone(),
        block_connections: vec![(0, 2), (2, 1)],
        graph_explicit: true,
        ..Default::default()
    };
    let mut snarl = blocks_to_snarl(&preset.blocks, &preset.block_connections, true, "text");
    assert!(save_graph_to_preset(&mut preset, &mut snarl));
    assert_eq!(preset.blocks, original);
    assert_eq!(preset.block_connections, [(0, 2), (2, 1)]);
}
