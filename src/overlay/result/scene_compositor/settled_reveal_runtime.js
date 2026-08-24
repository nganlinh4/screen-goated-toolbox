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
  let pending = [];
  let frame = 0;

  function restore(item) {
    item.host.style.cssText = item.style;
    if (item.placeholder.parentNode) {
      item.placeholder.parentNode.insertBefore(item.host, item.placeholder);
      item.placeholder.remove();
    }
    item.complete();
  }

  function flush() {
    frame = 0;
    const batch = pending;
    pending = [];
    if (!batch.length) return;

    const measured = [];
    batch.forEach(function(item) {
      const rect = item.entry.directHost.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        measured.push({ item: item, rect: rect });
      } else {
        item.entry.directHost.style.cssText = item.style;
        item.complete();
      }
    });
    if (!measured.length) return;

    const left = Math.min.apply(null, measured.map(function(value) { return value.rect.left; }));
    const top = Math.min.apply(null, measured.map(function(value) { return value.rect.top; }));
    const right = Math.max.apply(null, measured.map(function(value) { return value.rect.right; }));
    const bottom = Math.max.apply(null, measured.map(function(value) { return value.rect.bottom; }));
    const layer = document.createElement('div');
    layer.className = 'source-replacement-reveal-batch';
    layer.style.cssText = 'position:fixed;pointer-events:none;overflow:visible;will-change:opacity,filter,transform;'
      + 'left:' + left + 'px;top:' + top + 'px;width:' + Math.max(1, right - left)
      + 'px;height:' + Math.max(1, bottom - top) + 'px;z-index:' + (highestStackOrder + 1) + ';';
    scene.appendChild(layer);

    const moved = measured.map(function(value) {
      const item = value.item;
      const host = item.entry.directHost;
      const placeholder = document.createComment('source-replacement-reveal');
      host.parentNode.insertBefore(placeholder, host);
      const opacity = getComputedStyle(item.entry.card).opacity;
      layer.appendChild(host);
      host.style.cssText += ';position:absolute;display:block;pointer-events:auto;visibility:visible;'
        + 'left:' + (value.rect.left - left) + 'px;top:' + (value.rect.top - top)
        + 'px;width:' + value.rect.width + 'px;height:' + value.rect.height
        + 'px;opacity:' + opacity + ';';
      return {
        host: host,
        placeholder: placeholder,
        style: item.style,
        complete: item.complete
      };
    });

    let finished = false;
    function finish() {
      if (finished) return;
      finished = true;
      moved.forEach(restore);
      layer.remove();
    }
    if (typeof layer.animate !== 'function') {
      requestAnimationFrame(function() { requestAnimationFrame(finish); });
      return;
    }
    const animation = layer.animate([
      { opacity: 0, filter: 'blur(8px)', transform: 'translate3d(0,4px,0)' },
      { opacity: 1, filter: 'blur(0)', transform: 'translate3d(0,0,0)' }
    ], { duration: 350, easing: 'cubic-bezier(0.2,0,0.2,1)', fill: 'both' });
    animation.addEventListener('finish', finish, { once: true });
    animation.addEventListener('cancel', finish, { once: true });
  }

  function enqueue(entry, complete) {
    const style = entry.directHost.style.cssText;
    entry.directHost.style.visibility = 'hidden';
    pending.push({ entry: entry, style: style, complete: complete });
    if (!frame) frame = requestAnimationFrame(flush);
  }

  return { enqueue: enqueue };
})();

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
  reportPendingPaint();
  return true;
}
