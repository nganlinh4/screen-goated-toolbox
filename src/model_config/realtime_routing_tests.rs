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
