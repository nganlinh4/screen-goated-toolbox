window.__SGT_TOPOLOGY_EPOCH__ = '1';

window.__SGT_APPLY_SCENE_BATCH__ = function(apply, acknowledgement, paintToken, batchId) {
  try {
    apply();
    window.__SGT_VERIFY_SCENE_GEOMETRY__();
    if (batchId != null) window.ipc.postMessage(JSON.stringify({action: 'scene_batch_complete', batch_id: String(batchId)}));
    if (acknowledgement) window.ipc.postMessage(JSON.stringify(acknowledgement));
    if (paintToken) requestAnimationFrame(() => requestAnimationFrame(() =>
      window.ipc.postMessage('scene_painted:' + paintToken)));
  } catch (error) {
    window.ipc.postMessage(JSON.stringify({
      type: 'command_error', command: 'scene_batch', id: null,
      error: String(error && error.message ? error.message : error)
    }));
  }
};

window.__SGT_REBASE_TOPOLOGY__ = function(epoch, apply) {
  const current = String(epoch);
  window.__SGT_TOPOLOGY_EPOCH__ = current;
  try {
    window.__SGT_NOTIFY_CARDS_TOPOLOGY__?.(current);
    window.invalidateButtonRegions?.();
    window.__SGT_PROCESSING_CONTROLS__?.invalidate();
    window.__SGT_BUTTON_SCENE__?.resetTopology();
    apply();
    window.__SGT_VERIFY_SCENE_GEOMETRY__();
    window.__SGT_BUTTON_SCENE__?.rebuild();
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (window.__SGT_TOPOLOGY_EPOCH__ !== current) return;
      try {
        window.invalidateButtonRegions?.();
        window.__SGT_PROCESSING_CONTROLS__?.invalidate();
        window.__SGT_BUTTON_SCENE__?.rebuild();
        if (typeof window.updateButtonOpacity !== 'function') throw new Error('button region publisher unavailable');
        window.updateButtonOpacity(true);
        window.__SGT_REFINE_EDITOR__?.republishDrafts();
        window.ipc.postMessage(JSON.stringify({action: 'topology_ready', epoch: current}));
        window.__SGT_REFINE_EDITOR__?.republishFocus();
      } catch (error) {
        window.ipc.postMessage(JSON.stringify({type: 'command_error', command: 'topology_rebase', id: null,
          error: String(error && error.message ? error.message : error)}));
      }
    }));
  } catch (error) {
    window.ipc.postMessage(JSON.stringify({type: 'command_error', command: 'topology_rebase', id: null,
      error: String(error && error.message ? error.message : error)}));
  }
};
