use super::backend::{collection_specs, preload_collection};
use super::GalleryEvent;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

pub(super) fn run_preload_worker(
    cache_root: &std::path::Path,
    tx: Sender<GalleryEvent>,
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    for spec in collection_specs().iter().copied() {
        if stop_signal.load(Ordering::Relaxed) {
            let _ = tx.send(GalleryEvent::Paused);
            break;
        }

        match preload_collection(spec, cache_root, &tx, stop_signal.as_ref()) {
            Ok(files) => {
                let _ = tx.send(GalleryEvent::Ready {
                    id: spec.id.to_string(),
                    files,
                });
            }
            Err(message) => {
                let _ = tx.send(GalleryEvent::Error {
                    id: spec.id.to_string(),
                    message,
                });
            }
        }

        if stop_signal.load(Ordering::Relaxed) {
            let _ = tx.send(GalleryEvent::Paused);
            break;
        }
    }
}
