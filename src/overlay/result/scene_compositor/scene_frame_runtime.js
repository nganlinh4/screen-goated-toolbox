function navigateTo(entry, url, epoch = window.__SGT_TOPOLOGY_EPOCH__ || '1') {
  window.ipc.postMessage(JSON.stringify({
    type: 'navigation_request', id: Number(entry.card.dataset.id), url: String(url), topology_epoch: epoch
  }));
}
window.addEventListener('message', function(event) {
  if (!event.data) return;
  if (event.data.type !== 'card_diagnostic') return;
  if (typeof event.data.card_id !== 'string' || typeof event.data.document_revision !== 'number') return;
  const id = event.data.card_id;
  const entry = cards.get(id);
  if (!entry || entry.mode !== 'isolated') return;
  if (event.source !== entry.frame.contentWindow) return;
  if (event.data.document_revision !== entry.revision) return;
  // Authored documents may report their own visible interactive readiness.
  // This is observational only and at most once per document; effects never
  // arrive over the public window message channel.
  if (event.data.phase === 'interactive_document_alive') {
    if (!entry.commandPort || entry.authorInteractiveReported) return;
    entry.authorInteractiveReported = true;
    handleIsolatedCardMessage(id, entry, {
      type: 'card_diagnostic', phase: 'interactive_document_alive',
      payload_len: typeof event.data.payload_len === 'number' && Number.isFinite(event.data.payload_len)
        ? Math.min(Math.max(event.data.payload_len, 0), 10000000) : 0,
      text_len: typeof event.data.text_len === 'number' && Number.isFinite(event.data.text_len)
        ? Math.min(Math.max(event.data.text_len, 0), 10000000) : 0,
      opacity: typeof event.data.opacity === 'string' ? event.data.opacity.slice(0, 32) : ''
    });
    return;
  }
  if (event.data.phase !== 'bridge_ready') return;
  if (entry.commandPort || !event.ports || event.ports.length !== 1) return;
  const port = event.ports[0];
  const documentRevision = entry.revision;
  entry.commandPort = port;
  port.addEventListener('message', function(portEvent) {
    if (entry.mode !== 'isolated' || entry.revision !== documentRevision || entry.commandPort !== port) return;
    handleIsolatedCardMessage(id, entry, portEvent.data);
  });
  port.start();
  entry.activateIsolatedBridge();
});
function handleIsolatedCardMessage(id, entry, data) {
  if (!data || typeof data !== 'object') return;
  if (['card_interaction', 'card_navigation', 'copy_selection'].includes(data.type)
      && String(data.topology_epoch || '1') !== (window.__SGT_TOPOLOGY_EPOCH__ || '1')) return;
    if (data.type === 'fit_diagnostic') {
      if (!allowFrameDiagnostic()) return;
      try {
        const encoded = JSON.stringify({ type: 'fit_diagnostic', id: Number(id), payload: data.payload });
        if (encoded.length <= 8192) window.ipc.postMessage(encoded);
      } catch (_) { /* Unserializable page diagnostics are not host events. */ }
    } else if (data.type === 'frame_request') {
      const handle = Number(data.handle || 0);
      const documentRevision = entry.revision;
      requestAnimationFrame(function(timestamp) {
        if (entry.revision !== documentRevision) return;
        postCardMessage(entry, { type: 'frame_tick', handle: handle, timestamp: timestamp });
      });
    } else if (data.type === 'fit_request') {
      queueFit(entry, data.streaming);
    } else if (data.type === 'fit_complete') {
      completeFit(entry);
    } else if (data.type === 'card_diagnostic') {
      if ((data.phase === 'script_error' || data.phase === 'promise_rejection') && !allowFrameDiagnostic()) return;
      const revision = typeof data.content_revision === 'number' && Number.isFinite(data.content_revision)
        ? data.content_revision : 0;
      if (String(data.phase || '').startsWith('font_ready_')) {
        entry.fontReady = true;
        const flushed = flushPendingContent(entry);
        if (entry.visible && !flushed) {
          queueFit(entry, entry.streaming);
        }
      }
      if (!revision || revision === entry.contentRevision) {
        const phase = typeof data.phase === 'string' ? data.phase.slice(0, 128) : 'bridge_unknown';
        const details = {
          revision: revision || entry.contentRevision,
          payloadLen: typeof data.payload_len === 'number' && Number.isFinite(data.payload_len) ? data.payload_len : 0,
          textLen: typeof data.text_len === 'number' && Number.isFinite(data.text_len) ? data.text_len : 0,
          opacity: typeof data.opacity === 'string' ? data.opacity.slice(0, 32) : '',
          error: typeof data.error === 'string' ? data.error.slice(0, 1024) : null
        };
        if (!deferIsolatedSettledPaint(entry, phase, details)) {
          reportCardDiagnostic(id, entry, phase, details);
        }
        if (phase === 'interactive_document_alive') {
          const surface = isolatedSurfaceVisibility(entry);
          reportCardDiagnostic(id, entry, surface.visible ? 'interactive_surface_visible'
            : 'interactive_surface_rejected', { error: surface.error });
        }
      }
    } else if (data.type === 'card_interaction') {
      raiseCard(entry);
      window.ipc.postMessage(JSON.stringify({ type: 'interaction', id: Number(id), topology_epoch: String(data.topology_epoch || '1') }));
    } else if (data.type === 'copy_selection') {
      window.ipc.postMessage(JSON.stringify({
        action: 'copy_selection', hwnd: id, text: String(data.text || ''), topology_epoch: String(data.topology_epoch || '1')
      }));
    } else if (data.type === 'card_navigation') {
      navigateTo(entry, data.url, String(data.topology_epoch || '1'));
    }
}
