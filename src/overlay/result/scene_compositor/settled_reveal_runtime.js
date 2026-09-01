function setSettledSurfaceVisibility(entry, visible) {
  const visibility = visible ? 'visible' : 'hidden';
  entry.directHost.style.visibility = visibility;
  entry.frame.style.visibility = visibility;
}

function isolatedSurfaceVisibility(entry) {
  const cardStyle = getComputedStyle(entry.card);
  const frameStyle = getComputedStyle(entry.frame);
  const rect = entry.frame.getBoundingClientRect();
  const visible = entry.mode === 'isolated' && entry.frame.isConnected
    && !entry.card.hidden && !entry.frame.hidden
    && cardStyle.display !== 'none' && cardStyle.visibility === 'visible'
    && frameStyle.display !== 'none' && frameStyle.visibility === 'visible'
    && Number(cardStyle.opacity) > 0 && Number(frameStyle.opacity) > 0
    && rect.width >= 1 && rect.height >= 1 && rect.right > 0 && rect.bottom > 0
    && rect.left < window.innerWidth && rect.top < window.innerHeight;
  return {
    visible: visible,
    error: visible ? null : 'mode=' + entry.mode + ' connected=' + entry.frame.isConnected
      + ' card_hidden=' + entry.card.hidden + ' frame_hidden=' + entry.frame.hidden
      + ' card_display=' + cardStyle.display + ' frame_display=' + frameStyle.display
      + ' card_visibility=' + cardStyle.visibility + ' frame_visibility=' + frameStyle.visibility
      + ' card_opacity=' + cardStyle.opacity + ' frame_opacity=' + frameStyle.opacity
      + ' rect=' + rect.left + ',' + rect.top + ',' + rect.width + ',' + rect.height
      + ' viewport=' + window.innerWidth + ',' + window.innerHeight
  };
}

const sourceReplacementReveal = (function() {
  function dispose(value, reportPaint) {
    if (value.finished) return;
    value.finished = true;
    if (value.entry.sourceReplacementReveal === value) {
      value.entry.sourceReplacementReveal = null;
    }
    value.animation.cancel();
    value.surface.style.willChange = value.priorWillChange;
    if (reportPaint) value.complete();
  }

  function start(value) {
    if (value.finished || value.entry.sourceReplacementReveal !== value) return;
    if (!value.surface.isConnected || !value.entry.card.isConnected) {
      dispose(value, true);
      return;
    }
    value.surface.style.visibility = 'visible';
    value.animation.addEventListener('finish', function() {
      dispose(value, true);
    }, { once: true });
    value.animation.addEventListener('cancel', function() {
      dispose(value, false);
    }, { once: true });
    value.animation.currentTime = 0;
    value.animation.play();
  }

  function enqueue(entry, complete) {
    if (entry.sourceReplacementReveal) {
      dispose(entry.sourceReplacementReveal, false);
    }
    const surface = entry.visualSurface;
    if (typeof Element.prototype.animate !== 'function') {
      surface.style.visibility = 'visible';
      complete();
      return;
    }
    const value = {
      entry: entry,
      surface: surface,
      priorWillChange: surface.style.willChange,
      animation: surface.animate([
        { filter: 'blur(8px)', transform: 'translate3d(0,4px,0)' },
        { filter: 'blur(0)', transform: 'translate3d(0,0,0)' }
      ], { duration: 350, easing: 'cubic-bezier(0.2,0,0.2,1)', fill: 'both' }),
      complete: complete,
      finished: false
    };
    entry.sourceReplacementReveal = value;
    surface.style.willChange = 'filter,transform';
    value.animation.pause();
    value.animation.currentTime = 0;

    const readiness = [Promise.resolve(value.animation.ready).catch(function() {})];
    const backdrop = entry.backdrop;
    if (backdrop && backdrop.dataset.url && typeof backdrop.decode === 'function') {
      readiness.push(backdrop.decode().catch(function() {}));
    }
    const layoutReady = entry.directState.sourceLayoutReady;
    if (layoutReady && typeof layoutReady.then === 'function') {
      readiness.push(layoutReady.catch(function() {}));
    }
    Promise.all(readiness).then(function() {
      start(value);
    }, function() {
      start(value);
    });
  }

  return { enqueue: enqueue };
})();

function prepareSettledReveal(entry, contentRevision) {
  if (!String(entry.body || '').trim()) return false;
  entry.awaitingSettledReveal = true;
  entry.settledRevealRevision = contentRevision;
  entry.pendingSettledPaint = null;
  if (entry.sourceReplacement === true && entry.mode === 'direct') {
    entry.visualSurface.style.visibility = 'hidden';
  } else {
    setSettledSurfaceVisibility(entry, false);
  }
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
  function reportPendingPaint() {
    if (!pending) return;
    if (pending.kind === 'direct') {
      reportPaint(entry, pending.phase, pending.revision);
    } else {
      reportCardDiagnostic(entry.card.dataset.id, entry, pending.phase, pending.details);
    }
  }
  if (entry.sourceReplacement === true && entry.mode === 'direct') {
    sourceReplacementReveal.enqueue(entry, reportPendingPaint);
    return true;
  }
  setSettledSurfaceVisibility(entry, true);
  reportPendingPaint();
  return true;
}
