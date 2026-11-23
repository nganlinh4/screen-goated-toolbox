use crate::config::ProcessingBlock;
use crate::overlay::result::RefineContext;

pub(super) fn first_processing_in_path(blocks: &[ProcessingBlock], visited: &[usize]) -> bool {
    visited
        .iter()
        .take(visited.len().saturating_sub(1))
        .all(|&index| {
            blocks
                .get(index)
                .is_some_and(|block| block.block_type == "input_adapter")
        })
}

pub(super) fn retain_result_context(
    block_type: &str,
    block_idx: usize,
    first_processing: bool,
    context: &RefineContext,
) -> bool {
    block_type == "input_adapter"
        || block_idx == 0
        || (first_processing
            && matches!(
                (block_type, context),
                ("image", RefineContext::Image(_)) | ("audio", RefineContext::Audio(_))
            ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_media_processor_keeps_its_source_for_refine() {
        let image = RefineContext::Image(vec![1, 2]);
        let audio = RefineContext::Audio(vec![3, 4].into());
        assert!(retain_result_context("image", 2, true, &image));
        assert!(!retain_result_context("image", 2, false, &image));
        assert!(retain_result_context("audio", 2, true, &audio));
        assert!(!retain_result_context("text", 2, true, &image));
        assert!(retain_result_context("input_adapter", 1, false, &image));
        assert!(retain_result_context("text", 0, true, &image));
    }
}
