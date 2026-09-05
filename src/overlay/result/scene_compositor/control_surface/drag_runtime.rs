//! Pointer-drag JavaScript for controls rendered by the shared result scene compositor.

pub fn get_javascript() -> &'static str {
    r#"
let activeResultDragPreview = null;
let settlingResultDragTargets = new Set();

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
    const gestureId = window.__SGT_BUTTON_SCENE__?.allocateGestureId();
    if (!gestureId) return;
    activeResultDragPreview = {
        gestureId: gestureId,
        hwnd: String(hwnd), targets: targets, pointerId: e.pointerId,
        startX: e.clientX, startY: e.clientY, dx: 0, dy: 0, frame: 0,
        cardOrigins: cardOrigins, nativeTargets: nativeTargets
    };
    settlingResultDragTargets.clear();

    let action = 'result_drag_start';
    if (e.button === 0 && groupActions) action = 'result_group_drag_start';
    else if (e.button === 1) action = 'result_all_drag_start';
    else if (e.button === 2) action = 'result_group_drag_start';

    window.ipc.postMessage(JSON.stringify({
        action: action,
        hwnd: hwnd,
        gesture_id: gestureId,
        pointer_type: e.pointerType,
        button: e.button,
        pointer_id: e.pointerId,
        origin_x: Math.round(e.clientX * (window.devicePixelRatio || 1)),
        origin_y: Math.round(e.clientY * (window.devicePixelRatio || 1))
    }));
    window.__SGT_BUTTON_SCENE__?.setDragActive(true, gestureId);
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
            gesture_id: activeResultDragPreview.gestureId,
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
        gesture_id: drag.gestureId,
        cancelled: !event || event.type !== 'pointerup',
        dx: Math.round(drag.dx * scale), dy: Math.round(drag.dy * scale)
    }));
    window.__SGT_BUTTON_SCENE__?.releaseDragPreview(
        event ? event.clientX : undefined,
        event ? event.clientY : undefined,
        drag.gestureId);
    settlingResultDragTargets = new Set(drag.targets);
    activeResultDragPreview = null;
    setResultDraggingCursor(false);
}
document.addEventListener('pointermove', queueResultDragPreview, true);
document.addEventListener('pointerup', finishLocalResultDrag, true);
document.addEventListener('pointercancel', finishLocalResultDrag, true);
window.addEventListener('blur', () => finishLocalResultDrag(null));
document.addEventListener('visibilitychange', () => {
    if (document.hidden) finishLocalResultDrag(null);
});
window.clearResultDragControlPreview = function() {
    document.querySelectorAll('.button-group').forEach(group => { group.style.translate = ''; });
};
window.shouldPreserveResultDragGeometry = function(id) {
    const key = String(id);
    return Boolean(activeResultDragPreview?.targets.includes(key)) || settlingResultDragTargets.has(key);
};
window.releaseResultDragGeometryLock = function() {
    settlingResultDragTargets.clear();
};
window.settleResultDragGesture = function(gestureId) {
    const drag = activeResultDragPreview;
    if (!drag || drag.gestureId !== Number(gestureId)) return;
    if (drag.frame) cancelAnimationFrame(drag.frame);
    activeResultDragPreview = null;
    setResultDraggingCursor(false);
};
window.cancelResultDragForCard = function(id) {
    const drag = activeResultDragPreview;
    if (!drag || !drag.targets.includes(String(id))) return;
    if (drag.frame) cancelAnimationFrame(drag.frame);
    activeResultDragPreview = null;
    settlingResultDragTargets.clear();
    setResultDraggingCursor(false);
    window.__SGT_BUTTON_SCENE__?.setDragActive(false, drag.gestureId);
};
"#
}
