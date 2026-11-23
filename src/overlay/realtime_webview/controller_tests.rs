use super::*;

#[test]
fn committed_read_cursor_matches_shared_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/read-cursor.json"
    )))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut cursor = ReadCursor::default();
        let actual: Vec<_> = case["updates"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter_map(|(index, update)| {
                let revision = case["revisions"][index].as_u64().unwrap_or(0);
                cursor.update(update.as_str().unwrap(), revision).1
            })
            .collect();
        let expected: Vec<_> = case["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|segment| segment.as_str().unwrap())
            .collect();
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}

#[test]
fn revision_retires_coalesced_reset_even_when_new_text_has_old_prefix() {
    let mut cursor = ReadCursor::default();
    assert_eq!(cursor.update("First", 1), (false, Some("First".into())));
    assert_eq!(
        cursor.update("First again", 2),
        (true, Some("First again".into()))
    );
    assert_eq!(cursor.update("First again", 2), (false, None));
}

#[test]
fn initial_sentence_search_preserves_unicode_boundaries() {
    assert_eq!(previous_sentence_end("Finished. 다음"), Some(9));
    assert_eq!(previous_sentence_end("Done.   "), None);
    assert_eq!(previous_sentence_end("한글"), None);
    assert_eq!(previous_sentence_end("First! Second?"), Some(6));
    assert_eq!(previous_sentence_end(""), None);
}

#[test]
fn direct_speech_read_state_survives_transcription_and_render_refreshes() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/read-state.json"
    )))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let state = std::sync::atomic::AtomicBool::new(case["initialEnabled"].as_bool().unwrap());
        for event in case["events"].as_array().unwrap() {
            if let Some(direct) = event["directSpeech"].as_bool() {
                reconcile_read_enabled(&state, direct);
            }
            assert_eq!(
                state.load(Ordering::SeqCst),
                event["expectedEnabled"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn first_enable_skips_history_like_the_tts_queue_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/tts-runtime/queue-semantics.json"
    )))
    .unwrap();
    let cases: Vec<_> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["action"] == "enable_realtime")
        .collect();
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let committed = case["initial_committed_text"].as_str().unwrap();
        // A fresh cursor is the producer state the fixture starts from.
        assert_eq!(case["initial_state"]["spoken_length"], 0, "{name}");
        assert_eq!(case["initial_state"]["queued_length"], 0, "{name}");
        let expected = &case["expected"];
        let expected_segments: Vec<_> = expected["queued_segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|segment| segment.as_str().unwrap().to_owned())
            .collect();

        let mut cursor = ReadCursor::default();
        let (replaced, segment) = cursor.update(committed, 1);
        assert!(!replaced, "{name}");
        let segments: Vec<_> = segment.into_iter().collect();
        assert_eq!(segments, expected_segments, "{name}");

        let queued_bytes = cursor.text.len();
        let skipped_bytes = queued_bytes - segments.iter().map(String::len).sum::<usize>();
        let spoken = committed[..skipped_bytes].chars().count() as u64;
        let queued = committed[..queued_bytes].chars().count() as u64;
        assert_eq!(Some(spoken), expected["spoken_length"].as_u64(), "{name}");
        assert_eq!(Some(queued), expected["queued_length"].as_u64(), "{name}");
        // Delivering the same committed text again queues nothing more.
        assert_eq!(cursor.update(committed, 1), (false, None), "{name}");
    }
}
