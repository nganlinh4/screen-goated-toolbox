use super::*;
use crate::api::gemini_live::{GeminiLiveManager, test_support::*};

#[test]
fn empty_chunk_does_not_start_output_or_consume_thinking_presentation() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/preset-system/live-response-wait.json"
    ))
    .unwrap();
    assert_eq!(fixture["emptyChunkStartsOutput"], false);
    let manager = GeminiLiveManager::new();
    let (response, options) = request(&manager, None);
    {
        let queue = manager.work_queue.lock().unwrap();
        let producer = &queue[0];
        assert!(producer.publish(LiveEvent::TextChunk(String::new())));
        assert!(
            !options
                .first_output_received
                .load(std::sync::atomic::Ordering::SeqCst)
        );
        assert!(response.receiver.try_recv().is_err());
        producer.response_tx.send(LiveEvent::Thinking).unwrap();
        producer
            .response_tx
            .send(LiveEvent::TextChunk(String::new()))
            .unwrap();
    }
    publish(&manager, LiveEvent::TextChunk("visible".to_string()));
    publish(&manager, LiveEvent::TurnComplete);
    let mut chunks = Vec::new();
    assert_eq!(
        collect_response(response, &options, true, "en", |text| chunks
            .push(text.to_string()))
        .unwrap(),
        "visible"
    );
    let thinking = crate::gui::locale::LocaleText::get("en")
        .global_settings
        .model_thinking;
    assert_eq!(
        chunks,
        vec![
            thinking.to_string(),
            format!("{}visible", crate::api::WIPE_SIGNAL)
        ]
    );
}

#[test]
fn final_only_output_requires_explicit_completion_and_nonblank_content() {
    let manager = GeminiLiveManager::new();
    let (response, options) = request(&manager, None);
    publish(&manager, LiveEvent::TextChunk("one".to_string()));
    publish(&manager, LiveEvent::TextChunk("two".to_string()));
    publish(&manager, LiveEvent::TurnComplete);
    let mut chunks = Vec::new();
    assert_eq!(
        collect_response(response, &options, false, "en", |text| chunks
            .push(text.to_string()))
        .unwrap(),
        "onetwo"
    );
    assert_eq!(chunks, ["onetwo"]);
    let (response, options) = request(&manager, None);
    publish(&manager, LiveEvent::TurnComplete);
    assert!(
        collect_response(response, &options, false, "en", |_| panic!(
            "blank completion cannot publish"
        ))
        .is_err()
    );
    let (response, options) = request(&manager, None);
    let queued = manager.work_queue.lock().unwrap().pop_front().unwrap();
    assert!(queued.publish(LiveEvent::TextChunk("partial".to_string())));
    drop(queued);
    assert!(
        collect_response(response, &options, false, "en", |_| panic!(
            "incomplete response cannot publish"
        ))
        .is_err()
    );
}

#[test]
fn callback_failure_retires_queue_and_socket_without_cancelling_other_owners() {
    let manager = GeminiLiveManager::new();
    let shared = cancel_flag();
    let (response, options) = request(&manager, Some(shared.clone()));
    let (client, peer) = tcp_pair();
    response.control.bind_transport(&client).unwrap();
    publish(&manager, LiveEvent::TextChunk("first".to_string()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = collect_response(response, &options, true, "en", |_| {
            panic!("callback failed")
        });
    }));
    assert!(result.is_err());
    assert!(manager.work_queue.lock().unwrap().is_empty());
    assert_not_cancelled(&shared);
    assert_closed(peer);
}
