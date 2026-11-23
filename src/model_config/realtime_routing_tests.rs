use std::sync::mpsc;
use std::time::Duration;

use super::*;

#[test]
fn realtime_model_routing_remains_available_while_app_state_is_locked() {
    let app = crate::APP.lock().expect("app state lock must be available");
    let (sender, receiver) = mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(move || {
            let route = realtime_transcription_live_protocol(GEMINI_LIVE_TRANSLATE_MODEL_ID);
            sender.send(route).expect("route result must be received");
        });

        let route = receiver.recv_timeout(Duration::from_secs(2));
        drop(app);

        assert_eq!(
            route,
            Ok(Some("live-translate")),
            "realtime model routing must not wait for mutable app state"
        );
    });
}

#[test]
fn source_language_choice_follows_the_catalog_capability() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../catalog/model_catalog.json")).unwrap();
    let declared = manifest["realtime_transcription_requires_source_language"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect::<Vec<_>>();
    for &(id, _) in realtime_transcription_model_options() {
        assert_eq!(
            realtime_transcription_requires_source_language(id),
            declared.contains(&id),
            "{id}"
        );
    }
    for id in declared {
        assert!(realtime_transcription_requires_source_language(id), "{id}");
    }
}
