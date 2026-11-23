use super::super::{Session, SessionKind, display_change};
use std::sync::{Arc, atomic::Ordering};

pub(in super::super) fn start(session: Arc<Session>) {
    let view = session.view.lock().unwrap();
    if session.kind != SessionKind::Interactive
        || session.stop.load(Ordering::Acquire)
        || session.started.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let completion = session.capture_lifetime.begin();
    crate::overlay::result::scene_compositor::exclude_from_capture(true);
    drop(view);
    let translator = Arc::clone(&session);
    std::thread::spawn(move || super::super::pipeline::run(translator));
    std::thread::spawn(move || {
        let result = super::observe(&session);
        // observe owns Capture: its Close calls must finish before stop can return.
        drop(completion);
        if let Err(error) = result
            && !session.stop.load(Ordering::Acquire)
        {
            if error
                .downcast_ref::<display_change::DisplayChanged>()
                .is_some()
            {
                display_change::stop(&session);
            } else {
                super::super::stop_expected(Some(&session));
                super::super::super::capture::notify_error(&error.to_string());
            }
        }
    });
}
