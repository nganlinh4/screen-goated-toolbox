(function() {
  const models = new Map();
  const controlGeometry = window.__SGT_CONTROL_GEOMETRY_CACHE__();
  let externalDrag = false;
  let nativeDrag = 0;
  let awaitingDragSettle = 0;
  let nextGestureId = 1;
  let controlsHiddenForDrag = false;
  const completionPulseTokens = new Map();
  const completedCards = new Set();

  function mergeCard(model) {
    const key = String(model.id);
    const current = models.get(key) || { id: Number(model.id), visible: false };
    if (model.rect) current.rect = model.rect;
    if (model.control_rect) current.controlRect = model.control_rect;
    if (model.controls) current.controls = model.controls;
    if (model.visible !== undefined) current.visible = Boolean(model.visible);
    if (model.stack_order !== undefined) current.stackOrder = Number(model.stack_order || 0);
    controlGeometry.merge(current, model);
    models.set(key, current);
  }

  function clearClickableRegions() {
    window.invalidateButtonRegions?.();
    window.ipc.postMessage(JSON.stringify({
      action: 'update_clickable_regions',
      scale: window.devicePixelRatio || 1,
      regions: []
    }));
  }

  function hideControlsForDrag() {
    if (controlsHiddenForDrag) return;
    controlsHiddenForDrag = true;
    document.getElementById('button-container').style.visibility = 'hidden';
    window.__SGT_PROCESSING_CONTROLS__?.hide(true);
    clearClickableRegions();
  }

  function allocateGestureId() {
    const id = nextGestureId;
    nextGestureId = nextGestureId >= 2147483647 ? 1 : nextGestureId + 1;
    return id;
  }

  function rebuild(restoreGestureId) {
    const container = document.getElementById('button-container');
    if (externalDrag) {
      hideControlsForDrag();
      return;
    }
    if (nativeDrag || awaitingDragSettle) return;
    const restoreControlsAfterLayout = controlsHiddenForDrag;
    const scale = window.devicePixelRatio || 1;
    const windows = {};
    for (const [key, model] of models) {
      const sourceGeometry = controlGeometry.get(model, models, scale);
      const processing = sourceGeometry?.source ? window.__SGT_PROCESSING_CONTOURS__?.controlState(key) : null;
      if ((!model.visible && !processing) || !model.controlRect || !model.controls || model.controls.hidden) continue;
      windows[key] = {
        rect: {
          x: model.controlRect.x / scale,
          y: model.controlRect.y / scale,
          w: model.controlRect.width / scale,
          h: model.controlRect.height / scale
        },
        state: model.controls,
        previewOnly: !model.visible,
        processing,
        sourceGeometry
      };
    }
    if (restoreControlsAfterLayout) {
      container.style.visibility = '';
      controlsHiddenForDrag = false;
      window.__SGT_PROCESSING_CONTROLS__?.hide(false);
    }
    window.updateWindows(windows);
    if (restoreGestureId !== undefined) {
      window.restoreButtonRegionsAfterDrag?.(restoreGestureId);
    }
    for (const key of completedCards) tryPulseCompletion(key);
    for (const [key, model] of models) {
      if (model.stackOrder !== undefined) {
        window.setWindowButtonStackOrder(key, model.stackOrder);
      }
    }
    if (Object.keys(windows).length === 0) clearClickableRegions();
  }

  function acceptsSettlement(command) {
    const id = Number(command.gesture_id || 0);
    return id !== 0
      ? nativeDrag === id || awaitingDragSettle === id
      : externalDrag && !nativeDrag && !awaitingDragSettle;
  }

  function releaseGeometryPreview(gestureId) {
    window.settleResultDragGesture?.(gestureId);
    window.__SGT_CARD_RESIZE__?.settleGesture(gestureId);
    window.clearResultDragControlPreview?.();
    window.releaseResultDragGeometryLock?.();
  }

  function apply(command) {
    if (command.type === 'processing') {
      rebuild(); return;
    } else if (command.type === 'snapshot') {
      models.clear();
      controlGeometry.clear();
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'upsert') {
      mergeCard(command.card);
    } else if (command.type === 'upsert_batch') {
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'stream' || command.type === 'finalize') {
      mergeCard(command.card);
    } else if (command.type === 'geometry') {
      if (!nativeDrag && !awaitingDragSettle) window.clearResultDragControlPreview?.();
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'drag_settled') {
      const gestureId = Number(command.gesture_id || 0);
      for (const card of command.cards || []) mergeCard(card);
      setDragActive(false, gestureId);
      return;
    } else if (command.type === 'controls') {
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'opacity') {
      const key = String(command.id);
      const model = models.get(key);
      if (model && model.controls) model.controls.opacityPercent = Number(command.opacity);
    } else if (command.type === 'raise') {
      mergeCard({ id: command.id, stack_order: command.stack_order });
    } else if (command.type === 'remove') {
      const key = String(command.id);
      models.delete(key);
      controlGeometry.clear();
      completedCards.delete(key);
    } else if (command.type === 'refine_text') {
      window.setRefineText(String(command.id), String(command.text || ''), Boolean(command.is_insert));
      return;
    } else if (command.type === 'external_drag') {
      externalDrag = Boolean(command.active);
    } else if (command.type === 'theme') {
      const style = document.getElementById('sgt-controls-theme-css');
      if (style) style.textContent = String(command.theme.controls_css || '');
    } else {
      return;
    }
    rebuild();
  }

  function setDragActive(active, gestureId) {
    const id = Number(gestureId || 0);
    if (active) {
      if (!Number.isSafeInteger(id) || id <= 0) return;
      nativeDrag = id;
      awaitingDragSettle = 0;
      hideControlsForDrag();
      return;
    }
    const matchesNative = id !== 0 && nativeDrag === id;
    const matchesAwaiting = id !== 0 && awaitingDragSettle === id;
    const matchesExternal = id === 0 && externalDrag && !nativeDrag && !awaitingDragSettle;
    if (!matchesNative && !matchesAwaiting && !matchesExternal) return;
    if (matchesNative) nativeDrag = 0;
    if (matchesAwaiting) awaitingDragSettle = 0;
    if (matchesExternal) externalDrag = false;
    if (nativeDrag || awaitingDragSettle || externalDrag) return;
    releaseGeometryPreview(id);
    rebuild(id);
  }

  function releaseDragPreview(pointerX, pointerY, gestureId) {
    const id = Number(gestureId || 0);
    if (!id || nativeDrag !== id) return;
    nativeDrag = 0;
    awaitingDragSettle = id;
    if (Number.isFinite(pointerX) && Number.isFinite(pointerY)) {
      window.updateCursorPosition?.(pointerX, pointerY);
    }
  }

  function tryPulseCompletion(key) {
    if (window.__SGT_PROCESSING_CONTOURS__?.controlState(key)) return;
    const model = models.get(key);
    const token = Number(model?.controls?.onboardingPulseToken || 0);
    if (!token || completionPulseTokens.get(key) === token) return;
    const group = document.querySelector('.button-group[data-hwnd="' + key + '"]');
    if (!group) return;
    completionPulseTokens.set(key, token);
    const started = performance.now();
    const duration = 1250;
    function animatePulse(now) {
      const progress = Math.min(1, (now - started) / duration);
      const pulseOpacity = Math.sin(Math.PI * progress);
      const scale = 1 + (0.05 * Math.sin(Math.PI * progress));
      group.dataset.pulseOpacity = String(pulseOpacity);
      group.style.setProperty('transform', 'scale(' + scale + ')', 'important');
      window.updateButtonOpacity();
      if (progress < 1) {
        requestAnimationFrame(animatePulse);
        return;
      }
      delete group.dataset.pulseOpacity;
      group.style.removeProperty('transform');
      window.updateButtonOpacity();
    }
    requestAnimationFrame(animatePulse);
  }

  function pulseCompletion(id) {
    const key = String(id);
    completedCards.add(key);
    tryPulseCompletion(key);
  }

  const applyResultCommand = window.applyHostCommand;
  window.applyHostCommand = function(command) {
    if (command.type === 'drag_settled') {
      if (!acceptsSettlement(command)) return;
      releaseGeometryPreview(Number(command.gesture_id || 0));
    }
    applyResultCommand(command);
    apply(command);
  };
  window.__SGT_BUTTON_SCENE__ = {
    allocateGestureId: allocateGestureId,
    rebuild: rebuild,
    clearClickableRegions: clearClickableRegions,
    setDragActive: setDragActive,
    releaseDragPreview: releaseDragPreview,
    isGeometryPreviewActive: function() { return Boolean(nativeDrag || awaitingDragSettle || externalDrag); },
    pulseCompletion: pulseCompletion
  };
})();
