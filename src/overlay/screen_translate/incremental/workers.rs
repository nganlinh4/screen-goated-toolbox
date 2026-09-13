use super::{Batch, Message, coordinator::LANES};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) fn start<'scope, 'env>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    sender: &mpsc::SyncSender<Message>,
    cancel: &Arc<AtomicBool>,
    trace: &str,
    settings: &crate::config::types::ScreenTranslateSettings,
) -> [mpsc::SyncSender<Batch>; LANES] {
    std::array::from_fn(|lane| {
        let (requests, batches) = mpsc::sync_channel::<Batch>(1);
        let cancel = Arc::clone(cancel);
        let sender = sender.clone();
        let trace = trace.to_string();
        let settings = settings.clone();
        scope.spawn(move || {
            while let Ok(batch) = batches.recv() {
                if cancel.load(Ordering::Acquire) { break; }
                crate::log_info!("[ScreenTranslateSchedule] trace={trace} batch={} lane={lane} units={} source_bytes={} context_units={}", batch.sequence,
                    batch.candidates.len(), batch.candidates.iter().map(|c| c.source_text.len()).sum::<usize>(),
                    batch.scene.iter().filter(|c| !c.source_text.is_empty()).count());
                let result = super::super::inference::translate(
                    super::super::inference::TranslateInput {
                        trace_id: &trace,
                        target_language: &settings.target_language,
                        translation_model: &settings.translation_model,
                        translation_prompt: &settings.translation_prompt,
                        candidates: &batch.candidates,
                        scene: &batch.scene,
                        prior_translations: &batch.prior_translations,
                    },
                    Arc::clone(&cancel),
                    |region| { let _ = sender.send(Message::Translated(lane, region)); },
                );
                if sender.send(Message::BatchDone(lane, result.map_err(|e| e.to_string()))).is_err() { break; }
            }
        });
        requests
    })
}
