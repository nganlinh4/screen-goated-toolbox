use super::{collapse_separators, prune_transcripts};
use crate::overlay::translation_gummy::TranslationGummyTranscriptItem;

fn item(id: u64, role: &'static str) -> TranslationGummyTranscriptItem {
    TranslationGummyTranscriptItem {
        id,
        role,
        text: id.to_string(),
        is_final: true,
        lang: String::new(),
    }
}

#[test]
fn pruning_does_not_leave_an_orphaned_output_or_separator() {
    let mut items = vec![
        item(1, "separator"),
        item(2, "input"),
        item(3, "output"),
        item(4, "separator"),
        item(5, "input"),
        item(6, "output"),
    ];

    prune_transcripts(&mut items, 3);

    assert_eq!(
        items.iter().map(|item| item.role).collect::<Vec<_>>(),
        ["input", "output"]
    );
}

#[test]
fn separator_cleanup_removes_leading_and_duplicate_separators() {
    let mut items = vec![
        item(1, "separator"),
        item(2, "separator"),
        item(3, "input"),
        item(4, "separator"),
        item(5, "separator"),
        item(6, "output"),
    ];

    collapse_separators(&mut items);

    assert_eq!(
        items.iter().map(|item| item.role).collect::<Vec<_>>(),
        ["input", "separator", "output"]
    );
}
