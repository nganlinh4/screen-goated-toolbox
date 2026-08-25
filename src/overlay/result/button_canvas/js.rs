//! JavaScript for the result compositor's control layer.

pub fn get_javascript() -> &'static str {
    r#"
window.registeredWindows = {}; window.L10N = #L10N_JSON#;
window.iconSvgs = #ICON_SVGS_JSON#;
let lastVisibleState = new Map();
let lastSentRegions = new Map();
let highestButtonStackOrder = 0;
let cursorX = 0, cursorY = 0;
const activeGrabbingSources = new Set();
let activeResultDragPreview = null;
window.raiseWindowButtons = function(hwnd) {
    const group = document.querySelector('.button-group[data-hwnd="' + hwnd + '"]');
    if (!group) return;
    highestButtonStackOrder += 1;
    group.style.zIndex = String(highestButtonStackOrder);
};
window.setWindowButtonStackOrder = function(hwnd, stackOrder) {
    const group = document.querySelector('.button-group[data-hwnd="' + hwnd + '"]');
    if (!group) return;
    const order = Number(stackOrder || 0);
    highestButtonStackOrder = Math.max(highestButtonStackOrder, order);
    group.style.zIndex = String(order);
};

function contrastingControlSurface(color) {
    const match = /^#([0-9a-f]{6})$/i.exec(String(color || '').trim());
    if (!match) return '';
    const value = parseInt(match[1], 16);
    const channels = [(value >> 16) & 255, (value >> 8) & 255, value & 255]
        .map(channel => {
            const normalized = channel / 255;
            return normalized <= 0.04045
                ? normalized / 12.92
                : Math.pow((normalized + 0.055) / 1.055, 2.4);
        });
    const luminance = 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
    return luminance <= 0.179 ? 'light' : 'dark';
}

function setResultDraggingCursor(active) {
    if (active) {
        activeGrabbingSources.add("result-handle");
    } else {
        activeGrabbingSources.delete("result-handle");
    }
    applyGrabbingCursorState();
}
function setOpacityDraggingCursor(active) {
    if (active) {
        activeGrabbingSources.add("opacity");
    } else {
        activeGrabbingSources.delete("opacity");
    }
    applyGrabbingCursorState();
}
function applyGrabbingCursorState() {
    const html = document.documentElement;
    if (!html) return;

    if (activeGrabbingSources.size > 0) {
        html.style.cursor = "grabbing";
        if (document.body) document.body.style.cursor = "grabbing";
    } else {
        html.style.cursor = "";
        if (document.body) document.body.style.cursor = "";
    }
}

window.setResultDraggingCursor = setResultDraggingCursor;
window.updateOpacity = function(hwnd, value) {
    value = parseInt(value);
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
    let needsUpdate = false;

    groups.forEach(group => {
        const rect = group.getBoundingClientRect();

        let dx = 0, dy = 0;
        if (cursorX < rect.left) dx = rect.left - cursorX;
        else if (cursorX > rect.right) dx = cursorX - rect.right;

        if (cursorY < rect.top) dy = rect.top - cursorY;
        else if (cursorY > rect.bottom) dy = cursorY - rect.bottom;

        const dist = Math.sqrt(dx * dx + dy * dy);

        const maxRadius = 150;
        const proximityOpacity = group.classList.contains('proximity-pinned')
            ? 1 : Math.max(0, Math.min(1, 1 - (dist / maxRadius)));
        const pulseOpacity = Math.max(0, Math.min(1, Number(group.dataset.pulseOpacity || 0)));
        const opacity = Math.max(proximityOpacity, pulseOpacity);

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
            scale: window.devicePixelRatio || 1,
            regions: regions
        }));
    }
}

