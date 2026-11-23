use super::{Admission, admit, close_matches};
use std::sync::{Arc, Mutex};

#[test]
fn stale_admission_cannot_replace_current_selector() {
    let session = Arc::new(Mutex::new(1));
    let guard: Admission = {
        let session = session.clone();
        Arc::new(move |effect| {
            let current = session.lock().unwrap();
            if *current != 1 {
                return false;
            }
            effect();
            true
        })
    };
    *session.lock().unwrap() = 2;
    let mut replaced = false;
    assert!(!admit(&Some(guard), &mut || replaced = true));
    assert!(!replaced);
}

#[test]
fn close_message_is_bound_to_window_instance() {
    assert!(close_matches(0, 7));
    assert!(close_matches(7, 7));
    assert!(!close_matches(7, 8));
}
