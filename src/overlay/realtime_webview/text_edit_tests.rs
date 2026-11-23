use super::super::layout::CardRole;
use super::super::mailbox::{CommandBuffer, PushResult};
use super::super::protocol::{CardText, HostCommand, TextEdit};
use super::{apply, update};

/// What the card script does with an edit: JavaScript strings are UTF-16.
fn apply_like_the_card(shown: &mut Vec<u16>, edit: &TextEdit) {
    assert!(
        edit.remove_utf16 <= shown.len(),
        "edit removes more than is shown"
    );
    shown.truncate(shown.len() - edit.remove_utf16);
    shown.extend(edit.append.encode_utf16());
}

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn a_growing_transcript_sends_only_what_was_added() {
    let mut scene = CardText::default();
    let mut card = Vec::new();
    let mut transcript = String::new();
    let mut sent_bytes = 0;
    let mut full_resend_bytes = 0;
    for index in 0..2_000 {
        let word = format!("word{index} 한국어 ");
        transcript.push_str(&word);
        let draft = format!("draft {index}");
        let shown_before = scene.committed.len();
        let edit = update(&mut scene, &transcript, &draft).expect("the text changed");
        assert_eq!(edit.remove_utf16, 0, "an append removes nothing");
        // The card trims trailing spaces, so the separator moves with the word.
        assert_eq!(edit.append, transcript.trim_end()[shown_before..]);
        assert!(
            edit.append.len() <= word.len(),
            "only the new words are sent"
        );
        let command = serde_json::to_string(&HostCommand::TextEdit {
            role: CardRole::Transcription,
            edit: edit.clone(),
        })
        .unwrap();
        assert!(
            command.len() < word.len() * 3 + draft.len() + 96,
            "{command}"
        );
        sent_bytes += command.len();
        full_resend_bytes += transcript.len();
        apply_like_the_card(&mut card, &edit);
        assert_eq!(card, utf16(transcript.trim_end()));
        assert_eq!(scene.committed, transcript.trim_end());
        assert_eq!(scene.draft, format!(" {draft}"));
    }
    // Work is linear in what was added plus a bounded envelope per update,
    // while resending the whole card would grow with the square of the length.
    assert!(
        sent_bytes < transcript.len() * 2 + 2_000 * 128,
        "sent {sent_bytes} bytes for a {} byte transcript",
        transcript.len()
    );
    assert!(
        sent_bytes * 50 < full_resend_bytes,
        "sent {sent_bytes} bytes where full resends cost {full_resend_bytes}"
    );
}

#[test]
fn corrections_remove_only_the_changed_tail_in_utf16_units() {
    let mut scene = CardText::default();
    let mut card = Vec::new();
    for (committed, removed) in [
        ("Xin chào 😀 one", 0),
        ("Xin chào 😀 two", 3),
        ("Xin chào 😃", 6),
        ("Xin chào", 3),
        ("Xin chào 한국어 😀", 0),
        ("", 15),
    ] {
        let edit = update(&mut scene, committed, "").unwrap();
        assert_eq!(edit.remove_utf16, removed, "{committed}");
        apply_like_the_card(&mut card, &edit);
        assert_eq!(card, utf16(committed));
    }
}

#[test]
fn spacing_matches_the_card_and_an_unchanged_card_sends_nothing() {
    let mut scene = CardText::default();
    let edit = update(&mut scene, "done  ", "  next").unwrap();
    assert_eq!(
        (edit.append.as_str(), edit.draft.as_str()),
        ("done", " next")
    );
    assert!(update(&mut scene, "done ", " next").is_none());
    assert_eq!(update(&mut scene, "", "next").unwrap().draft, "next");
    assert!(update(&mut scene, "", " next").is_none());
}

#[test]
fn queued_edits_fold_into_one_command_with_the_same_result() {
    let mut scene = CardText::default();
    let mut buffer = CommandBuffer::default();
    let mut transcript = String::new();
    for index in 0..500 {
        if index % 7 == 6 {
            // A recognizer correction rewrites the last word.
            let keep = transcript
                .trim_end()
                .rfind(' ')
                .map_or(0, |space| space + 1);
            transcript.truncate(keep);
            transcript.push_str("fixed😀 ");
        } else {
            transcript.push_str(&format!("w{index} "));
        }
        let edit = update(&mut scene, &transcript, "tail").unwrap();
        let command = HostCommand::TextEdit {
            role: CardRole::Transcription,
            edit,
        };
        assert_eq!(buffer.push(command), PushResult::Queued);
    }
    let commands = buffer.drain();
    let [HostCommand::TextEdit { edit, .. }] = commands.as_slice() else {
        panic!("expected one folded edit, got {commands:?}");
    };
    let mut card = Vec::new();
    apply_like_the_card(&mut card, edit);
    assert_eq!(card, utf16(transcript.trim_end()));
    assert_eq!(edit.draft, " tail");
}

#[test]
fn an_edit_queued_behind_full_text_updates_that_text() {
    let mut buffer = CommandBuffer::default();
    let full = CardText {
        committed: "hello wor".to_string(),
        draft: String::new(),
    };
    buffer.push(HostCommand::Text {
        role: CardRole::Translation,
        text: full.clone(),
    });
    let mut scene = full;
    let edit = update(&mut scene, "hello world", "!").unwrap();
    buffer.push(HostCommand::TextEdit {
        role: CardRole::Translation,
        edit: edit.clone(),
    });
    buffer.push(HostCommand::TextEdit {
        role: CardRole::Transcription,
        edit,
    });
    let commands = buffer.drain();
    assert_eq!(commands.len(), 2, "roles are separate lanes");
    let HostCommand::Text { text, .. } = &commands[0] else {
        panic!("the pending full text absorbs the edit");
    };
    assert_eq!(text, &scene);

    let mut applied = CardText::default();
    apply(
        &mut applied,
        &TextEdit {
            remove_utf16: 5,
            append: "abc".to_string(),
            draft: "d".to_string(),
        },
    );
    assert_eq!(
        (applied.committed.as_str(), applied.draft.as_str()),
        ("abc", "d")
    );
}
