let controlMonitors = [];
let controlFontRevision = 0;
let controlLayoutFrame = null;
function queueControlLayout() {
    if (controlLayoutFrame !== null) return;
    controlLayoutFrame = requestAnimationFrame(() => {
        controlLayoutFrame = null;
        window.__SGT_BUTTON_SCENE__?.rebuild();
    });
}
window.__SGT_SET_CONTROL_MONITORS__ = function(monitors) {
    controlMonitors = Array.isArray(monitors) ? monitors : [];
    queueControlLayout();
};
window.addEventListener('resize', queueControlLayout);
document.fonts.addEventListener('loadingdone', () => { controlFontRevision++; queueControlLayout(); });
const controlResizeObserver = new ResizeObserver(entries => {
    for (const entry of entries) {
        const box = entry.borderBoxSize?.[0];
        if (!box) continue;
        const size = { w: box.inlineSize, h: box.blockSize };
        const old = entry.target.controlMeasuredSize;
        if (!old || Math.abs(old.w - size.w) > 0.1 || Math.abs(old.h - size.h) > 0.1) {
            entry.target.controlMeasuredSize = size;
            queueControlLayout();
        }
    }
});

function measureControlOrientations(group, hwnd, state, editor, source) {
    const result = {};
    for (const vertical of [false, true]) {
        if (editor && vertical) continue;
        const probe = group.cloneNode(editor);
        probe.removeAttribute('data-hwnd');
        probe.inert = true;
        probe.classList.toggle('vertical', vertical);
        probe.style.cssText += ';visibility:hidden!important;pointer-events:none!important;transform:none!important;'
            + 'left:0;right:auto;top:0;bottom:auto;max-width:none;max-height:none;overflow:visible;';
        if (!editor) probe.innerHTML = generateButtonsHTML(hwnd, state, vertical);
        group.parentElement.appendChild(probe);
        result[vertical ? 'vertical' : 'horizontal'] = { w: probe.offsetWidth, h: probe.offsetHeight };
        if (source && !editor) {
            probe.classList.add('control-measure-expanded');
            Object.assign(result[vertical ? 'vertical' : 'horizontal'],
                { reserveW: probe.offsetWidth, reserveH: probe.offsetHeight });
        }
        probe.remove();
    }
    return result;
}

