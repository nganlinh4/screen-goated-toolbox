use super::delivery::{self, ApplyResult, Operation};
use super::state::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(super) fn apply_pending_text() {
    if let Some(pending) = EDITOR_SESSIONS.take_pending() {
        pending.dispatch(&EDITOR_SESSIONS, apply_text);
    }
}

fn apply_text(
    text: String,
    action: EditorAction,
    owner: Option<EditorOwner>,
    operation: Option<Arc<Operation>>,
) {
    let Some(operation) = operation else { return };
    let Some(sequence) = operation.sequence() else {
        operation.fail_uncertain();
        return;
    };
    if !operation.admit() {
        if EDITOR_SESSIONS.finish_operation(operation.id()) {
            super::wake_editor_queue();
        }
        return;
    }
    // Renderer admission remains unsettled until this potentially reentrant
    // synchronous focus step has returned and the page acknowledges the edit.
    if owner.is_some() || matches!(&action, EditorAction::Focus { .. }) {
        super::refocus_editor_guarded(owner.clone(), operation.session());
    }
    if matches!(&action, EditorAction::Focus { retry: false }) {
        super::focus::schedule_retry(operation.session(), owner.clone());
    }
    if DOCUMENT_NONCE.lock().unwrap().as_deref() != Some(operation.nonce()) {
        operation.fail_uncertain();
        return;
    }
    let native_passive = PASSIVE_CAPTURE_ENABLED.load(Ordering::SeqCst)
        && matches!(&action, EditorAction::Insert | EditorAction::Replace);
    let native = if native_passive {
        let replace = matches!(&action, EditorAction::Replace);
        let mutation = || super::passive::apply_text(operation.session(), &text, replace);
        let result = if let Some(owner) = &owner {
            owner.token.run_if_active(mutation).flatten()
        } else {
            mutation()
        };
        let Some(result) = result else {
            operation.ack_apply(ApplyResult::Rejected);
            return;
        };
        Some(result)
    } else {
        None
    };
    let (kind, revision, passive, snapshot) = match &action {
        EditorAction::Insert => ("insert", "", false, ""),
        EditorAction::Replace => ("replace", "", false, ""),
        EditorAction::Focus { .. } => ("focus", "", false, ""),
        EditorAction::Clear {
            revision,
            passive,
            snapshot,
        } => ("clear", revision.as_str(), *passive, snapshot.as_str()),
    };
    let payload = serde_json::json!({
        "nonce": operation.nonce(), "session": operation.session().to_string(),
        "id": operation.id().to_string(), "sequence": sequence.to_string(), "text": text,
        "action": kind, "revision": revision, "passive": passive, "snapshot": snapshot,
        "nativePassive": native_passive,
        "nativeSnapshot": native.as_ref().map(|value| value.0.as_str()).unwrap_or(""),
        "nativeRevision": native.as_ref().map(|value| value.1.as_str()).unwrap_or(""),
        "focus": !PASSIVE_CAPTURE_ENABLED.load(Ordering::SeqCst),
    });
    let script = format!(
        r#"(() => {{
        const p = {payload};
        const reply = status => ({{nonce:window.__sgtDocumentNonce, session:p.session, id:p.id, status}});
        const atMost = (a, b) => a.length < b.length || (a.length === b.length && a <= b);
        if (window.__sgtDocumentNonce !== p.nonce || window.__sgtTextSession !== p.session ||
            !window.__sgtCancelledThrough || atMost(p.sequence, window.__sgtCancelledThrough)) return reply('rejected');
        const editor = document.getElementById('editor');
        if (!editor) return reply('rejected');
        if (p.nativePassive) {{
            if (atMost(window.__sgtPassiveRevision || '0', p.nativeRevision)) {{
                editor.value = p.nativeSnapshot;
                editor.selectionStart = editor.selectionEnd = editor.value.length;
                window.__sgtPassiveRevision = p.nativeRevision;
                window.__sgtTextRevision = (BigInt(window.__sgtTextRevision || '0') + 1n).toString();
            }}
            return reply('applied');
        }}
        if (p.action === 'focus') {{
            if (!p.focus) return reply('skipped');
            editor.focus();
            return reply('applied');
        }}
        if (p.action === 'clear') {{
            if (p.passive ? window.__sgtPassiveRevision !== p.revision || editor.value !== p.snapshot : window.__sgtTextRevision !== p.revision) return reply('skipped');
            if (p.passive && p.revision === '18446744073709551615') return reply('skipped');
            editor.value = '';
            editor.selectionStart = editor.selectionEnd = 0;
            window.__sgtTextRevision = (BigInt(window.__sgtTextRevision || '0') + 1n).toString();
            if (p.passive) window.__sgtPassiveRevision = (BigInt(p.revision) + 1n).toString();
            if (p.focus) editor.focus();
            return reply('applied');
        }}
        if (p.action === 'replace') {{
            editor.value = p.text;
            editor.selectionStart = editor.selectionEnd = p.text.length;
        }} else if (p.action === 'insert') {{
            const start = editor.selectionStart, end = editor.selectionEnd;
            editor.value = editor.value.substring(0, start) + p.text + editor.value.substring(end);
            editor.selectionStart = editor.selectionEnd = start + p.text.length;
        }} else return reply('rejected');
        window.__sgtTextRevision = (BigInt(window.__sgtTextRevision || '0') + 1n).toString();
        if (p.focus) editor.focus();
        return reply('applied');
    }})()"#
    );
    let callback_operation = operation.clone();
    let callback_action = action;
    let submitted = TEXT_INPUT_WEBVIEW.with(|webview| {
        webview.borrow().as_ref().is_some_and(|wv| {
            wv.evaluate_script_with_callback(&script, move |result| {
                let outcome = classify_apply_status(
                    validated_status(&callback_operation, &result),
                    native_passive,
                );
                if outcome == ApplyResult::Applied
                    && let EditorAction::Clear {
                        revision,
                        passive: true,
                        ..
                    } = &callback_action
                {
                    super::passive::clear_if_revision(callback_operation.session(), revision);
                }
                callback_operation.ack_apply(outcome);
            })
            .is_ok()
        })
    });
    if !submitted {
        operation.fail_uncertain();
    }
}

