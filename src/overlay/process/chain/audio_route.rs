use crate::config::ProcessingBlock;

pub(super) fn preprocessed_text(
    block_idx: usize,
    result: &Option<(usize, String)>,
) -> Option<&str> {
    result
        .as_ref()
        .and_then(|(index, text)| (*index == block_idx).then_some(text.as_str()))
}

pub(crate) fn streaming_audio_result(
    blocks: &[ProcessingBlock],
    connections: &[(usize, usize)],
    graph_explicit: bool,
    streaming: bool,
    text: String,
) -> Option<(usize, String)> {
    if !streaming {
        return None;
    }
    initial_audio_blocks(blocks, connections, graph_explicit)
        .first()
        .map(|&index| (index, text))
}

pub(super) fn execute_audio_branch(
    block: &ProcessingBlock,
    context: &crate::overlay::result::RefineContext,
    execute: impl FnOnce(&ProcessingBlock, &[u8]) -> anyhow::Result<String>,
) -> anyhow::Result<String> {
    match context {
        crate::overlay::result::RefineContext::Audio(wav) if !wav.is_empty() => execute(block, wav),
        _ => Err(anyhow::anyhow!("Missing audio context")),
    }
}

/// Audio that can consume the original recording before any processing step.
pub(crate) fn initial_audio_blocks(
    blocks: &[ProcessingBlock],
    connections: &[(usize, usize)],
    graph_explicit: bool,
) -> Vec<usize> {
    let mut found = Vec::new();
    let mut visited = vec![false; blocks.len()];
    let mut pending = vec![0];
    while let Some(index) = pending.pop() {
        let Some(block) = blocks.get(index) else {
            continue;
        };
        if std::mem::replace(&mut visited[index], true) {
            continue;
        }
        if block.block_type == "audio" {
            found.push(index);
        } else if block.block_type == "input_adapter" {
            if connections.is_empty() && !graph_explicit {
                pending.push(index + 1);
            } else {
                pending.extend(
                    connections
                        .iter()
                        .rev()
                        .filter_map(|&(from, to)| (from == index).then_some(to)),
                );
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: &str) -> ProcessingBlock {
        ProcessingBlock {
            block_type: kind.into(),
            ..Default::default()
        }
    }

    #[test]
    fn routes_only_reachable_audio_before_processing_in_branch_order() {
        let blocks = [
            block("input_adapter"),
            block("audio"),
            block("input_adapter"),
            block("audio"),
            block("audio"),
            block("text"),
        ];
        assert_eq!(
            initial_audio_blocks(&blocks, &[(0, 2), (2, 3), (0, 4), (4, 1), (0, 5)], true),
            [3, 4]
        );
    }

    #[test]
    fn linear_adapter_and_direct_audio_and_empty_input() {
        assert_eq!(
            initial_audio_blocks(
                &[block("input_adapter"), block("audio"), block("audio")],
                &[],
                false
            ),
            [1]
        );
        assert_eq!(initial_audio_blocks(&[block("audio")], &[], false), [0]);
        assert!(initial_audio_blocks(&[block("input_adapter")], &[], false).is_empty());
        assert!(initial_audio_blocks(&[], &[], false).is_empty());
        assert!(
            initial_audio_blocks(&[block("input_adapter"), block("audio")], &[], true).is_empty()
        );
    }

    #[test]
    fn branching_shares_recording_backing() {
        use crate::overlay::result::RefineContext;
        let original = RefineContext::Audio(vec![1; 1024].into());
        let left = original.clone();
        let right = original.clone();
        let (
            RefineContext::Audio(original),
            RefineContext::Audio(left),
            RefineContext::Audio(right),
        ) = (&original, &left, &right)
        else {
            panic!("expected audio")
        };
        assert!(std::sync::Arc::ptr_eq(original, left));
        assert!(std::sync::Arc::ptr_eq(original, right));
    }

    #[test]
    fn cycles_and_invalid_destinations_do_not_loop() {
        let blocks = [block("input_adapter"), block("audio")];
        assert_eq!(
            initial_audio_blocks(&blocks, &[(0, 0), (0, 99), (0, 1)], true),
            [1]
        );
    }

    #[test]
    fn branched_audio_reuses_only_selected_result_and_executes_other_with_original_audio() {
        use crate::overlay::result::RefineContext;
        let mut left = block("audio");
        left.model = "left".into();
        left.prompt = "left prompt".into();
        let mut right = block("audio");
        right.model = "right".into();
        right.prompt = "right prompt".into();
        let blocks = [block("input_adapter"), left, right];
        let selected = initial_audio_blocks(&blocks, &[(0, 1), (0, 2)], true)[0];
        let result = Some((selected, "left output".to_string()));
        let audio = RefineContext::Audio(vec![1, 2, 3].into());
        let mut calls = Vec::new();
        let outputs: Vec<_> = [1, 2]
            .into_iter()
            .map(|index| {
                preprocessed_text(index, &result)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        execute_audio_branch(&blocks[index], &audio, |block, wav| {
                            calls.push((block.model.clone(), block.prompt.clone(), wav.to_vec()));
                            Ok("right output".into())
                        })
                        .unwrap()
                    })
            })
            .collect();
        assert_eq!(outputs, ["left output", "right output"]);
        assert_eq!(
            calls,
            [("right".into(), "right prompt".into(), vec![1, 2, 3])]
        );
        assert!(
            execute_audio_branch(&blocks[2], &RefineContext::None, |_, _| unreachable!()).is_err()
        );
    }

    #[test]
    fn finite_branches_execute_independently_when_first_provider_fails() {
        use crate::overlay::result::RefineContext;
        let mut first = block("audio");
        first.model = "first".into();
        let mut second = block("audio");
        second.model = "second".into();
        let blocks = [block("input_adapter"), first, second];
        let edges = [(0, 1), (0, 2)];
        let prepared = streaming_audio_result(&blocks, &edges, true, false, "unexpected".into());
        let wav = RefineContext::Audio(vec![1, 2, 3].into());
        let mut calls = Vec::new();
        let results: Vec<_> = initial_audio_blocks(&blocks, &edges, true)
            .into_iter()
            .map(|index| {
                assert!(preprocessed_text(index, &prepared).is_none());
                execute_audio_branch(&blocks[index], &wav, |block, bytes| {
                    calls.push((block.model.clone(), bytes.to_vec()));
                    if block.model == "first" {
                        Err(anyhow::anyhow!("provider failed"))
                    } else {
                        Ok("second result".into())
                    }
                })
            })
            .collect();
        assert!(results[0].is_err());
        assert_eq!(results[1].as_ref().unwrap(), "second result");
        assert_eq!(
            calls,
            [
                ("first".into(), vec![1, 2, 3]),
                ("second".into(), vec![1, 2, 3])
            ]
        );
        assert_eq!(
            streaming_audio_result(&blocks, &edges, true, true, "live".into()),
            Some((1, "live".into()))
        );
    }
}
