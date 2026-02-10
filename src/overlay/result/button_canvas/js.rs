//! JavaScript code for button canvas WebView

pub fn get_javascript() -> &'static str {
    r#"
// Track registered windows: { hwnd: { x, y, w, h } }
window.registeredWindows = {};
window.L10N = #L10N_JSON#;
window.iconSvgs = #ICON_SVGS_JSON#;

// Track visibility state to minimize IPC calls
let lastVisibleState = new Map();
let lastSentRegions = new Map();

// Track cursor position for radius-based opacity
let cursorX = 0, cursorY = 0;
let broomDragData = null;

// Opacity slider state
window.opacityValues = {};

function setBroomDraggingCursor(active) {
    const html = document.documentElement;
    if (!html) return;

    if (active) {
        html.style.cursor = "grabbing";
        if (document.body) document.body.style.cursor = "grabbing";
    } else {
        html.style.cursor = "";
        if (document.body) document.body.style.cursor = "";
    }
}

window.setBroomDraggingCursor = setBroomDraggingCursor;

window.updateOpacity = function(hwnd, value) {
    value = parseInt(value);
    window.opacityValues[hwnd] = value;
    window.ipc.postMessage(JSON.stringify({
        action: "set_opacity",
        hwnd: hwnd,
        value: value
    }));

    const group = document.querySelector('.button-group[data-hwnd="' + hwnd + '"]');
    if (group) {
        const span = group.querySelector('.opacity-value-inline');
        if (span) span.textContent = value + '%';
    }
};

window.updateCursorPosition = (x, y) => {
    cursorX = x;
    cursorY = y;
    updateButtonOpacity();
};

function updateButtonOpacity() {
    const groups = document.querySelectorAll('.button-group');
    let needsUpdate = (broomDragData && broomDragData.moved) || false;

    groups.forEach(group => {
        const rect = group.getBoundingClientRect();

        let dx = 0, dy = 0;
        if (cursorX < rect.left) dx = rect.left - cursorX;
        else if (cursorX > rect.right) dx = cursorX - rect.right;

        if (cursorY < rect.top) dy = rect.top - cursorY;
        else if (cursorY > rect.bottom) dy = cursorY - rect.bottom;

        const dist = Math.sqrt(dx * dx + dy * dy);

        const maxRadius = 150;
        let opacity = Math.max(0, Math.min(1, 1 - (dist / maxRadius)));

        if (broomDragData && broomDragData.moved && broomDragData.hwnd === group.dataset.hwnd) {
            opacity = 1.0;
        }

        group.style.opacity = opacity;

        const isVisible = opacity > 0.1;
        group.style.pointerEvents = isVisible ? 'auto' : 'none';

        const hwnd = group.dataset.hwnd;
        if (lastVisibleState.get(hwnd) !== isVisible) {
            lastVisibleState.set(hwnd, isVisible);
            needsUpdate = true;
        }

        if (isVisible) {
            const currentRegion = {
                x: Math.round(rect.left),
                y: Math.round(rect.top),
                w: Math.round(rect.width),
                h: Math.round(rect.height)
            };
            const regionStr = JSON.stringify(currentRegion);
            if (lastSentRegions.get(hwnd) !== regionStr) {
                needsUpdate = true;
            }
        }
    });

    if (needsUpdate) {
        const regions = [];
        const padding = 5;

        groups.forEach(group => {
            if (lastVisibleState.get(group.dataset.hwnd)) {
                const rect = group.getBoundingClientRect();
                const isVertical = group.classList.contains('vertical');
                let region;

                if (isVertical) {
                    region = {
                        x: rect.left + 1,
                        y: rect.top - 200,
                        w: rect.width + padding,
                        h: rect.height + 200 + padding
                    };
                } else {
                    region = {
                        x: rect.left - 200,
                        y: rect.top + 1,
                        w: rect.width + 200 + padding,
                        h: rect.height + padding
                    };
                }
                regions.push(region);

                const rawRegion = {
                    x: Math.round(rect.left),
                    y: Math.round(rect.top),
                    w: Math.round(rect.width),
                    h: Math.round(rect.height)
                };
                lastSentRegions.set(group.dataset.hwnd, JSON.stringify(rawRegion));
            }
        });

        window.ipc.postMessage(JSON.stringify({
            action: "update_clickable_regions",
            regions: regions
        }));
    }
}

