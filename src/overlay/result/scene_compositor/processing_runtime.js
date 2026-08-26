window.__SGT_CREATE_PROCESSING_AURA__ = function() {
  const namespace = 'http://www.w3.org/2000/svg';
  window.__SGT_PROCESSING_GRADIENT_ID__ = (window.__SGT_PROCESSING_GRADIENT_ID__ || 0) + 1;
  const gradientId = 'sgt-processing-gradient-' + window.__SGT_PROCESSING_GRADIENT_ID__;
  const element = document.createElementNS(namespace, 'svg');
  element.classList.add('processing-aura');
  element.setAttribute('aria-hidden', 'true');
  element.setAttribute('preserveAspectRatio', 'none');
  const definitions = document.createElementNS(namespace, 'defs');
  const gradient = document.createElementNS(namespace, 'linearGradient');
  gradient.id = gradientId;
  gradient.setAttribute('gradientUnits', 'userSpaceOnUse');
  for (const [offset, color] of [['0%', '#55dcff'], ['28%', '#8870ff'],
    ['55%', '#ff5cc8'], ['78%', '#ffb340'], ['100%', '#55dcff']]) {
    const stop = document.createElementNS(namespace, 'stop');
    stop.setAttribute('offset', offset); stop.setAttribute('stop-color', color);
    gradient.appendChild(stop);
  }
  const motion = document.createElementNS(namespace, 'animateTransform');
  motion.setAttribute('attributeName', 'gradientTransform');
  motion.setAttribute('type', 'rotate');
  motion.setAttribute('dur', '1.8s');
  motion.setAttribute('repeatCount', 'indefinite');
  motion.setAttribute('begin', 'indefinite');
  gradient.appendChild(motion);
  definitions.appendChild(gradient); element.appendChild(definitions);
  const track = document.createElementNS(namespace, 'rect');
  track.classList.add('processing-track');
  track.setAttribute('pathLength', '100');
  element.appendChild(track);
  for (const className of ['processing-runner-glow', 'processing-runner']) {
    const runner = document.createElementNS(namespace, 'rect');
    runner.classList.add(className);
    runner.setAttribute('pathLength', '100');
    runner.setAttribute('stroke', 'url(#' + gradientId + ')');
    element.appendChild(runner);
  }
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let active = false; let running = false;
  function syncMotion() {
    const shouldRun = active && !reducedMotion.matches;
    if (shouldRun === running) return;
    running = shouldRun;
    if (shouldRun) motion.beginElement();
    else motion.endElement();
  }
  function reducedMotionChanged() { syncMotion(); }
  reducedMotion.addEventListener('change', reducedMotionChanged);
  function resize(width, height, scale) {
    const stroke = Math.max(1, 2 / scale);
    const edge = stroke;
    const radius = Math.min(8 / scale, width / 2, height / 2);
    const centerX = width / 2; const centerY = height / 2;
    const halfSpan = Math.hypot(width, height) / 2;
    element.setAttribute('viewBox', '0 0 ' + width + ' ' + height);
    gradient.setAttribute('x1', String(centerX - halfSpan));
    gradient.setAttribute('y1', String(centerY));
    gradient.setAttribute('x2', String(centerX + halfSpan));
    gradient.setAttribute('y2', String(centerY));
    motion.setAttribute('from', '0 ' + centerX + ' ' + centerY);
    motion.setAttribute('to', '360 ' + centerX + ' ' + centerY);
    for (const rect of element.querySelectorAll('rect')) {
      rect.setAttribute('x', String(edge));
      rect.setAttribute('y', String(edge));
      rect.setAttribute('width', String(Math.max(0, width - edge * 2)));
      rect.setAttribute('height', String(Math.max(0, height - edge * 2)));
      rect.setAttribute('rx', String(radius));
      rect.setAttribute('ry', String(radius));
      rect.setAttribute('stroke-width', String(stroke));
    }
    if (running) {
      motion.endElement(); motion.beginElement();
    }
  }
  return {
    element: element,
    resize: resize,
    setState: function(nextActive) {
      active = nextActive; syncMotion();
    },
    destroy: function() {
      active = false; syncMotion();
      reducedMotion.removeEventListener('change', reducedMotionChanged);
    }
  };
};
