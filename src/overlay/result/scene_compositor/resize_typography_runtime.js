(function() {
  function numberStyle(body, name, fallback) {
    const inline = parseFloat(body.style[name]);
    if (Number.isFinite(inline)) return inline;
    const computed = parseFloat(getComputedStyle(body)[name]);
    return Number.isFinite(computed) ? computed : fallback;
  }

  function cancelMotion(fitState, cancelFrame) {
    if (fitState._sgtFitAnim) {
      try { cancelFrame(fitState._sgtFitAnim); } catch (_error) {}
      fitState._sgtFitAnim = null;
    }
    const motion = fitState._sgtMotionController;
    if (motion && motion.frame !== null) {
      try { cancelFrame(motion.frame); } catch (_error) {}
    }
    fitState._sgtMotionController = null;
  }

  function rebase(snapshot, size) {
    const body = snapshot.body;
    const fontSize = numberStyle(body, 'fontSize', 14);
    const lineHeight = numberStyle(body, 'lineHeight', fontSize * 1.15);
    const paddingTop = numberStyle(body, 'paddingTop', 0);
    const paddingBottom = numberStyle(body, 'paddingBottom', 0);
    const contentHeight = Math.max(lineHeight,
      body.scrollHeight - paddingTop - paddingBottom);
    snapshot.width = Math.max(1, size.width);
    snapshot.height = Math.max(1, size.height);
    snapshot.fontSize = fontSize;
    snapshot.contentHeight = contentHeight;
    snapshot.lineCount = Math.max(1, contentHeight / Math.max(1, lineHeight));
    snapshot.fontStretch = numberStyle(body, 'fontStretch', 90);
    snapshot.letterSpacing = numberStyle(body, 'letterSpacing', 0);
    snapshot.lastAppliedFontSize = fontSize;
    snapshot.lastAppliedPaddingTop = paddingTop;
    snapshot.lastAppliedPaddingBottom = paddingBottom;
  }

  function begin(body, fitState, size, cancelFrame) {
    if (!body || !fitState || typeof cancelFrame !== 'function') return null;
    const text = (body.innerText || body.textContent || '').trim();
    if (!text) return null;
    cancelMotion(fitState, cancelFrame);
    const snapshot = {
      body: body,
      fitState: fitState,
      cancelFrame: cancelFrame,
      textLength: text.length,
      lastSize: { width: size.width, height: size.height }
    };
    rebase(snapshot, size);
    return snapshot;
  }

  function canonicalAxesChanged(snapshot) {
    const body = snapshot.body;
    return Math.abs(numberStyle(body, 'fontSize', snapshot.lastAppliedFontSize)
        - snapshot.lastAppliedFontSize) > 0.05
      || Math.abs(numberStyle(body, 'paddingTop', snapshot.lastAppliedPaddingTop)
        - snapshot.lastAppliedPaddingTop) > 0.05
      || Math.abs(numberStyle(body, 'paddingBottom', snapshot.lastAppliedPaddingBottom)
        - snapshot.lastAppliedPaddingBottom) > 0.05
      || Math.abs(numberStyle(body, 'fontStretch', snapshot.fontStretch)
        - snapshot.fontStretch) > 0.1
      || Math.abs(numberStyle(body, 'letterSpacing', snapshot.letterSpacing)
        - snapshot.letterSpacing) > 0.05;
  }

  function preview(snapshot, size) {
    if (!snapshot) return;
    if (canonicalAxesChanged(snapshot)) {
      cancelMotion(snapshot.fitState, snapshot.cancelFrame);
      rebase(snapshot, snapshot.lastSize);
    }
    const widthRatio = Math.max(0.01, size.width / snapshot.width);
    const heightRatio = Math.max(0.01, size.height / snapshot.height);
    const areaScale = Math.sqrt(widthRatio * heightRatio);
    const axisScale = Math.min(widthRatio, heightRatio);
    const multilineWeight = Math.max(0, Math.min(1, (snapshot.lineCount - 1) / 4));
    const scale = axisScale + (areaScale - axisScale) * multilineWeight;
    const minimum = snapshot.textLength < 200 ? 6 : 14;
    const maximum = snapshot.textLength < 300
      ? 200
      : (snapshot.textLength < 1500
        ? 100
        : Math.max(24, Math.min(48, Math.floor(size.height / 10))));
    const fontSize = Math.max(minimum, Math.min(maximum, snapshot.fontSize * scale));
    const wrappedHeight = snapshot.contentHeight * scale * scale / widthRatio;
    const linearHeight = snapshot.contentHeight * scale;
    const predictedHeight = linearHeight
      + (wrappedHeight - linearHeight) * multilineWeight;
    const gap = Math.max(0, size.height - predictedHeight);
    const paddingTop = Math.floor(gap * 0.3);
    const paddingBottom = Math.floor(gap * 0.7);
    snapshot.body.style.fontSize = fontSize + 'px';
    snapshot.body.style.paddingTop = paddingTop + 'px';
    snapshot.body.style.paddingBottom = paddingBottom + 'px';
    snapshot.fitState._sgtCurrentFontSize = fontSize;
    snapshot.lastAppliedFontSize = fontSize;
    snapshot.lastAppliedPaddingTop = paddingTop;
    snapshot.lastAppliedPaddingBottom = paddingBottom;
    snapshot.lastSize = { width: size.width, height: size.height };
  }

  window.__SGT_TYPOGRAPHY_RESIZE__ = { begin: begin, preview: preview };
})();
