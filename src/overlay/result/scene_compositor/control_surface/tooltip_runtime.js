window.__SGT_CONTROL_TOOLTIP__ = (() => {
    const selector = '#button-container [data-l10n-title]';
    const control = element => element?.closest?.(selector);
    let target = null, pointer = null, timer = null, tooltip = null;
    let published = 'null';

    function publish(rect) {
        const key = JSON.stringify(rect);
        if (key === published) return;
        published = key;
        window.ipc.postMessage(JSON.stringify({
            action: 'control_tooltip_region',
            topology_epoch: window.__SGT_TOPOLOGY_EPOCH__ || '1',
            rect,
        }));
    }

    function hide() {
        clearTimeout(timer);
        timer = null;
        target = null;
        if (tooltip) tooltip.hidden = true;
        publish(null);
    }

    function containsPointer(rect) {
        return pointer && pointer.x >= rect.left && pointer.x < rect.right
            && pointer.y >= rect.top && pointer.y < rect.bottom;
    }

    function checkPointer(x, y) {
        pointer = { x, y };
        const next = control(document.elementFromPoint?.(x, y));
        if (target && (next !== target || !containsPointer(target.getBoundingClientRect()))) hide();
        // Leaving a clipped native input region can omit the browser's leave
        // event. Recover when the physical pointer re-enters the same control.
        if (!target && next?.matches(':hover') && activeGrabbingSources.size === 0) start(next);
    }

    function refresh() {
        if (!target || timer !== null) return;
        const anchor = target.getBoundingClientRect();
        const label = target.getAttribute('aria-label');
        const style = getComputedStyle(target);
        if (!label || !target.isConnected || !containsPointer(anchor)
            || activeGrabbingSources.size > 0 || target.closest('.button-group')?.inert
            || style.visibility === 'hidden' || style.pointerEvents === 'none') {
            hide();
            return;
        }

        if (!tooltip) {
            tooltip = document.createElement('div');
            tooltip.className = 'result-control-tooltip';
            tooltip.setAttribute('aria-hidden', 'true'); // The control already carries the label.
            document.getElementById('button-container').appendChild(tooltip);
        }
        const work = target.closest('.button-group')?.controlPlacement?.work
            || { x: 0, y: 0, w: innerWidth, h: innerHeight };
        const scale = window.__SGT_MONITOR_SCALE_FOR_RECT__?.({ x: anchor.left, y: anchor.top,
            w: anchor.width, h: anchor.height }) || 1;
        const gap = 6 * scale;
        const above = Math.max(0, anchor.top - work.y - 2 * gap);
        const below = Math.max(0, work.y + work.h - anchor.bottom - 2 * gap);
        if (work.w <= 2 * gap || Math.max(above, below) === 0) {
            hide();
            return;
        }
        tooltip.style.setProperty('--tooltip-scale', scale);
        tooltip.style.maxWidth = Math.min(320 * scale, work.w - 2 * gap) + 'px';
        tooltip.style.maxHeight = Math.max(above, below) + 'px';
        tooltip.textContent = label;
        tooltip.hidden = false;
        const size = tooltip.getBoundingClientRect();
        const x = Math.max(work.x + gap, Math.min(anchor.left + (anchor.width - size.width) / 2,
            work.x + work.w - size.width - gap));
        const y = size.height <= below ? anchor.bottom + gap : anchor.top - gap - size.height;
        tooltip.style.left = x + 'px';
        tooltip.style.top = y + 'px';

        // The native visual region uses physical pixels relative to the compositor.
        const dpr = window.devicePixelRatio || 1;
        const rect = tooltip.getBoundingClientRect();
        const left = Math.floor(rect.left * dpr), top = Math.floor(rect.top * dpr);
        publish({ x: left, y: top, width: Math.ceil(rect.right * dpr) - left,
            height: Math.ceil(rect.bottom * dpr) - top });
    }

    function start(next) {
        hide();
        target = next;
        timer = setTimeout(() => { timer = null; refresh(); }, 450);
    }

    document.addEventListener('pointerover', event => {
        const next = control(event.target);
        if (!next || next === target || event.buttons || event.pointerType === 'touch') return;
        pointer = { x: event.clientX, y: event.clientY };
        start(next);
    });
    document.addEventListener('pointerout', event => {
        if (target && control(event.target) === target && control(event.relatedTarget) !== target) hide();
    });
    document.addEventListener('pointerdown', hide, true);
    document.addEventListener('keydown', event => { if (event.key === 'Escape') hide(); });
    document.addEventListener('visibilitychange', () => { if (document.hidden) hide(); });
    window.addEventListener('blur', hide);
    return { checkPointer, hide, refresh };
})();
