function activateCard(entry, becameVisible) {
  if (!entry.visible || entry.navigationDepth !== 0) return;
  if (entry.mode === 'direct' && entry.pendingContent) {
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
  if (entry.commandPort) entry.commandPort.close();
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
