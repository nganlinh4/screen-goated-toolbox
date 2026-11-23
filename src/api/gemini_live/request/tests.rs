use super::*;
use crate::api::gemini_live::{GeminiLiveManager, test_support::*};

#[test]
fn dropping_response_removes_only_its_exact_queued_work() {
    let manager = GeminiLiveManager::new();
    let shared = cancel_flag();
    let (first, _) = request(&manager, Some(shared.clone()));
    let (second, _) = request(&manager, Some(shared.clone()));
    assert_eq!(manager.work_queue.lock().unwrap().len(), 2);
    drop(first);
    let queue = manager.work_queue.lock().unwrap();
    assert_eq!(queue.len(), 1);
    assert!(Arc::ptr_eq(&queue[0].control, &second.control));
    drop(queue);
    assert_not_cancelled(&shared);
    assert!(!second.is_retired());
    drop(second);
    assert!(manager.work_queue.lock().unwrap().is_empty());
}

#[test]
fn response_teardown_closes_accepted_transport_before_and_during_output() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/preset-system/live-response-wait.json"
    ))
    .unwrap();
    assert_eq!(fixture["cancelBeforeOutput"], "close_owned_socket");
    assert_eq!(
        fixture["cancelDuringOutput"],
        "close_owned_socket_without_late_chunks"
    );
    for content_started in [false, true] {
        let manager = GeminiLiveManager::new();
        let shared = cancel_flag();
        let (response, _) = request(&manager, Some(shared.clone()));
        let (client, peer) = tcp_pair();
        response.control.bind_transport(&client).unwrap();
        if content_started {
            publish(&manager, LiveEvent::TextChunk("first".to_string()));
            assert!(matches!(
                response.receiver.try_recv(),
                Ok(LiveEvent::TextChunk(_))
            ));
        }
        let control = response.control.clone();
        drop(response);
        assert!(control.is_retired());
        assert_closed(peer);
        assert_not_cancelled(&shared);
        assert!(manager.work_queue.lock().unwrap().is_empty());
        drop(client);
    }
}

#[test]
fn retired_response_shuts_down_a_late_transport_registration() {
    let manager = GeminiLiveManager::new();
    let (response, _) = request(&manager, None);
    let control = response.control.clone();
    drop(response);
    let (client, peer) = tcp_pair();
    assert!(control.bind_transport(&client).is_err());
    assert_closed(peer);
}

#[test]
fn retired_publication_cannot_start_output_or_reach_receiver() {
    let manager = GeminiLiveManager::new();
    let (response, options) = request(&manager, None);
    let queued = manager.work_queue.lock().unwrap().pop_front().unwrap();
    response.control.retire();
    assert!(!queued.publish(LiveEvent::TextChunk("late".to_string())));
    assert!(!options.first_output_received.load(Ordering::SeqCst));
    assert!(response.receiver.try_recv().is_err());
}

#[test]
fn manager_generation_and_shutdown_retire_already_queued_events() {
    for shutdown in [false, true] {
        let manager = GeminiLiveManager::new();
        let (response, options) = request(&manager, None);
        publish(&manager, LiveEvent::TextChunk("queued".to_string()));
        if shutdown {
            manager.shutdown.store(true, Ordering::SeqCst);
        } else {
            manager.interrupt_generation.fetch_add(1, Ordering::SeqCst);
        }
        assert!(crate::api::gemini_live::wait::receive_live_event(&response, &options).is_err());
    }
}