function updateWindows(windowsData) {
    window.registeredWindows = windowsData;
    const container = document.getElementById('button-container');
    const screenW = window.innerWidth, screenH = window.innerHeight;
    const deviceScale = window.devicePixelRatio || 1;
    const rect = raw => ({ x: raw[0] / deviceScale, y: raw[1] / deviceScale,
        w: raw[2] / deviceScale, h: raw[3] / deviceScale });
    const monitors = controlMonitors.map(m => ({ bounds: rect(m.bounds), work: rect(m.work) }));
    const existingGroups = new Map();
    const processingIds = new Set();
    container.querySelectorAll('.button-group').forEach(el => existingGroups.set(el.dataset.hwnd, el));

    for (const [hwnd, data] of Object.entries(windowsData)) {
        const state = data.state || {};
        const rawAnchor = state.controlAnchor;
        const placementRect = Array.isArray(rawAnchor) && rawAnchor.length === 4 ? rect(rawAnchor) : data.rect;
        const geometry = data.sourceGeometry || { anchor: placementRect, obstacles: [placementRect], source: false };
        const controlScale = Math.max(0.5, Math.min(3, Number(state.controlScalePercent || 100) / 100));
        let group = existingGroups.get(hwnd);
        if (!group) {
            group = document.createElement('div');
            group.className = 'button-group';
            group.style.opacity = '0';
            group.dataset.hwnd = hwnd;
            container.appendChild(group);
            controlResizeObserver.observe(group);
        } else existingGroups.delete(hwnd);
        group.inert = Boolean(data.previewOnly);
        group.dataset.previewOnly = data.previewOnly ? 'true' : 'false';
        group.style.visibility = data.previewOnly ? 'hidden' : '';

        const { opacityPercent, ...structuralState } = state;
        const stateKey = JSON.stringify(structuralState) + ':' + controlFontRevision;
        const editor = Boolean(window.__SGT_REFINE_EDITOR__?.reconcile(group, hwnd, state));
        group.classList.toggle('proximity-pinned', editor);
        if (state.controlColor) group.style.setProperty('--chain-control-color', state.controlColor);
        else group.style.removeProperty('--chain-control-color');
        const localSurface = contrastingControlSurface(state.controlColor);
        group.classList.toggle('local-control-surface-light', localSurface === 'light');
        group.classList.toggle('local-control-surface-dark', localSurface === 'dark');
        group.style.setProperty('--control-scale', String(controlScale));
        if (group.controlSizeKey !== stateKey) {
            group.controlSizes = measureControlOrientations(group, hwnd, state, editor, geometry.source);
            group.controlSizeKey = stateKey;
            group.controlMeasuredSize = null;
        }
        const previous = group.controlPlacement;
        const sameAnchor = previous?.anchor && ['x', 'y', 'w', 'h'].every(k => previous.anchor[k] === geometry.anchor[k]);
        const locked = !editor && previous && (!geometry.source || sameAnchor)
            && (group.matches(':hover') || group.contains(document.activeElement) || activeGrabbingSources.size > 0
                || (geometry.source && lastVisibleState.get(hwnd)));
        const sizes = { ...group.controlSizes };
        if (previous && group.controlMeasuredSize) {
            const key = previous.vertical ? 'vertical' : 'horizontal';
            if (sizes[key]) sizes[key] = { ...sizes[key], w: Math.max(sizes[key].w, group.controlMeasuredSize.w),
                h: Math.max(sizes[key].h, group.controlMeasuredSize.h) };
        }
        const work = window.__SGT_CONTROL_PLACEMENT__.workArea(geometry.anchor, monitors,
            { x: 0, y: 0, w: screenW, h: screenH });
        const layoutKey = JSON.stringify([stateKey, geometry.anchor, sizes, work, Boolean(locked)]);
        let pos = previous;
        if (group.controlLayoutKey !== layoutKey || (geometry.source && group.controlGeometry !== geometry)) {
            pos = window.__SGT_CONTROL_PLACEMENT__.place(geometry, sizes, work, previous, locked);
            group.controlLayoutKey = layoutKey;
            group.controlGeometry = geometry;
        }
        if (!pos) continue;
        const newStateStr = stateKey + pos.vertical;
        if (!editor && group.dataset.lastState !== newStateStr) {
            group.innerHTML = generateButtonsHTML(hwnd, state, pos.vertical);
            group.controlMeasuredSize = null;
        }
        group.dataset.lastState = newStateStr;
        group.classList.toggle('vertical', pos.vertical);
        group.controlPlacement = pos;
        const natural = group.controlSizes[pos.vertical ? 'vertical' : 'horizontal'];
        // An exceptionally small work area remains scrollable, not unreachable.
        group.style.maxWidth = work.w + 'px';
        group.style.maxHeight = work.h + 'px';
        group.style.overflow = natural && (Math.max(natural.w, natural.reserveW || 0) > work.w
            || Math.max(natural.h, natural.reserveH || 0) > work.h) ? 'auto' : '';
        const opacity = group.querySelector('.opacity-slider-inline');
        const opacityLabel = group.querySelector('.opacity-value-inline');
        if (opacity && opacityPercent != null) {
            opacity.value = opacityPercent;
            if (opacityLabel) opacityLabel.textContent = opacityPercent + '%';
        }
        if (geometry.source || pos.direction === 'bottom' || pos.direction === 'right') {
            group.style.left = 'auto';
            group.style.right = (screenW - (pos.x + pos.w)) + 'px';
        } else { group.style.left = pos.x + 'px'; group.style.right = 'auto'; }
        if (pos.vertical) {
            group.style.top = 'auto';
            group.style.bottom = (screenH - (pos.y + pos.h)) + 'px';
        } else { group.style.top = pos.y + 'px'; group.style.bottom = 'auto'; }
        if (data.processing) {
            processingIds.add(data.processing.id);
            window.__SGT_PROCESSING_CONTROLS__.layout(group, data.processing, work);
        }
    }
    window.__SGT_PROCESSING_CONTROLS__.retain(processingIds);
    existingGroups.forEach((el, key) => {
        controlResizeObserver.unobserve(el);
        el.remove();
        lastVisibleState.delete(key);
        lastSentRegions.delete(key);
    });
    window.__SGT_REFINE_EDITOR__?.settleFocusMode();
    updateButtonOpacity();
    if (container.querySelectorAll('.button-group').length === 0) {
        window.ipc.postMessage(JSON.stringify({ action: 'update_clickable_regions', scale: deviceScale, regions: [] }));
    }
}
window.updateWindows = updateWindows;
