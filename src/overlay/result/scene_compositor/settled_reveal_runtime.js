function setSettledSurfaceVisibility(entry, visible) {
  const visibility = visible ? 'visible' : 'hidden';
  entry.directHost.style.visibility = visibility;
  entry.frame.style.visibility = visibility;
}

function prepareSettledReveal(entry, contentRevision) {
  if (!String(entry.body || '').trim()) return false;
  entry.awaitingSettledReveal = true;
  entry.settledRevealRevision = contentRevision;
  entry.pendingSettledPaint = null;
  setSettledSurfaceVisibility(entry, false);
  return true;
}

function reportPaint(entry, phase, contentRevision) {
  requestAnimationFrame(function() {
    requestAnimationFrame(function() {
      if (contentRevision !== entry.contentRevision) return;
      const text = (entry.bodyElement.innerText || entry.bodyElement.textContent || '').trim();
      const style = getComputedStyle(entry.bodyElement);
      reportCardDiagnostic(entry.card.dataset.id, entry, phase + '_painted', {
        revision: contentRevision,
        payloadLen: entry.bodyElement.innerHTML.length,
        textLen: text.length,
        opacity: style.opacity
      });
    });
  });
}

function reportOrDeferPaint(entry, phase, contentRevision) {
  if (!String(entry.body || '').trim()) return;
  if (entry.awaitingSettledReveal && entry.settledRevealRevision === contentRevision) {
    entry.pendingSettledPaint = {
      kind: 'direct',
      phase: phase,
      revision: contentRevision
    };
    return;
  }
  reportPaint(entry, phase, contentRevision);
}

function deferIsolatedSettledPaint(entry, phase, details) {
  if (!entry.awaitingSettledReveal || entry.settledRevealRevision !== details.revision
      || !String(phase).endsWith('_painted')) return false;
  entry.pendingSettledPaint = {
    kind: 'isolated',
    phase: phase,
    details: details
  };
  return true;
}

function revealSettledContent(entry, contentRevision) {
  if (!entry.awaitingSettledReveal || entry.settledRevealRevision !== contentRevision
      || entry.contentRevision !== contentRevision) return false;
  const pending = entry.pendingSettledPaint;
  entry.awaitingSettledReveal = false;
  entry.pendingSettledPaint = null;
  setSettledSurfaceVisibility(entry, true);
  if (!pending) return true;
  if (pending.kind === 'direct') {
    reportPaint(entry, pending.phase, pending.revision);
  } else {
    reportCardDiagnostic(entry.card.dataset.id, entry, pending.phase, pending.details);
  }
  return true;
}