function calculateButtonPosition(winRect, controlScale) {
    const screenW = window.innerWidth;
    const screenH = window.innerHeight;
    const longDim = 300 * controlScale;
    const shortDim = 32 * controlScale;
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

function escapeText(value) {
    return String(value).replace(/[&<>]/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[ch]);
}

function escapeAttribute(value) {
    return escapeText(value).replace(/"/g, '&quot;');
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

    if (!state.groupActions) {
        const backHideClass = canGoBack ? '' : 'hidden';
        buttons += `<div class="btn ${backHideClass}" onclick="action('${hwnd}', 'back')" title="${window.L10N.back}">
            ${window.iconSvgs.arrow_back}
        </div>`;

        const forwardHideClass = canGoForward ? '' : 'hidden';
        buttons += `<div class="btn ${forwardHideClass}" onclick="action('${hwnd}', 'forward')" title="${window.L10N.forward}">
            ${window.iconSvgs.arrow_forward}
        </div>`;
    }

    if (!isBrowsing && !state.groupActions && state.modelLabel) {
        buttons += `<div class="model-badge" title="${escapeAttribute(state.modelLabel)}">${escapeText(state.modelLabel)}</div>`;
    }

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

    if (!state.groupActions && state.hasUndo) {
        buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'undo')" title="${window.L10N.undo}">
            ${window.iconSvgs.undo}
        </div>`;
    }

    if (!state.groupActions && state.hasRedo) {
        buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'redo')" title="${window.L10N.redo}">
            ${window.iconSvgs.redo}
        </div>`;
    }

    if (state.editEnabled !== false) buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'edit')" title="${window.L10N.edit}">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 258" width="14" height="14" style="fill: currentColor; stroke: currentColor; stroke-width: 20; stroke-linejoin: round; opacity: 0.9;">
            <path d="m122.062 172.77l-10.27 23.52c-3.947 9.042-16.459 9.042-20.406 0l-10.27-23.52c-9.14-20.933-25.59-37.595-46.108-46.703L6.74 113.52c-8.987-3.99-8.987-17.064 0-21.053l27.385-12.156C55.172 70.97 71.917 53.69 80.9 32.043L91.303 6.977c3.86-9.303 16.712-9.303 20.573 0l10.403 25.066c8.983 21.646 25.728 38.926 46.775 48.268l27.384 12.156c8.987 3.99 8.987 17.063 0 21.053l-28.267 12.547c-20.52 9.108-36.97 25.77-46.109 46.703"/>
            <path d="m217.5 246.937l-2.888 6.62c-2.114 4.845-8.824 4.845-10.937 0l-2.889-6.62c-5.148-11.803-14.42-21.2-25.992-26.34l-8.898-3.954c-4.811-2.137-4.811-9.131 0-11.269l8.4-3.733c11.87-5.273 21.308-15.017 26.368-27.22l2.966-7.154c2.067-4.985 8.96-4.985 11.027 0l2.966 7.153c5.06 12.204 14.499 21.948 26.368 27.221l8.4 3.733c4.812 2.138 4.812 9.132 0 11.27l-8.898 3.953c-11.571 5.14-20.844 14.537-25.992 26.34"/>
        </svg>
    </div>`;

    buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'download')" title="${window.L10N.download}">
        ${window.iconSvgs.download}
    </div>`;

    const speakerIcon = state.ttsLoading ? 'hourglass_empty' : (state.ttsSpeaking ? 'stop' : 'volume_up');
    const speakerClass = state.ttsLoading ? 'loading' : (state.ttsSpeaking ? 'active' : '');
    buttons += `<div class="btn ${speakerClass} ${hideClass}" onclick="action('${hwnd}', 'speaker')" title="${window.L10N.speaker}">
        ${window.iconSvgs[speakerIcon]}
    </div>`;

    buttons += `<div class="btn result-handle"
        onpointerdown="handleResultDrag(event, '${hwnd}', ${Boolean(state.groupActions)})"
        oncontextmenu="return false;"
        title="${window.L10N.result_handle}">
        ${window.iconSvgs.cleaning_services}
    </div>`;

    return buttons;
}