fn classify_apply_status(status: Option<&str>, native_applied: bool) -> ApplyResult {
    match status {
        Some("applied") => ApplyResult::Applied,
        Some("rejected") if native_applied => ApplyResult::Applied,
        Some("rejected") => ApplyResult::Rejected,
        Some("skipped") => ApplyResult::Skipped,
        _ => ApplyResult::Uncertain,
    }
}

pub(super) fn revoke_operation(id: u64) {
    let Some(operation) = delivery::get(id) else {
        return;
    };
    let Some(sequence) = operation.sequence() else {
        operation.fail_uncertain();
        return;
    };
    if DOCUMENT_NONCE.lock().unwrap().as_deref() != Some(operation.nonce()) {
        operation.fail_uncertain();
        return;
    }
    let payload = serde_json::json!({
        "nonce": operation.nonce(), "session": operation.session().to_string(),
        "id": operation.id().to_string(), "sequence": sequence.to_string(),
    });
    let script = format!(
        r#"(() => {{
        const p = {payload};
        const reply = status => ({{nonce:window.__sgtDocumentNonce, session:p.session, id:p.id, status}});
        if (window.__sgtDocumentNonce !== p.nonce || !window.__sgtCancelledThrough) return reply('invalid');
        const atMost = (a, b) => a.length < b.length || (a.length === b.length && a <= b);
        if (window.__sgtTextSession === p.session && !atMost(p.sequence, window.__sgtCancelledThrough))
            window.__sgtCancelledThrough = p.sequence;
        return reply('revoked');
    }})()"#
    );
    let callback_operation = operation.clone();
    let submitted = TEXT_INPUT_WEBVIEW.with(|webview| {
        webview.borrow().as_ref().is_some_and(|wv| {
            wv.evaluate_script_with_callback(&script, move |result| {
                let valid = validated_status(&callback_operation, &result) == Some("revoked");
                callback_operation.ack_revoke(valid);
            })
            .is_ok()
        })
    });
    if !submitted {
        operation.fail_uncertain();
    }
}

fn validated_status(operation: &Operation, result: &str) -> Option<&'static str> {
    let parsed: serde_json::Value = serde_json::from_str(result).ok()?;
    if parsed.get("nonce")?.as_str()? != operation.nonce()
        || parsed.get("session")?.as_str()? != operation.session().to_string()
        || parsed.get("id")?.as_str()? != operation.id().to_string()
    {
        return None;
    }
    match parsed.get("status")?.as_str()? {
        "applied" => Some("applied"),
        "rejected" => Some("rejected"),
        "skipped" => Some("skipped"),
        "revoked" => Some("revoked"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_insert_before_revocation_is_not_reported_as_cancelled() {
        let operation = Operation::new(1, "native-test".into(), None, Arc::new(|_| {})).unwrap();
        assert!(operation.set_sequence(1));
        assert!(operation.admit());
        assert!(operation.cancel());
        assert_eq!(operation.ack_revoke(true), None);
        assert_eq!(
            operation.ack_apply(classify_apply_status(Some("rejected"), true)),
            Some(delivery::Settlement::AppliedBeforeCancellationSettled),
        );
        assert_eq!(classify_apply_status(None, true), ApplyResult::Uncertain);
    }
}
