use sgt_screen_text_detector_protocol::stream::{self, Request};
use std::io::Read;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(crate) enum Incoming {
    Capture {
        id: u64,
        jpeg: Vec<u8>,
        cancelled: Arc<AtomicBool>,
    },
    Stop,
}

pub(crate) fn start(
    mut reader: impl Read + Send + 'static,
    active: Arc<Mutex<(u64, Arc<AtomicBool>)>>,
    mut last_id: u64,
) -> mpsc::Receiver<Incoming> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        while let Ok((id, request)) = stream::read_request(&mut reader) {
            match request {
                Request::Capture(jpeg) => {
                    let mut current = active.lock().unwrap();
                    if current.0 != 0 || id <= last_id {
                        break;
                    }
                    last_id = id;
                    let cancelled = Arc::new(AtomicBool::new(false));
                    *current = (id, Arc::clone(&cancelled));
                    drop(current);
                    if sender
                        .try_send(Incoming::Capture {
                            id,
                            jpeg,
                            cancelled,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Request::Cancel => {
                    let current = active.lock().unwrap();
                    if current.0 == id {
                        current.1.store(true, Ordering::Release);
                    }
                }
                Request::Shutdown | Request::Hello(_) => break,
            }
        }
        active.lock().unwrap().1.store(true, Ordering::Release);
        let _ = sender.try_send(Incoming::Stop);
    });
    receiver
}