function calculateButtonPosition(winRect) {
    const screenW = window.innerWidth;
    const screenH = window.innerHeight;
    const longDim = 300;
    const shortDim = 32;
    const margin = 4;

    const spaceBottom = screenH - (winRect.y + winRect.h);
    const spaceTop = winRect.y;
    const spaceRight = screenW - (winRect.x + winRect.w);
    const spaceLeft = winRect.x;

    const clamp = (val, max) => Math.max(0, Math.min(val, max));

    if (spaceBottom >= shortDim + margin) {
        let x = winRect.x + winRect.w - longDim;
        x = clamp(x, screenW - longDim);
        return { x: x, y: winRect.y + winRect.h + margin, direction: 'bottom' };
    }
    else if (spaceRight >= shortDim + margin) {
        let y = winRect.y + (winRect.h - longDim) / 2;
        y = clamp(y, screenH - longDim);
        return { x: winRect.x + winRect.w + margin, y: y, direction: 'right' };
    }
    else if (spaceLeft >= shortDim + margin) {
        let y = winRect.y + (winRect.h - longDim) / 2;
        y = clamp(y, screenH - longDim);
        return { x: winRect.x - shortDim - margin, y: y, direction: 'left' };
    }
    else if (spaceTop >= shortDim + margin) {
        let x = winRect.x + (winRect.w - longDim) / 2;
        x = clamp(x, screenW - longDim);
        return { x: x, y: winRect.y - shortDim - margin, direction: 'top' };
    }
    else {
        let x = winRect.x + (winRect.w - longDim) / 2;
        x = clamp(x, screenW - longDim);
        let y = winRect.y + winRect.h - shortDim - margin;
        y = Math.max(winRect.y, y);
        return { x: x, y: y, direction: 'inside' };
    }
}

