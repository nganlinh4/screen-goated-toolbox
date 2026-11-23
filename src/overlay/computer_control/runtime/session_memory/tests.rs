use super::*;
use crate::overlay::computer_control::controller::world::SurfaceIdentity;

fn snapshot(frame_id: u64, generation: u64) -> uia_task::SnapshotFrame {
    uia_task::SnapshotFrame {
        b64: "AQID".into(),
        source: uia_task::FrameSource {
            view: super::super::session::View {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            frame_id,
            surface: SurfaceIdentity::Native {
                hwnd: 1,
                pid: 2,
                generation,
            },
        },
        captured_at: Instant::now(),
        byte_count: 3,
    }
}

#[test]
fn idle_frames_and_unconfirmed_speech_never_enter_saved_frames() {
    let _session = session_retention::start_session();
    let mut memory = ConversationMemory::default();
    let frame = snapshot(1, 1);
    memory.snapshot(&frame, false, false);
    memory.action_frame(&frame.b64, &frame.source, 1);
    assert!(memory.frames.is_empty());
    memory.snapshot(&frame, false, true);
    assert!(memory.frames.is_empty());
    memory.confirm_turn(7);
    assert_eq!(memory.frames.len(), 1);
    assert_eq!(memory.frames[0].turn_id, 7);
    assert_eq!(memory.frames[0].source, frame.source);
    memory.snapshot(&snapshot(2, 2), false, false);
    assert_eq!(memory.frames.len(), 1);
}

#[test]
fn new_input_and_expired_provisional_frames_do_not_cross_turns() {
    let _session = session_retention::start_session();
    let mut memory = ConversationMemory::default();
    memory.snapshot(&snapshot(1, 1), false, true);
    memory.begin_input();
    memory.confirm_turn(2);
    assert!(memory.frames.is_empty());
    memory.snapshot(&snapshot(3, 1), false, true);
    memory.provisional.as_mut().unwrap().1 =
        Instant::now() - TRANSCRIPT_FRAME_WINDOW - Duration::from_secs(1);
    memory.confirm_turn(3);
    assert!(memory.frames.is_empty());
}

#[test]
fn stale_snapshots_and_oversize_payloads_are_excluded() {
    let _session = session_retention::start_session();
    let mut memory = ConversationMemory::default();
    memory.confirm_turn(1);
    let mut frame = snapshot(1, 1);
    frame.captured_at = Instant::now() - MAX_SNAPSHOT_AGE - Duration::from_secs(1);
    memory.snapshot(&frame, true, true);
    frame.captured_at = Instant::now();
    frame.b64 = "A".repeat(MAX_ENCODED_FRAME_BYTES + 1);
    memory.snapshot(&frame, true, true);
    assert!(memory.frames.is_empty());
    assert!(memory.provisional.is_none());
}

#[test]
fn representative_frames_are_bounded_and_use_surface_identity() {
    let _session = session_retention::start_session();
    let mut memory = ConversationMemory::default();
    memory.confirm_turn(1);
    memory.snapshot(&snapshot(1, 1), true, false);
    memory.snapshot(&snapshot(2, 1), true, false);
    assert_eq!(memory.frames.len(), 1);
    assert_eq!(memory.frames[0].source.frame_id, 2);
    for id in 2..=10 {
        memory.snapshot(&snapshot(id + 1, id), true, false);
    }
    assert_eq!(memory.frames.len(), MAX_FRAMES);
    assert_eq!(memory.frames[0].source.frame_id, 6);
    assert_eq!(memory.frames.last().unwrap().source.frame_id, 11);
}

#[test]
fn action_results_remain_bound_to_their_confirmed_turn() {
    let _session = session_retention::start_session();
    let mut memory = ConversationMemory::default();
    let frame = snapshot(1, 1);
    memory.confirm_turn(2);
    memory.action_frame(&frame.b64, &frame.source, 1);
    assert!(memory.frames.is_empty());
    memory.action_frame(&frame.b64, &frame.source, 2);
    assert_eq!(memory.frames[0].turn_id, 2);
    memory.confirm_turn(3);
    memory.action_frame(&frame.b64, &frame.source, 3);
    assert_eq!(memory.frames.len(), 2);
}

#[test]
fn private_policy_clears_committed_and_provisional_frames() {
    let _session = session_retention::start_session();
    let mut memory = ConversationMemory::default();
    memory.confirm_turn(1);
    memory.snapshot(&snapshot(1, 1), true, false);
    memory.snapshot(&snapshot(2, 2), false, true);
    session_retention::current().forbid();
    memory.confirm_turn(2);
    assert!(memory.frames.is_empty());
    assert!(memory.provisional.is_none());
    memory.snapshot(&snapshot(3, 3), true, true);
    assert!(memory.frames.is_empty());
    assert!(memory.provisional.is_none());
}
