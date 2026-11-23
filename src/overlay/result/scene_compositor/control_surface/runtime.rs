//! JavaScript for controls rendered by the shared result scene compositor.

pub fn get_javascript() -> &'static str {
    r#"
window.registeredWindows = {}; window.L10N = #L10N_JSON#;
window.iconSvgs = #ICON_SVGS_JSON#;
let lastVisibleState = new Map();
let lastSentRegions = new Map();
let highestButtonStackOrder = 0;
let cursorX = 0, cursorY = 0;
const sourceRevealHoldMs = 1000;
let sourceRevealTimer = null, sourceRevealDeadline = 0;
let sourceRevealEpoch = '';
let proximityUpdatePending = false;
const activeGrabbingSources = new Set();
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
        topology_epoch: window.__SGT_TOPOLOGY_EPOCH__ || '1',
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
    window.__SGT_CONTROL_TOOLTIP__?.checkPointer(x, y);
    updateButtonOpacity();
};
window.invalidateButtonRegions = () => {
    window.__SGT_CONTROL_TOOLTIP__?.hide();
    lastVisibleState.clear();
    lastSentRegions.clear();
};
window.restoreButtonRegionsAfterDrag = gestureId => updateButtonOpacity(true, true, gestureId);
function scheduleSourceReveal(deadline) {
    const epoch = window.__SGT_TOPOLOGY_EPOCH__ || '1';
    if (sourceRevealTimer !== null && sourceRevealDeadline === deadline && sourceRevealEpoch === epoch) return;
    if (sourceRevealTimer !== null) clearTimeout(sourceRevealTimer);
    sourceRevealTimer = null;
    sourceRevealDeadline = deadline;
    sourceRevealEpoch = epoch;
    if (!Number.isFinite(deadline)) return;
    sourceRevealTimer = setTimeout(() => {
        sourceRevealTimer = null;
        sourceRevealDeadline = 0;
        if (epoch === (window.__SGT_TOPOLOGY_EPOCH__ || '1')) updateButtonOpacity();
    }, Math.max(1, Math.ceil(deadline - performance.now())));
}
function updateButtonOpacity(forceUpdate = false, restoreAfterDrag = false, gestureId = 0) {
    if (window.__SGT_BUTTON_SCENE__?.isGeometryPreviewActive()) return;
    const groups = document.querySelectorAll('.button-group');
    let needsUpdate = forceUpdate;
    const now = performance.now();
    let nextReveal = Infinity;

    groups.forEach(group => {
        const rect = group.getBoundingClientRect();

        let dx = 0, dy = 0;
        if (cursorX < rect.left) dx = rect.left - cursorX;
        else if (cursorX > rect.right) dx = cursorX - rect.right;

        if (cursorY < rect.top) dy = rect.top - cursorY;
        else if (cursorY > rect.bottom) dy = cursorY - rect.bottom;

        const dist = Math.sqrt(dx * dx + dy * dy);

        const maxRadius = 150;
        const geometry = window.registeredWindows[group.dataset.hwnd]?.sourceGeometry;
        const sourceNear = Boolean(window.__SGT_SOURCE_NEAR__?.(geometry, cursorX, cursorY, 40));
        if (!geometry?.source) group.sourceRevealUntil = 0;
        else if (sourceNear || group.sourceNear) group.sourceRevealUntil = now + sourceRevealHoldMs;
        group.sourceNear = sourceNear;
        if (geometry?.source && !sourceNear && now < (group.sourceRevealUntil || 0)) {
            nextReveal = Math.min(nextReveal, group.sourceRevealUntil);
        }
        const heldVisible = geometry?.source && (sourceNear || now < (group.sourceRevealUntil || 0)
            || group.contains(document.activeElement) || group.matches(':hover'));
        const proximityOpacity = heldVisible || group.classList.contains('proximity-pinned')
            ? 1 : Math.max(0, Math.min(1, 1 - (dist / maxRadius)));
        const pulseOpacity = Math.max(0, Math.min(1, Number(group.dataset.pulseOpacity || 0)));
        const opacity = group.dataset.previewOnly === 'true' ? 0 : Math.max(proximityOpacity, pulseOpacity);

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

    scheduleSourceReveal(nextReveal);
    if (needsUpdate) {
        const regions = [];
        const padding = 5;

        groups.forEach(group => {
            if (lastVisibleState.get(group.dataset.hwnd)) {
                const rect = group.getBoundingClientRect();
                const isVertical = group.classList.contains('vertical');
                let region;

                if (window.registeredWindows[group.dataset.hwnd]?.sourceGeometry) {
                    // Measured on every expansion frame; source discovery must
                    // not turn the surrounding empty canvas into an input area.
                    region = { x: rect.left, y: rect.top, w: rect.width + padding, h: rect.height + padding };
                } else if (isVertical) {
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
            action: restoreAfterDrag
                ? "restore_clickable_regions"
                : "update_clickable_regions",
            topology_epoch: window.__SGT_TOPOLOGY_EPOCH__ || '1',
            gesture_id: restoreAfterDrag ? gestureId : undefined,
            scale: window.devicePixelRatio || 1,
            regions: regions
        }));
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

    let buttons = '';

    if (!state.groupActions) {
        const backHideClass = canGoBack ? '' : 'hidden';
        buttons += `<div class="btn ${backHideClass}" onclick="action('${hwnd}', 'back')" data-l10n-title="back" aria-label="${escapeAttribute(window.L10N.back)}">
            ${window.iconSvgs.arrow_back}
        </div>`;

        const forwardHideClass = canGoForward ? '' : 'hidden';
        buttons += `<div class="btn ${forwardHideClass}" onclick="action('${hwnd}', 'forward')" data-l10n-title="forward" aria-label="${escapeAttribute(window.L10N.forward)}">
            ${window.iconSvgs.arrow_forward}
        </div>`;
    }

    if (!isBrowsing && !state.groupActions && state.modelLabel) {
        buttons += `<div class="model-badge">${escapeText(state.modelLabel)}</div>`;
    }

    const opacityValue = state.opacityPercent || 100;
    const verticalClass = isVertical ? 'vertical-slider' : '';
    buttons += `<div class="btn opacity-btn-expandable ${verticalClass} ${hideClass}" data-l10n-title="opacity" aria-label="${escapeAttribute(window.L10N.opacity)}">
        <div class="opacity-slider-wrapper">
            <input type="range" class="opacity-slider-inline" min="10" max="100" value="${opacityValue}"
                oninput="updateOpacity('${hwnd}', this.value)" />
            <span class="opacity-value-inline">${opacityValue}%</span>
        </div>
        <div class="opacity-icon-wrapper">
            ${window.iconSvgs.opacity}
        </div>
    </div>`;

    buttons += `<div class="btn ${state.copySuccess ? 'success' : ''} ${hideClass}" onclick="action('${hwnd}', 'copy')" data-l10n-title="copy" aria-label="${escapeAttribute(window.L10N.copy)}">
        ${window.iconSvgs[state.copySuccess ? 'check' : 'content_copy']}
    </div>`;

    if (!state.groupActions && state.hasUndo) {
        buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'undo')" data-l10n-title="undo" aria-label="${escapeAttribute(window.L10N.undo)}">
            ${window.iconSvgs.undo}
        </div>`;
    }

    if (!state.groupActions && state.hasRedo) {
        buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'redo')" data-l10n-title="redo" aria-label="${escapeAttribute(window.L10N.redo)}">
            ${window.iconSvgs.redo}
        </div>`;
    }

    if (state.editEnabled !== false) buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'edit')" data-l10n-title="edit" aria-label="${escapeAttribute(window.L10N.edit)}">
        ${window.iconSvgs.auto_awesome}
    </div>`;

    if (state.copyImage) buttons += `<div class="btn copy-image-btn ${hideClass}" onclick="window.__SGT_SOURCE_IMAGE__.request('${hwnd}')" data-l10n-title="copy_image" aria-label="${escapeAttribute(window.L10N.copy_image)}">
        ${window.iconSvgs.filter}
    </div>`;
    else buttons += `<div class="btn ${hideClass}" onclick="action('${hwnd}', 'download')" data-l10n-title="download" aria-label="${escapeAttribute(window.L10N.download)}">
        ${window.iconSvgs.download}
    </div>`;

    const speakerIcon = state.ttsLoading ? 'hourglass_empty' : (state.ttsSpeaking ? 'stop' : 'volume_up');
    const speakerClass = state.ttsLoading ? 'loading' : (state.ttsSpeaking ? 'active' : '');
    buttons += `<div class="btn ${speakerClass} ${hideClass}" onclick="action('${hwnd}', 'speaker')" data-l10n-title="speaker" aria-label="${escapeAttribute(window.L10N.speaker)}">
        ${window.iconSvgs[speakerIcon]}
    </div>`;

    buttons += `<div class="btn result-handle"
        onpointerdown="handleResultDrag(event, '${hwnd}', ${Boolean(state.groupActions)})"
        oncontextmenu="return false;"
        data-l10n-title="result_handle" aria-label="${escapeAttribute(window.L10N.result_handle)}">
        ${window.iconSvgs.cleaning_services}
    </div>`;

    return buttons;
}

window.addEventListener("pointerup", () => setOpacityDraggingCursor(false));
window.addEventListener("blur", () => setOpacityDraggingCursor(false));
document.addEventListener("visibilitychange", () => {
    if (document.hidden) setOpacityDraggingCursor(false);
});
for (const name of ['focusin', 'focusout', 'pointerover', 'pointerout']) {
    document.addEventListener(name, event => {
        if (!(event.target instanceof Element) || !event.target.closest('#button-container')
            || proximityUpdatePending) return;
        proximityUpdatePending = true;
        queueMicrotask(() => {
            proximityUpdatePending = false;
            if (window.document?.getElementById('button-container')) updateButtonOpacity();
        });
    });
}
document.addEventListener("pointerdown", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const group = target.closest(".button-group[data-hwnd]");
    if (group) {
        window.raiseWindowButtons(group.dataset.hwnd);
        window.ipc.postMessage(JSON.stringify({
            action: "interact",
            topology_epoch: window.__SGT_TOPOLOGY_EPOCH__ || '1',
            hwnd: group.dataset.hwnd
        }));
    }
    if (target.closest(".opacity-slider-inline")) {
        setOpacityDraggingCursor(true);
    }
});

function action(hwnd, cmd) {
    window.ipc.postMessage(JSON.stringify({ action: cmd, hwnd: hwnd, topology_epoch: window.__SGT_TOPOLOGY_EPOCH__ || '1' }));
}

window.updateButtonOpacity = updateButtonOpacity;
"#
}