function generateButtonsHTML(hwnd, state, isVertical) {
    const canGoBack = state.navDepth > 0;
    const canGoForward = state.navDepth < state.maxNavDepth;
    const isBrowsing = state.isBrowsing || false;
    const hideClass = isBrowsing ? 'hidden' : '';

    if (state.isEditing) {
        return generateRefineInputHTML(hwnd, state);
    }

    let buttons = '';

    const backHideClass = canGoBack ? '' : 'hidden';
    buttons += `<div class="btn ${backHideClass}" onclick="action('${hwnd}', 'back')" title="${window.L10N.back}">
        ${window.iconSvgs.arrow_back}
    </div>`;

    const forwardHideClass = canGoForward ? '' : 'hidden';
    buttons += `<div class="btn ${forwardHideClass}" onclick="action('${hwnd}', 'forward')" title="${window.L10N.forward}">
        ${window.iconSvgs.arrow_forward}
    </div>`;

    const opacityValue = state.opacityPercent || 100;
    const verticalClass = isVertical ? 'vertical-slider' : '';
    buttons += `<div class="btn opacity-btn-expandable ${verticalClass} ${hideClass}" title="${window.L10N.opacity}">
        <div class="opacity-slider-wrapper">
            <input type="range" class="opacity-slider-inline" min="10" max="100" value="${opacityValue}"
                oninput="updateOpacity('${hwnd}', this.value)" />
            <span class="opacity-value-inline">${opacityValue}%</span>
        </div>
        <div class="opacity-icon-wrapper">
            ${window.iconSvgs.opacity}
        </div>
    </div>`;

    buttons += `<div class="btn ${state.copySuccess ? 'success' : ''} ${hideClass}" onclick="action('${hwnd}', 'copy')" title="${window.L10N.copy}">
        ${window.iconSvgs[state.copySuccess ? 'check' : 'content_copy']}
    </div>`;

    if (state.hasUndo) {
        buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'undo')" title="${window.L10N.undo}">
            ${window.iconSvgs.undo}
        </div>`;
    }

    if (state.hasRedo) {
        buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'redo')" title="${window.L10N.redo}">
            ${window.iconSvgs.redo}
        </div>`;
    }

    buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'edit')" title="${window.L10N.edit}">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 258" width="14" height="14" style="fill: currentColor; stroke: currentColor; stroke-width: 20; stroke-linejoin: round; opacity: 0.9;">
            <path d="m122.062 172.77l-10.27 23.52c-3.947 9.042-16.459 9.042-20.406 0l-10.27-23.52c-9.14-20.933-25.59-37.595-46.108-46.703L6.74 113.52c-8.987-3.99-8.987-17.064 0-21.053l27.385-12.156C55.172 70.97 71.917 53.69 80.9 32.043L91.303 6.977c3.86-9.303 16.712-9.303 20.573 0l10.403 25.066c8.983 21.646 25.728 38.926 46.775 48.268l27.384 12.156c8.987 3.99 8.987 17.063 0 21.053l-28.267 12.547c-20.52 9.108-36.97 25.77-46.109 46.703"/>
            <path d="m217.5 246.937l-2.888 6.62c-2.114 4.845-8.824 4.845-10.937 0l-2.889-6.62c-5.148-11.803-14.42-21.2-25.992-26.34l-8.898-3.954c-4.811-2.137-4.811-9.131 0-11.269l8.4-3.733c11.87-5.273 21.308-15.017 26.368-27.22l2.966-7.154c2.067-4.985 8.96-4.985 11.027 0l2.966 7.153c5.06 12.204 14.499 21.948 26.368 27.221l8.4 3.733c4.812 2.138 4.812 9.132 0 11.27l-8.898 3.953c-11.571 5.14-20.844 14.537-25.992 26.34"/>
        </svg>
    </div>`;

    const mdClass = state.isMarkdown ? 'active' : '';
    const mdIcon = state.isMarkdown ? 'newsmode' : 'notes';
    buttons += `<div class="btn ${mdClass} ${hideClass}" onclick="action('${hwnd}', 'markdown')" title="${window.L10N.markdown}">
        ${window.iconSvgs[mdIcon]}
    </div>`;

    buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'download')" title="${window.L10N.download}">
        ${window.iconSvgs.download}
    </div>`;

    const speakerIcon = state.ttsLoading ? 'hourglass_empty' : (state.ttsSpeaking ? 'stop' : 'volume_up');
    const speakerClass = state.ttsLoading ? 'loading' : (state.ttsSpeaking ? 'active' : '');
    buttons += `<div class="btn ${speakerClass} ${hideClass}" onclick="action('${hwnd}', 'speaker')" title="${window.L10N.speaker}">
        ${window.iconSvgs[speakerIcon]}
    </div>`;

    buttons += `<div class="btn broom"
        onmousedown="handleBroomDrag(event, '${hwnd}')"
        oncontextmenu="return false;"
        title="${window.L10N.broom}">
        ${window.iconSvgs.cleaning_services}
    </div>`;

    return buttons;
}

function handleBroomDrag(e, hwnd) {
    if (e.button !== 0 && e.button !== 1 && e.button !== 2) return;
    setBroomDraggingCursor(true);

    const group = document.querySelector('.button-group[data-hwnd="' + hwnd + '"]');
    if (group) {
        group.style.opacity = '0';
        group.style.pointerEvents = 'none';
        lastVisibleState.set(hwnd, false);
    }

    let action = 'broom_drag_start';
    if (e.button === 1) action = 'broom_all_drag_start';
    else if (e.button === 2) action = 'broom_group_drag_start';

    window.ipc.postMessage(JSON.stringify({
        action: action,
        hwnd: hwnd
    }));
}

