//! Persistent refinement editor runtime for the shared result scene compositor.

pub(super) fn get_javascript() -> &'static str {
    r#"
(function() {
    const editors = new Map();
    let focusMode = false;

    function post(action, hwnd, text) {
        const message = { action: action, hwnd: String(hwnd) };
        if (text !== undefined) message.text = text;
        window.ipc.postMessage(JSON.stringify(message));
    }

    function requestNativeFocus(hwnd) {
        focusMode = true;
        post('request_refine_focus', hwnd);
    }

    function publishDraft(editor) {
        post('update_refine_draft', editor.hwnd, editor.input.value);
    }

    function submit(editor) {
        const text = editor.input.value;
        if (text.trim()) post('submit_refine', editor.hwnd, text);
    }

    function handleKey(editor, event) {
        if (editor.composing || event.isComposing) return;
        if (event.key === 'Enter') {
            event.preventDefault();
            submit(editor);
        } else if (event.key === 'Escape') {
            event.preventDefault();
            post('cancel_refine', editor.hwnd);
        } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
            event.preventDefault();
            post(event.key === 'ArrowUp' ? 'history_up_refine' : 'history_down_refine',
                editor.hwnd, editor.input.value);
        }
    }

    function actionButton(className, title, icon) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = className;
        button.title = title || '';
        button.innerHTML = icon;
        button.addEventListener('pointerdown', event => event.preventDefault());
        return button;
    }

    function create(hwnd, initialText) {
        const bar = document.createElement('div');
        bar.className = 'refine-bar';
        bar.dataset.refineHwnd = String(hwnd);

        const input = document.createElement('input');
        input.type = 'text';
        input.className = 'refine-input';
        input.placeholder = window.L10N.overlay_refine_placeholder || 'Refine...';
        input.autocomplete = 'off';
        input.value = String(initialText || '');
        bar.appendChild(input);

        const microphone = actionButton('refine-action-btn', '', window.iconSvgs.mic);
        microphone.addEventListener('click', () => action(String(hwnd), 'mic'));
        bar.appendChild(microphone);

        const send = actionButton('refine-action-btn send', '', window.iconSvgs.send);
        send.addEventListener('click', () => submit(editor));
        bar.appendChild(send);

        const cancel = actionButton(
            'btn refine-cancel', window.L10N.cancel, window.iconSvgs.close);
        cancel.addEventListener('click', () => post('cancel_refine', hwnd));
        bar.appendChild(cancel);

        const editor = { hwnd: String(hwnd), bar: bar, input: input, composing: false };
        input.addEventListener('pointerdown', () => requestNativeFocus(editor.hwnd));
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

    function reconcile(group, hwnd, state) {
        const key = String(hwnd);
        if (!state.isEditing) {
            editors.delete(key);
            return false;
        }
        let editor = editors.get(key);
        if (!editor || !editor.bar.isConnected || editor.bar.parentElement !== group) {
            editor = create(key, state.inputText);
            group.replaceChildren(editor.bar);
            requestAnimationFrame(() => requestNativeFocus(key));
        }
        return true;
    }

    function nativeFocusGranted(hwnd) {
        const editor = editors.get(String(hwnd));
        if (!editor || !editor.input.isConnected) return;
        editor.input.focus({ preventScroll: true });
        const end = editor.input.value.length;
        if (editor.input.selectionStart === null) editor.input.setSelectionRange(end, end);
    }

    function settleFocusMode() {
        if (focusMode && editors.size === 0) {
            focusMode = false;
            post('release_refine_focus', '0');
        }
    }

    function setText(hwnd, text, isInsert) {
        const editor = editors.get(String(hwnd));
        if (!editor || !editor.input.isConnected) return;
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
        requestNativeFocus(editor.hwnd);
    }

    window.setRefineText = setText;
    window.__SGT_REFINE_EDITOR__ = {
        reconcile: reconcile,
        nativeFocusGranted: nativeFocusGranted,
        settleFocusMode: settleFocusMode,
        activeCount: () => editors.size
    };
})();
"#
}
