//! Capture lifecycle and recording-drain tests.

use super::*;

/// The hooks and the event queue are process-wide.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn events_survive_repeated_stops_until_a_recording_is_finalized() {
    let _serial = SERIAL.lock();
    while EVENT_QUEUE.pop().is_some() {}
    assert!(
        EVENT_QUEUE
            .push(QueuedInputEvent {
                kind: QueuedEventKind::Wheel,
                timestamp_ns: 2_000_000_000,
                code: 120,
                modifiers: 0,
                layout: 0,
            })
            .is_ok()
    );
    stop_capture(); // The first finalization attempt can time out here.
    stop_capture(); // A later retry must not drain the earlier attempt.
    let events = drain_captured_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "wheel");
    assert_eq!(events[0].timestamp, 2.0);
    assert!(drain_captured_events().is_empty());

    // Once the capture publishes its media-timeline origin, events are
    // measured from it, not from when the hooks were installed.
    set_timeline_origin(timeline::anchor() + std::time::Duration::from_millis(500));
    assert!(
        EVENT_QUEUE
            .push(QueuedInputEvent {
                kind: QueuedEventKind::Wheel,
                timestamp_ns: 2_000_000_000,
                code: 120,
                modifiers: 0,
                layout: 0,
            })
            .is_ok()
    );
    let events = drain_captured_events();
    assert_eq!(events[0].timestamp, 1.5);
}

/// Delivers one key event through the production keyboard hook procedure.
fn deliver_key(message: u32, vk: u32) {
    let event = KBDLLHOOKSTRUCT {
        vkCode: vk,
        ..Default::default()
    };
    unsafe {
        keyboard_hook_proc(
            HC_ACTION as i32,
            WPARAM(message as usize),
            LPARAM(&event as *const KBDLLHOOKSTRUCT as isize),
        );
    }
}

#[test]
fn keystrokes_are_captured_with_the_keystroke_overlay_off() {
    let _serial = SERIAL.lock();
    // The start request carries no overlay setting: both recording backends
    // start the hooks the same way, so the overlay can be enabled later.
    start_capture().unwrap();
    assert!(
        KEYBOARD_HOOKED.load(Ordering::Acquire),
        "every recording installs the keyboard hook"
    );
    const VK_F24: u32 = 0x87;
    deliver_key(WM_KEYDOWN, VK_F24);
    deliver_key(WM_KEYUP, VK_F24);
    stop_capture();
    assert!(!KEYBOARD_HOOKED.load(Ordering::Acquire));

    let typed: Vec<_> = drain_captured_events()
        .into_iter()
        .filter(|event| event.vk == Some(VK_F24))
        .map(|event| (event.event_type, event.direction))
        .collect();
    assert_eq!(
        typed,
        vec![("keyboard", Some("down")), ("keyboard", Some("up"))]
    );
}