function handleResultDrag(e, hwnd, groupActions) {
    if (e.button !== 0 && e.button !== 1 && e.button !== 2) return;
    if (activeResultDragPreview) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    setResultDraggingCursor(true);
    const root = window.registeredWindows[String(hwnd)];
    const groupIds = root?.state?.groupIds?.map(String) || [String(hwnd)];
    const allIds = [...new Set(Object.entries(window.registeredWindows).flatMap(([id, model]) =>
        model?.state?.groupIds?.map(String) || [id]))];
    const targets = e.button === 1 ? allIds
        : ((groupActions || e.button === 2) ? groupIds : [String(hwnd)]);
    const nativeTargets = targets.filter(id =>
        Boolean(window.registeredWindows[id]?.state?.isBrowsing));
    const cardOrigins = new Map();
    for (const id of targets) {
        const card = document.querySelector('.result-card[data-id="' + id + '"]');
        if (!card) continue;
        const rect = card.getBoundingClientRect();
        cardOrigins.set(id, { x: rect.left, y: rect.top });
    }
    activeResultDragPreview = {
        hwnd: String(hwnd), targets: targets, pointerId: e.pointerId,
        startX: e.clientX, startY: e.clientY, dx: 0, dy: 0, frame: 0,
        cardOrigins: cardOrigins, nativeTargets: nativeTargets
    };

    let action = 'result_drag_start';
    if (e.button === 0 && groupActions) action = 'result_group_drag_start';
    else if (e.button === 1) action = 'result_all_drag_start';
    else if (e.button === 2) action = 'result_group_drag_start';

    window.ipc.postMessage(JSON.stringify({
        action: action,
        hwnd: hwnd
    }));
    window.__SGT_BUTTON_SCENE__?.setDragActive(true);
}

function renderResultDragPreview() {
    if (!activeResultDragPreview) return;
    activeResultDragPreview.frame = 0;
    const dx = activeResultDragPreview.dx;
    const dy = activeResultDragPreview.dy;
    const offset = dx + 'px ' + dy + 'px';
    for (const id of activeResultDragPreview.targets) {
        const card = document.querySelector('.result-card[data-id="' + id + '"]');
        const origin = activeResultDragPreview.cardOrigins.get(id);
        if (card && origin) {
            card.style.transform = 'translate3d(' + (origin.x + dx) + 'px,' +
                (origin.y + dy) + 'px,0)';
        }
        const group = document.querySelector('.button-group[data-hwnd="' + id + '"]');
        if (group) group.style.translate = offset;
    }
    if (activeResultDragPreview.nativeTargets.length) {
        const scale = window.devicePixelRatio || 1;
        window.ipc.postMessage(JSON.stringify({
            action: 'result_drag_preview', hwnd: activeResultDragPreview.hwnd,
            dx: Math.round(dx * scale), dy: Math.round(dy * scale)
        }));
    }
}
function queueResultDragPreview(event) {
    const drag = activeResultDragPreview;
    if (!drag || event.pointerId !== drag.pointerId) return;
    drag.dx = event.clientX - drag.startX;
    drag.dy = event.clientY - drag.startY;
    if (!drag.frame) drag.frame = requestAnimationFrame(renderResultDragPreview);
    event.preventDefault();
}
function finishLocalResultDrag(event) {
    const drag = activeResultDragPreview;
    if (!drag || (event && event.pointerId !== drag.pointerId)) return;
    if (event) {
        drag.dx = event.clientX - drag.startX;
        drag.dy = event.clientY - drag.startY;
    }
    if (drag.frame) cancelAnimationFrame(drag.frame);
    renderResultDragPreview();
    const scale = window.devicePixelRatio || 1;
    window.ipc.postMessage(JSON.stringify({
        action: 'result_drag_finish', hwnd: drag.hwnd,
        dx: Math.round(drag.dx * scale), dy: Math.round(drag.dy * scale)
    }));
    window.__SGT_BUTTON_SCENE__?.releaseDragPreview(
        event ? event.clientX : undefined,
        event ? event.clientY : undefined);
    activeResultDragPreview = null;
    setResultDraggingCursor(false);
}
document.addEventListener('pointermove', queueResultDragPreview, true);
document.addEventListener('pointerup', finishLocalResultDrag, true);
document.addEventListener('pointercancel', finishLocalResultDrag, true);
window.clearResultDragControlPreview = function() {
    document.querySelectorAll('.button-group').forEach(group => { group.style.translate = ''; });
};

