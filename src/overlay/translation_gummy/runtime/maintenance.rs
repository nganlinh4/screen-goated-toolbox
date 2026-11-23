use std::sync::{LazyLock, mpsc};

type Detection = (u64, String);

static DETECTIONS: LazyLock<mpsc::SyncSender<Detection>> = LazyLock::new(|| {
    let (tx, rx) = mpsc::sync_channel::<Detection>(64);
    std::thread::spawn(move || {
        while let Ok((id, text)) = rx.recv() {
            let Some(lang) = crate::lang_detect::detect_language(&text) else {
                continue;
            };
            if super::super::state::with_state(|state| {
                apply_language(&mut state.transcripts, id, &text, lang)
            }) {
                super::super::state::request_sync();
                request_persist();
            }
        }
    });
    tx
});

fn apply_language(
    items: &mut [super::TranslationGummyTranscriptItem],
    id: u64,
    text: &str,
    lang: String,
) -> bool {
    if let Some(item) = items
        .iter_mut()
        .find(|item| item.id == id && item.is_final && item.text == text)
    {
        item.lang = lang;
        true
    } else {
        false
    }
}

static PERSISTENCE: LazyLock<mpsc::SyncSender<()>> = LazyLock::new(|| {
    let (tx, rx) = mpsc::sync_channel::<()>(1);
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            super::super::state::persist_transcripts();
        }
    });
    tx
});

pub(super) fn request_detection(pending: Vec<(u64, String)>) {
    for (id, text) in pending {
        if text.len() <= 65_536 {
            let _ = DETECTIONS.try_send((id, text));
        }
    }
}

pub(in crate::overlay::translation_gummy) fn request_persist() {
    let _ = PERSISTENCE.try_send(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_detection_requires_same_final_row_and_text() {
        let mut rows = vec![super::super::TranslationGummyTranscriptItem {
            id: 42,
            role: "input",
            text: "new text".into(),
            is_final: true,
            lang: String::new(),
        }];
        assert!(!apply_language(&mut rows, 42, "old text", "eng".into()));
        assert!(!apply_language(&mut rows, 41, "new text", "eng".into()));
        rows[0].is_final = false;
        assert!(!apply_language(&mut rows, 42, "new text", "eng".into()));
        rows[0].is_final = true;
        assert!(apply_language(&mut rows, 42, "new text", "eng".into()));
        assert_eq!(rows[0].lang, "eng");
    }
}