window.addEventListener("mouseup", () => setBroomDraggingCursor(false));
window.addEventListener("blur", () => setBroomDraggingCursor(false));
document.addEventListener("visibilitychange", () => {
    if (document.hidden) setBroomDraggingCursor(false);
});

function action(hwnd, cmd) {
    if (cmd === 'broom_click' && window.ignoreNextBroomClick) return;
    window.ipc.postMessage(JSON.stringify({ action: cmd, hwnd: hwnd }));
}

function updateWindows(windowsData) {
    window.registeredWindows = windowsData;

    const container = document.getElementById('button-container');
    const screenW = window.innerWidth;
    const screenH = window.innerHeight;

    const existingGroups = new Map();
    container.querySelectorAll('.button-group').forEach(el => {
        existingGroups.set(el.dataset.hwnd, el);
    });

    for (const [hwnd, data] of Object.entries(windowsData)) {
        let pos = calculateButtonPosition(data.rect);
        let group = existingGroups.get(hwnd);

        if (!group) {
            group = document.createElement('div');
            group.className = 'button-group';
            group.style.opacity = '0';
            group.dataset.hwnd = hwnd;
            container.appendChild(group);
        } else {
            existingGroups.delete(hwnd);
        }

        const isVertical = pos.direction === 'left' || pos.direction === 'right';
        const newStateStr = JSON.stringify(data.state || {}) + isVertical;
        if (group.dataset.lastState !== newStateStr) {
            group.innerHTML = generateButtonsHTML(hwnd, data.state || {}, isVertical);
            group.dataset.lastState = newStateStr;
        }

        if (isVertical) {
            group.classList.add('vertical');
        } else {
            group.classList.remove('vertical');
        }

        const actualW = group.offsetWidth || (isVertical ? 50 : 400);
        const actualH = group.offsetHeight || (isVertical ? 400 : 50);

        let finalX = pos.x;
        let finalY = pos.y;

        if (pos.direction === 'bottom') {
            finalX = data.rect.x + data.rect.w - actualW;
            finalY = data.rect.y + data.rect.h + 4;
        } else if (pos.direction === 'top') {
            finalX = data.rect.x + (data.rect.w - actualW) / 2;
            finalY = data.rect.y - actualH - 4;
        } else if (pos.direction === 'right') {
            finalX = data.rect.x + data.rect.w + 4;
            finalY = data.rect.y + (data.rect.h - actualH) / 2;
        } else if (pos.direction === 'left') {
            finalX = data.rect.x - actualW - 4;
            finalY = data.rect.y + (data.rect.h - actualH) / 2;
        } else {
            finalX = data.rect.x + 8;
            finalY = data.rect.y + data.rect.h - actualH - 8;
            finalY = Math.max(data.rect.y, finalY);
        }

        const clamp = (val, size, max) => Math.max(0, Math.min(val, max - size));

        finalX = clamp(finalX, actualW, screenW);
        finalY = clamp(finalY, actualH, screenH);

        if (!broomDragData || broomDragData.hwnd !== hwnd) {
            if (pos.direction === 'bottom' || pos.direction === 'right') {
                group.style.left = 'auto';
                group.style.right = (screenW - (finalX + actualW)) + 'px';
            } else {
                group.style.left = finalX + 'px';
                group.style.right = 'auto';
            }

            if (isVertical) {
                group.style.top = 'auto';
                group.style.bottom = (screenH - (finalY + actualH)) + 'px';
            } else {
                group.style.top = finalY + 'px';
                group.style.bottom = 'auto';
            }
        }
    }

    existingGroups.forEach((el, key) => {
        el.remove();
        lastVisibleState.delete(key);
    });

    updateButtonOpacity();
}

window.updateWindows = updateWindows;

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
            title="Cancel">
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