window.addEventListener("blur", () => finishLocalResultDrag(null));
document.addEventListener("visibilitychange", () => {
    if (document.hidden) finishLocalResultDrag(null);
});
window.addEventListener("pointerup", () => setOpacityDraggingCursor(false));
window.addEventListener("blur", () => setOpacityDraggingCursor(false));
document.addEventListener("visibilitychange", () => {
    if (document.hidden) setOpacityDraggingCursor(false);
});
document.addEventListener("pointerdown", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const group = target.closest(".button-group[data-hwnd]");
    if (group) {
        window.raiseWindowButtons(group.dataset.hwnd);
        window.ipc.postMessage(JSON.stringify({
            action: "interact",
            hwnd: group.dataset.hwnd
        }));
    }
    if (target.closest(".opacity-slider-inline")) {
        setOpacityDraggingCursor(true);
    }
});

function action(hwnd, cmd) {
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
        const state = data.state || {};
        const rawAnchor = state.controlAnchor;
        const deviceScale = window.devicePixelRatio || 1;
        const placementRect = Array.isArray(rawAnchor) && rawAnchor.length === 4
            ? { x: rawAnchor[0] / deviceScale, y: rawAnchor[1] / deviceScale,
                w: rawAnchor[2] / deviceScale, h: rawAnchor[3] / deviceScale }
            : data.rect;
        const controlScale = Math.max(0.5, Math.min(3, Number(state.controlScalePercent || 100) / 100));
        let pos = calculateButtonPosition(placementRect, controlScale);
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

        const { opacityPercent, ...structuralState } = state;
        const isVertical = pos.direction === 'left' || pos.direction === 'right';
        const newStateStr = JSON.stringify(structuralState) + isVertical;
        if (group.dataset.lastState !== newStateStr) {
            group.innerHTML = generateButtonsHTML(hwnd, state, isVertical);
            group.dataset.lastState = newStateStr;
        }
        const opacity = group.querySelector('.opacity-slider-inline');
        const opacityLabel = group.querySelector('.opacity-value-inline');
        if (opacity && opacityPercent != null) {
            opacity.value = opacityPercent;
            if (opacityLabel) opacityLabel.textContent = opacityPercent + '%';
        }
        if (state.controlColor) {
            group.style.setProperty('--chain-control-color', state.controlColor);
        } else {
            group.style.removeProperty('--chain-control-color');
        }
        const localSurface = contrastingControlSurface(state.controlColor);
        group.classList.toggle('local-control-surface-light', localSurface === 'light');
        group.classList.toggle('local-control-surface-dark', localSurface === 'dark');
        group.style.setProperty('--control-scale', String(controlScale));
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
            finalX = placementRect.x + placementRect.w - actualW;
            finalY = placementRect.y + placementRect.h + 4;
        } else if (pos.direction === 'top') {
            finalX = placementRect.x + (placementRect.w - actualW) / 2;
            finalY = placementRect.y - actualH - 4;
        } else if (pos.direction === 'right') {
            finalX = placementRect.x + placementRect.w + 4;
            finalY = placementRect.y + (placementRect.h - actualH) / 2;
        } else if (pos.direction === 'left') {
            finalX = placementRect.x - actualW - 4;
            finalY = placementRect.y + (placementRect.h - actualH) / 2;
        } else {
            finalX = placementRect.x + placementRect.w - actualW - 8;
            finalY = placementRect.y + placementRect.h - actualH - 8;
            finalX = Math.max(placementRect.x, finalX);
            finalY = Math.max(placementRect.y, finalY);
        }
        const clamp = (val, size, max) => Math.max(0, Math.min(val, max - size));

        finalX = clamp(finalX, actualW, screenW);
        finalY = clamp(finalY, actualH, screenH);

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

    existingGroups.forEach((el, key) => {
        el.remove();
        lastVisibleState.delete(key);
        lastSentRegions.delete(key);
    });

    updateButtonOpacity();
    if (container.querySelectorAll('.button-group').length === 0) {
        window.ipc.postMessage(JSON.stringify({
            action: "update_clickable_regions", scale: window.devicePixelRatio || 1, regions: []
        }));
    }
}

window.updateWindows = updateWindows;
window.updateButtonOpacity = updateButtonOpacity;
"#
}
