window.__SGT_APPLY_SCENE_BATCH__ = function(apply, acknowledgement) {
  try {
    apply();
    window.__SGT_VERIFY_SCENE_GEOMETRY__();
    if (acknowledgement) window.ipc.postMessage(JSON.stringify(acknowledgement));
  } catch (error) {
    window.ipc.postMessage(JSON.stringify({
      type: 'command_error', command: 'scene_batch', id: null,
      error: String(error && error.message ? error.message : error)
    }));
  }
};
