function removeCard(id) {
  const key = String(id);
  const entry = cards.get(key);
  if (!entry) return;
  window.cancelResultDragForCard?.(key);
  cancelActiveFit(entry);
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
