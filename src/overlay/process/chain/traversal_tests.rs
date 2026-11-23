use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::time::Duration;

use super::super::AutoPasteDestination;
use super::{ChainStepRequest, run_chain_step};
use crate::config::{Config, ProcessingBlock};
use crate::overlay::result::{ChainCancelToken, RefineContext};
use windows::Win32::Foundation::RECT;

type Visit = (usize, AutoPasteDestination);
static OBSERVERS: LazyLock<Mutex<HashMap<String, mpsc::Sender<Visit>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[test]
fn first_processing_node_depends_on_path_not_disconnected_drafts() {
    let blocks = ["input_adapter", "text", "image", "text"].map(|kind| ProcessingBlock {
        block_type: kind.into(),
        ..Default::default()
    });
    assert!(super::super::path_state::first_processing_in_path(
        &blocks,
        &[0, 2]
    ));
    assert!(!super::super::path_state::first_processing_in_path(
        &blocks,
        &[0, 2, 3]
    ));
    assert!(!super::super::path_state::first_processing_in_path(
        &blocks,
        &[99, 2]
    ));
}

pub(super) fn record_visit(chain_id: &str, block_idx: usize, destination: AutoPasteDestination) {
    if let Some(observer) = OBSERVERS.lock().unwrap().get(chain_id) {
        let _ = observer.send((block_idx, destination));
    }
}

#[test]
fn cycle_and_invalid_edges_stop_before_execution_but_branches_can_rejoin() {
    exercise_branches(AutoPasteDestination::External(None));
}

#[test]
fn forked_and_rejoined_steps_keep_the_request_destination() {
    use crate::win_types::SendHwnd;
    use windows::Win32::Foundation::HWND;
    let window = SendHwnd(HWND(17_usize as *mut std::ffi::c_void));
    for destination in [
        AutoPasteDestination::External(Some(window)),
        AutoPasteDestination::TextInput(41),
        AutoPasteDestination::Refine(window, 53),
    ] {
        exercise_branches(destination);
    }
}

fn assert_destination(actual: AutoPasteDestination, expected: AutoPasteDestination) {
    match (actual, expected) {
        (AutoPasteDestination::External(actual), AutoPasteDestination::External(expected)) => {
            assert_eq!(
                actual.map(|window| window.as_isize()),
                expected.map(|window| window.as_isize())
            )
        }
        (AutoPasteDestination::TextInput(actual), AutoPasteDestination::TextInput(expected)) => {
            assert_eq!(actual, expected)
        }
        (
            AutoPasteDestination::Refine(window, session),
            AutoPasteDestination::Refine(expected_window, expected_session),
        ) => {
            assert_eq!(window.as_isize(), expected_window.as_isize());
            assert_eq!(session, expected_session);
        }
        _ => panic!("A chain redirected its destination: {actual:?}, expected {expected:?}"),
    }
}

fn exercise_branches(destination: AutoPasteDestination) {
    // Starting at 1 avoids app/global-state access; adapters are invisible and
    // do not call providers or touch user data. Real branch threads and the
    // actual step-entry guard are exercised.
    let blocks = (0..5)
        .map(|_| ProcessingBlock {
            block_type: "input_adapter".to_string(),
            show_overlay: false,
            ..Default::default()
        })
        .collect();
    let chain_id = format!("traversal-test-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let (tx, rx) = mpsc::channel();
    OBSERVERS.lock().unwrap().insert(chain_id.clone(), tx);
    let worker_chain_id = chain_id.clone();
    let (finished_tx, finished_rx) = mpsc::channel();
    std::thread::spawn(move || {
        run_chain_step(ChainStepRequest {
            block_idx: 1,
            input_text: "input".to_string(),
            current_rect: RECT::default(),
            blocks,
            connections: vec![(1, 2), (1, 3), (2, 4), (3, 4), (4, 1), (4, 4), (4, 99)],
            graph_explicit: true,
            config: Config::default(),
            parent_hwnd: Arc::new(Mutex::new(None)),
            context: RefineContext::None,
            skip_execution: false,
            preprocessed_audio: None,
            visited_blocks: Vec::new(),
            processing_indicator_hwnd: None,
            cancel_token: ChainCancelToken::new(),
            preset_id: String::new(),
            disable_auto_paste: true,
            auto_paste_destination: destination,
            chain_id: worker_chain_id,
            input_hwnd_refocus: None,
        });
        let _ = finished_tx.send(());
    });
    finished_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut visited = Vec::new();
    for _ in 0..5 {
        let (index, actual_destination) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_destination(actual_destination, destination);
        visited.push(index);
    }
    visited.sort_unstable();
    assert_eq!(visited, [1, 2, 3, 4, 4]);
    // Give delayed sibling branches enough time to reach their rejected edges.
    assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
    OBSERVERS.lock().unwrap().remove(&chain_id);
}
