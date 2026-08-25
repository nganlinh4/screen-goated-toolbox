(function() {
  const models = new Map();
  let externalDrag = false;
  let nativeDrag = false;
  let awaitingDragSettle = false;
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
    models.set(key, current);
  }

  function clearClickableRegions() {
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
    clearClickableRegions();
  }

  function rebuild() {
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
      if (!model.visible || !model.controlRect || !model.controls || model.controls.hidden) continue;
      windows[key] = {
        rect: {
          x: model.controlRect.x / scale,
          y: model.controlRect.y / scale,
          w: model.controlRect.width / scale,
          h: model.controlRect.height / scale
        },
        state: model.controls
      };
    }
    window.updateWindows(windows);
    if (restoreControlsAfterLayout) {
      container.style.visibility = '';
      controlsHiddenForDrag = false;
    }
    for (const key of completedCards) tryPulseCompletion(key);
    for (const [key, model] of models) {
      if (model.stackOrder !== undefined) {
        window.setWindowButtonStackOrder(key, model.stackOrder);
      }
    }
    if (Object.keys(windows).length === 0) clearClickableRegions();
  }

  function apply(command) {
    if (command.type === 'snapshot') {
      models.clear();
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'upsert') {
      mergeCard(command.card);
    } else if (command.type === 'stream' || command.type === 'finalize') {
      mergeCard(command.card);
    } else if (command.type === 'geometry') {
      if (!nativeDrag && !awaitingDragSettle) window.clearResultDragControlPreview?.();
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'drag_settled') {
      for (const card of command.cards || []) mergeCard(card);
      externalDrag = false;
      setDragActive(false);
      return;
    } else if (command.type === 'controls') {
      for (const card of command.cards || []) mergeCard(card);
    } else if (command.type === 'opacity') {
      const key = String(command.id);
      const model = models.get(key);
      if (model && model.controls) model.controls.opacityPercent = Number(command.opacity);
      const card = document.querySelector('.result-card[data-id="' + key + '"]');
      if (card) card.style.opacity = String(Math.max(0, Math.min(100, command.opacity)) / 100);
    } else if (command.type === 'raise') {
      mergeCard({ id: command.id, stack_order: command.stack_order });
    } else if (command.type === 'remove') {
      const key = String(command.id);
      models.delete(key);
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

  function setDragActive(active) {
    const wasActive = nativeDrag;
    const wasAwaitingSettle = awaitingDragSettle;
    nativeDrag = Boolean(active);
    if (nativeDrag) {
      awaitingDragSettle = false;
      hideControlsForDrag();
    } else if (wasActive || wasAwaitingSettle || controlsHiddenForDrag) {
      awaitingDragSettle = false;
      window.clearResultDragControlPreview?.();
      rebuild();
    }
  }

  function releaseDragPreview() {
    if (!nativeDrag) return;
    nativeDrag = false;
    awaitingDragSettle = true;
    document.getElementById('button-container').style.visibility = '';
    controlsHiddenForDrag = false;
  }

  function tryPulseCompletion(key) {
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
    applyResultCommand(command);
    apply(command);
  };
  window.__SGT_BUTTON_SCENE__ = {
    rebuild: rebuild,
    clearClickableRegions: clearClickableRegions,
    setDragActive: setDragActive,
    releaseDragPreview: releaseDragPreview,
    pulseCompletion: pulseCompletion
  };
})();
