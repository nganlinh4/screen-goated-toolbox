//! Refine-input JavaScript appended after the result control runtime.

pub(super) fn get_javascript() -> &'static str {
    r#"
function generateRefineInputHTML(hwnd, state) {
    const micSvg = window.iconSvgs.mic;
    const sendSvg = window.iconSvgs.send;

    return `<div class="refine-bar">
        <input type="text"
               id="input-${hwnd}"
               class="refine-input"
               placeholder="${window.L10N.overlay_refine_placeholder || 'Refine...'}"
               value="${state.inputText || ''}"
               onkeydown="handleRefineKey(event, '${hwnd}')"
               oninput="handleInput(event, '${hwnd}')"
               onfocus="ensureNativeFocus('${hwnd}');"
               onclick="ensureNativeFocus('${hwnd}');"
               autofocus
               autocomplete="off">
        <div class="refine-action-btn"
             onmousedown="event.preventDefault();"
             onclick="action('${hwnd}', 'mic')">
            ${micSvg}
        </div>
        <div class="refine-action-btn send" onclick="submitRefine('${hwnd}')">
            ${sendSvg}
        </div>
        <div class="btn" style="width:24px;height:24px;border:none;background:transparent;box-shadow:none;cursor:pointer;display:flex;align-items:center;justify-content:center;"
            onclick="action('${hwnd}', 'cancel_refine')"
            title="${window.L10N.cancel}">
            <span style="font-size:14px;color:var(--refine-placeholder);pointer-events:none;">✕</span>
        </div>
    </div>`;
}

let focusedInput = null;
let selectionStart = 0;
let selectionEnd = 0;
let inputValues = new Map();

function ensureNativeFocus(hwnd) {
    window.focus();
    window.ipc.postMessage(JSON.stringify({ action: "request_focus", hwnd: hwnd }));
}

function handleInput(e, hwnd) {
    ensureNativeFocus(hwnd);
    inputValues.set(hwnd, e.target.value);
}

function handleRefineKey(e, hwnd) {
    ensureNativeFocus(hwnd);
    if (e.key === 'Enter') {
        e.preventDefault();
        submitRefine(hwnd);
    } else if (e.key === 'Escape') {
        e.preventDefault();
        action(hwnd, 'cancel_refine');
    } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const val = inputValues.get(hwnd) || '';
        window.ipc.postMessage(JSON.stringify({
            action: 'history_up_refine',
            hwnd: hwnd,
            text: val
        }));
    } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const val = inputValues.get(hwnd) || '';
        window.ipc.postMessage(JSON.stringify({
            action: 'history_down_refine',
            hwnd: hwnd,
            text: val
        }));
    }

    focusedInput = e.target.id;
    selectionStart = e.target.selectionStart;
    selectionEnd = e.target.selectionEnd;
}

function submitRefine(hwnd) {
    const inputId = 'input-' + hwnd;
    const el = document.getElementById(inputId);
    const text = el ? el.value : (inputValues.get(hwnd) || '');
    if (text && text.trim().length > 0) {
        window.ipc.postMessage(JSON.stringify({
            action: 'submit_refine',
            hwnd: hwnd,
            text: text
        }));
        inputValues.delete(hwnd);
    }
}

window.setRefineText = (hwnd, text, isInsert) => {
    const inputId = 'input-' + hwnd;
    const el = document.getElementById(inputId);
    if (el) {
        if (isInsert) {
            const start = el.selectionStart;
            const end = el.selectionEnd;
            const val = el.value;
            el.value = val.substring(0, start) + text + val.substring(end);
            el.selectionStart = el.selectionEnd = start + text.length;
        } else {
            el.value = text;
        }
        inputValues.set(hwnd, el.value);
        el.focus();
    }
};

const originalUpdateWindows = window.updateWindows;
window.updateWindows = function(data) {
    const activeEl = document.activeElement;
    if (activeEl && activeEl.tagName === 'INPUT') {
        focusedInput = activeEl.id;
        selectionStart = activeEl.selectionStart;
        selectionEnd = activeEl.selectionEnd;
    }

    originalUpdateWindows(data);

    let focusedFound = false;
    if (focusedInput) {
        const el = document.getElementById(focusedInput);
        if (el) {
            el.focus();
            focusedFound = true;
            const trackingHwnd = focusedInput.replace('input-', '');
            if (inputValues.has(trackingHwnd)) {
                el.value = inputValues.get(trackingHwnd);
            }
            try {
                el.setSelectionRange(selectionStart, selectionEnd);
            } catch(e) {}
        }
    }

    if (!focusedFound) {
        const editBars = document.querySelectorAll('.refine-input');
        if (editBars.length > 0) {
            editBars[0].focus();
        }
    }
};
"#
}
