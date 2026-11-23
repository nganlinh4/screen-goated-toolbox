//! Persistent refinement editor runtime for the shared result scene compositor.

pub(super) fn get_javascript() -> &'static str {
    r#"
(function() {
    const editors = new Map();
    let focusMode = false;
    let requestedFocus = null;

    function post(action, hwnd, text, editSession, epoch = window.__SGT_TOPOLOGY_EPOCH__ || '1') {
        const message = { action: action, hwnd: String(hwnd), topology_epoch: epoch };
        if (text !== undefined) message.text = text;
        if (editSession !== undefined) message.editSession = editSession;
        window.ipc.postMessage(JSON.stringify(message));
    }

    function requestNativeFocus(editor, epoch = window.__SGT_TOPOLOGY_EPOCH__ || '1') {
        if (editors.get(editor.hwnd) !== editor || !editor.input.isConnected ||
            epoch !== (window.__SGT_TOPOLOGY_EPOCH__ || '1')) return;
        requestedFocus = editor;
        focusMode = true;
        post('request_refine_focus', editor.hwnd, undefined, editor.editSession, epoch);
    }

    function publishDraft(editor) {
        post('update_refine_draft', editor.hwnd, editor.input.value, editor.editSession);
    }

    function submit(editor) {
        const text = editor.input.value;
        if (text.trim()) post('submit_refine', editor.hwnd, text, editor.editSession);
    }

    function handleKey(editor, event) {
        if (editor.composing || event.isComposing) return;
        if (event.key === 'Enter') {
            event.preventDefault();
            submit(editor);
        } else if (event.key === 'Escape') {
            event.preventDefault();
            post('cancel_refine', editor.hwnd, undefined, editor.editSession);
        } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
            event.preventDefault();
            post(event.key === 'ArrowUp' ? 'history_up_refine' : 'history_down_refine',
                editor.hwnd, editor.input.value, editor.editSession);
        }
    }

    function actionButton(className, label, icon) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = className;
        if (label) button.setAttribute('aria-label', label);
        button.innerHTML = icon;
        button.addEventListener('pointerdown', event => event.preventDefault());
        return button;
    }

    function create(hwnd, initialText, editSession) {
        const bar = document.createElement('div');
        bar.className = 'refine-bar';
        bar.dataset.refineHwnd = String(hwnd);

        const input = document.createElement('input');
        input.type = 'text';
        input.className = 'refine-input';
        input.placeholder = window.L10N.overlay_refine_placeholder || 'Refine...';
        input.dataset.l10nPlaceholder = 'overlay_refine_placeholder';
        input.autocomplete = 'off';
        input.value = String(initialText || '');
        bar.appendChild(input);

        const microphone = actionButton('refine-action-btn', '', window.iconSvgs.mic);
        microphone.addEventListener('click', () => post('mic', editor.hwnd, undefined, editor.editSession));
        bar.appendChild(microphone);

        const send = actionButton('refine-action-btn send', '', window.iconSvgs.send);
        send.addEventListener('click', () => submit(editor));
        bar.appendChild(send);

        const cancel = actionButton(
            'btn refine-cancel', window.L10N.cancel, window.iconSvgs.close);
        cancel.dataset.l10nTitle = 'cancel';
        cancel.addEventListener('click', () => post('cancel_refine', editor.hwnd, undefined, editor.editSession));
        bar.appendChild(cancel);

        const editor = { hwnd: String(hwnd), editSession: editSession, bar: bar, input: input, composing: false };
        input.addEventListener('pointerdown', () => requestNativeFocus(editor));
        input.addEventListener('input', event => {
            if (!event.isComposing && !editor.composing) publishDraft(editor);
        });
        input.addEventListener('compositionstart', () => { editor.composing = true; });
        input.addEventListener('compositionend', () => {
            editor.composing = false;
            publishDraft(editor);
        });
        input.addEventListener('keydown', event => handleKey(editor, event));
        editors.set(editor.hwnd, editor);
        return editor;
    }

    function removeEditor(hwnd) {
        const key = String(hwnd);
        const wasRequested = requestedFocus?.hwnd === key;
        editors.delete(key);
        if (!wasRequested) return;
        requestedFocus = [...editors.values()].reverse().find(editor => editor.input.isConnected) || null;
        if (requestedFocus) requestNativeFocus(requestedFocus);
    }

    function reconcile(group, hwnd, state) {
        const key = String(hwnd);
        if (!state.isEditing || typeof state.editSession !== 'string' || !state.editSession) {
            removeEditor(key);
            return false;
        }
        let editor = editors.get(key);
        if (!editor || editor.editSession !== state.editSession || !editor.bar.isConnected || editor.bar.parentElement !== group) {
            editor = create(key, state.inputText, state.editSession);
            requestedFocus = editor;
            group.replaceChildren(editor.bar);
            const epoch = window.__SGT_TOPOLOGY_EPOCH__ || '1';
            requestAnimationFrame(() => {
                if (requestedFocus === editor) requestNativeFocus(editor, epoch);
            });
        }
        return true;
    }

    function nativeFocusGranted(hwnd, editSession, epoch) {
        const editor = editors.get(String(hwnd));
        if (!editor || requestedFocus !== editor || editor.editSession !== editSession || !editor.input.isConnected ||
            epoch !== (window.__SGT_TOPOLOGY_EPOCH__ || '1')) return;
        editor.input.focus({ preventScroll: true });
        const end = editor.input.value.length;
        if (editor.input.selectionStart === null) editor.input.setSelectionRange(end, end);
    }

    function settleFocusMode() {
        for (const [key, editor] of editors) {
            if (!editor.input.isConnected) removeEditor(key);
        }
        if (focusMode && editors.size === 0) {
            focusMode = false;
            post('release_refine_focus', '0');
        }
    }

    function setText(hwnd, text, isInsert, editSession) {
        const editor = editors.get(String(hwnd));
        if (!editor || !editor.input.isConnected || typeof editSession !== 'string' ||
            !editSession || editor.editSession !== editSession) return;
        const input = editor.input;
        if (isInsert) {
            const start = input.selectionStart ?? input.value.length;
            const end = input.selectionEnd ?? start;
            input.setRangeText(String(text), start, end, 'end');
        } else {
            input.value = String(text);
            input.setSelectionRange(input.value.length, input.value.length);
        }
        publishDraft(editor);
        requestNativeFocus(editor);
    }

    window.setRefineText = setText;
    window.__SGT_REFINE_EDITOR__ = {
        reconcile: reconcile,
        remove: removeEditor,
        nativeFocusGranted: nativeFocusGranted,
        settleFocusMode: settleFocusMode,
        republishDrafts: () => {
            for (const editor of editors.values()) {
                if (editor.input.isConnected) publishDraft(editor);
            }
            if (editors.size === 0) {
                focusMode = false;
                post('release_refine_focus', '0');
            }
        },
        republishFocus: () => { if (requestedFocus) requestNativeFocus(requestedFocus); },
        activeCount: () => editors.size
    };
})();
"#
}
