function applyGeometry(entry, model) {
  const preservePosition = window.shouldPreserveResultDragGeometry?.(entry.card.dataset.id) === true;
  const scale = window.devicePixelRatio || 1; entry.card.style.setProperty('--sgt-box-radius', (__SGT_BOX_RADIUS_PX__ / scale) + 'px');
  const width = model.rect.width / scale;
  const height = model.rect.height / scale;
  const widthCss = width + 'px';
  const heightCss = height + 'px';
  const resized = entry.card.style.width !== widthCss || entry.card.style.height !== heightCss;
  if (!preservePosition) {
    entry.card.style.translate = '';
    entry.card.style.transform = 'translate3d(' + (model.rect.x / scale) + 'px,' +
      (model.rect.y / scale) + 'px,0)';
  }
  entry.card.style.width = widthCss;
  entry.card.style.height = heightCss;
  syncSourceBackdrop(entry);
  entry.processing.resize(width, height, scale);
  if (resized && entry.ready && entry.visible) {
    clearTimeout(entry.resizeFit);
    entry.resizeFit = setTimeout(function() { queueFit(entry, entry.streaming); }, 40);
  }
}

function activateCard(entry, becameVisible) {
  if (!entry.visible || entry.navigationDepth !== 0) return;
  if (entry.mode === 'direct' && entry.pendingContent) {
    // Terminal content supersedes the streaming batch; do not wait behind its frame.
    if (entry.pendingContent.type === 'finalize' && !entry.sourceReplacement) {
      if (entry.contentFrame) cancelAnimationFrame(entry.contentFrame);
      entry.contentFrame = null;
      flushPendingContent(entry);
      return;
    }
    if (!entry.contentFrame) entry.contentFrame = requestAnimationFrame(function() {
      entry.contentFrame = null;
      if (cards.get(entry.card.dataset.id) !== entry) return;
      if (entry.navigationDepth === 0) flushPendingContent(entry);
    });
    return;
  }
  if (flushPendingContent(entry)) return;
  if (becameVisible && entry.ready) queueFit(entry, entry.streaming);
}
function removeCard(id) {
  const key = String(id);
  const entry = cards.get(key);
  if (!entry) return;
  window.cancelResultDragForCard?.(key);
  cancelActiveFit(entry);
  if (entry.contentFrame) cancelAnimationFrame(entry.contentFrame);
  clearTimeout(entry.resizeFit);
  entry.directRuntime.destroy();
  entry.resizeRuntime.destroy();
  entry.processing.destroy();
  sourceReplacementReveal.cancel(entry);
  if (entry.commandPort) entry.commandPort.close();
  entry.sourceBackdropSurface?.remove();
  entry.card.remove();
  cards.delete(key);
}
function applyTheme(theme) {
  currentThemeCss = String(theme.css || '');
  document.getElementById('sgt-theme-css').textContent = currentThemeCss;
  for (const appearance of theme.cards || []) {
    const entry = cards.get(String(appearance.id));
    if (!entry) continue;
    entry.card.style.background = appearance.background;
    postCardMessage(entry, { type: 'theme_update', css: currentThemeCss });
  }
}
