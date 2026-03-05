import type { KeystrokeEvent, VideoSegment } from '@/types/video';
import {
  type KeystrokeState,
  type KeystrokeRenderCache,
  type KeystrokeBubbleLayout,
  type KeystrokeVisualState,
  type KeystrokeOverlayTransform,
  type KeystrokeOverlayEditBounds,
  type ActiveKeystrokeEvent,
  type ActiveKeystrokeLanes,
  type KeystrokeLaneRenderItem,
  type KeystrokeLanePlacement,
  type ActiveKeystrokeFrameLayout,
  KEYSTROKE_SLOT_SPARSE_GAP_LIMIT,
  DEFAULT_KEYSTROKE_DELAY_SEC,
  DEFAULT_KEYSTROKE_OVERLAY_X,
  DEFAULT_KEYSTROKE_OVERLAY_Y,
  DEFAULT_KEYSTROKE_OVERLAY_SCALE,
  KEYSTROKE_OVERLAY_MIN_SCALE,
  KEYSTROKE_OVERLAY_MAX_SCALE,
  clamp01,
  computePercentile,
  getKeystrokeLabel,
  getKeystrokeVisualState,
  applyKeystrokeFontVariations,
  upperBound,
  isTimeInsideSegments,
  rebuildKeystrokeRenderCache,
} from './keystrokeTypes';

// --- EVENT LOOKUP ---

export function getKeystrokeDelaySec(segment: VideoSegment): number {
  const raw = segment.keystrokeDelaySec;
  if (typeof raw !== 'number' || Number.isNaN(raw)) return DEFAULT_KEYSTROKE_DELAY_SEC;
  return Math.max(-1, Math.min(1, raw));
}

export function getKeystrokeOverlayConfig(segment: VideoSegment): { x: number; y: number; scale: number } {
  const raw = segment.keystrokeOverlay;
  return {
    x: typeof raw?.x === 'number' ? Math.max(0, Math.min(100, raw.x)) : DEFAULT_KEYSTROKE_OVERLAY_X,
    y: typeof raw?.y === 'number' ? Math.max(0, Math.min(100, raw.y)) : DEFAULT_KEYSTROKE_OVERLAY_Y,
    scale: typeof raw?.scale === 'number' && Number.isFinite(raw.scale)
      ? Math.max(KEYSTROKE_OVERLAY_MIN_SCALE, Math.min(KEYSTROKE_OVERLAY_MAX_SCALE, raw.scale))
      : DEFAULT_KEYSTROKE_OVERLAY_SCALE,
  };
}

export function getKeystrokeOverlayTransform(
  segment: VideoSegment,
  canvasWidth: number,
  canvasHeight: number
): KeystrokeOverlayTransform {
  const overlay = getKeystrokeOverlayConfig(segment);
  return {
    anchorXPx: (overlay.x / 100) * canvasWidth,
    baselineYPx: (overlay.y / 100) * canvasHeight,
    scale: overlay.scale,
  };
}

export function getDelayedKeystrokeRange(
  startTime: number,
  endTime: number,
  delaySec: number
): { startTime: number; endTime: number } {
  return {
    startTime: startTime + delaySec,
    endTime: endTime + delaySec,
  };
}

export function findActiveKeystrokeEventsForKind(
  cache: KeystrokeRenderCache,
  currentTime: number,
  delaySec: number,
  kind: 'keyboard' | 'mouse'
): ActiveKeystrokeEvent[] {
  const startTimes = kind === 'keyboard' ? cache.keyboardStartTimes : cache.mouseStartTimes;
  const indices = kind === 'keyboard' ? cache.keyboardIndices : cache.mouseIndices;
  if (!startTimes.length || !indices.length) return [];
  const idx = upperBound(startTimes, currentTime - delaySec) - 1;
  if (idx < 0) return [];
  const maxDuration = kind === 'keyboard' ? cache.keyboardMaxDuration : cache.mouseMaxDuration;
  const minStartCandidate = currentTime - delaySec - maxDuration - 0.000001;
  const active: ActiveKeystrokeEvent[] = [];
  const seenIdentities = new Set<string>();
  for (let cursor = idx; cursor >= 0; cursor--) {
    const eventIndex = indices[cursor];
    const event = cache.displayEvents[eventIndex];
    if (event.startTime < minStartCandidate) break;
    const delayed = getDelayedKeystrokeRange(event.startTime, cache.effectiveEnds[eventIndex], delaySec);
    if (currentTime < delayed.startTime || currentTime > delayed.endTime) continue;
    const identity = cache.eventIdentities[eventIndex];
    if (seenIdentities.has(identity)) continue;
    seenIdentities.add(identity);
    active.push({
      event,
      startTime: delayed.startTime,
      endTime: delayed.endTime,
      slot: cache.eventSlots[eventIndex],
      identity,
    });
  }
  active.sort((a, b) => {
    if (a.slot !== b.slot) return a.slot - b.slot;
    return b.startTime - a.startTime;
  });
  return active;
}

export function findActiveKeystrokeEvents(
  state: KeystrokeState,
  segment: VideoSegment,
  currentTime: number,
  duration: number
): ActiveKeystrokeLanes | null {
  const cache = rebuildKeystrokeRenderCache(state, segment, duration);
  if (!cache) return null;
  const visibilitySegments = cache.visibilityRef ?? [];
  if (!visibilitySegments.length) return null;
  const delaySec = getKeystrokeDelaySec(segment);
  if (!isTimeInsideSegments(currentTime, visibilitySegments)) return null;
  const keyboard = findActiveKeystrokeEventsForKind(cache, currentTime, delaySec, 'keyboard');
  const mouse = cache.mode === 'keyboardMouse'
    ? findActiveKeystrokeEventsForKind(cache, currentTime, delaySec, 'mouse')
    : [];

  if (!keyboard.length && !mouse.length) return null;
  return {
    keyboard,
    mouse,
  };
}

// --- LAYOUT ---

export function measureKeystrokeBubble(
  state: KeystrokeState,
  ctx: CanvasRenderingContext2D,
  event: KeystrokeEvent,
  canvasHeight: number,
  overlayScale: number
): KeystrokeBubbleLayout {
  const isMouse = event.type === 'mousedown' || event.type === 'wheel';
  const label = getKeystrokeLabel(state, event);
  const safeScale = Math.max(KEYSTROKE_OVERLAY_MIN_SCALE, Math.min(KEYSTROKE_OVERLAY_MAX_SCALE, overlayScale));
  const fontSize = Math.round(Math.max(16, Math.min(38, canvasHeight * 0.036)) * safeScale);
  const paddingX = Math.round(fontSize * 0.24);
  const paddingY = Math.round(fontSize * 0.38);
  const radius = Math.round(fontSize * 0.64);
  const marginBottom = Math.round(Math.max(14, canvasHeight * 0.06));
  const KEY_ICON_MAP: Record<string, string> = {
    'Space': 'space', 'Enter': 'enter', 'Backspace': 'backspace',
    'Tab': 'tab', 'Delete': 'delete', 'CapsLock': 'capslock', 'Shift': 'shift',
  };
  // Icons for non-combo single keys only (combos like "Ctrl + Enter" stay text-only)
  const keyIcon = (!isMouse && !event.label.includes(' + '))
    ? (KEY_ICON_MAP[event.label] ?? null)
    : null;
  const showMouseIcon = isMouse;
  const iconBoxWidth = showMouseIcon
    ? Math.round(fontSize * 0.78)
    : (keyIcon ? Math.round(fontSize * 0.72) : 0);
  const iconGap = (showMouseIcon || keyIcon) ? Math.round(fontSize * 0.16) : 0;

  ctx.save();
  const originalVariations = ctx.canvas.style.fontVariationSettings;
  applyKeystrokeFontVariations(ctx, {
    alpha: 1,
    scale: 1,
    scaleX: 1,
    scaleY: 1,
    translateY: 0,
    wdth: 100,
    wght: 600,
    slnt: isMouse ? -6 : 0,
    rond: isMouse ? 96 : 88,
    holdMix: 0,
    laneWeight: 1,
  });
  ctx.font = `600 ${fontSize}px 'Google Sans Flex', sans-serif`;
  const textWidth = ctx.measureText(label).width;
  ctx.canvas.style.fontVariationSettings = originalVariations || 'normal';
  ctx.restore();

  const height = Math.ceil(fontSize + paddingY * 2);
  const rawWidth = textWidth + iconBoxWidth + iconGap + paddingX * 2;
  const minLabelWidth = (showMouseIcon || keyIcon)
    ? (iconBoxWidth + iconGap + height)
    : (height * 1.06); // Enforce square-like identical widths for single character keys like E and W

  return {
    label,
    showMouseIcon,
    keyIcon,
    iconBoxWidth,
    iconGap,
    fontSize,
    paddingX,
    paddingY,
    radius,
    marginBottom,
    width: Math.ceil(Math.max(rawWidth, minLabelWidth)),
    height,
  };
}

export function getCachedKeystrokeBubbleLayout(
  state: KeystrokeState,
  ctx: CanvasRenderingContext2D,
  event: KeystrokeEvent,
  canvasHeight: number,
  overlayScale: number
): KeystrokeBubbleLayout {
  const cacheKey = `${event.id}@${Math.round(canvasHeight)}@${overlayScale.toFixed(3)}@${state.keystrokeLanguage}`;
  const cached = state.layoutCache.get(cacheKey);
  if (cached) return cached;
  const measured = measureKeystrokeBubble(state, ctx, event, canvasHeight, overlayScale);
  state.layoutCache.set(cacheKey, measured);
  return measured;
}

export function getKeystrokeBubbleWidthForVisual(
  ctx: CanvasRenderingContext2D,
  layout: KeystrokeBubbleLayout,
  visual: KeystrokeVisualState
): number {
  // Rely exclusively on static layout width to eliminate E vs W jitter and export padding offsets
  void ctx; void visual;
  return layout.width;
}

export function buildKeystrokeLaneRenderItems(
  state: KeystrokeState,
  ctx: CanvasRenderingContext2D,
  laneEvents: ActiveKeystrokeEvent[],
  currentTime: number,
  canvasHeight: number,
  overlayScale: number
): KeystrokeLaneRenderItem[] {
  const items: KeystrokeLaneRenderItem[] = [];
  for (const active of laneEvents) {
    const layout = getCachedKeystrokeBubbleLayout(state, ctx, active.event, canvasHeight, overlayScale);
    const visual = getKeystrokeVisualState(
      currentTime,
      active.startTime,
      active.endTime,
      active.event.type,
      Boolean(active.event.isHold)
    );
    if (visual.alpha <= 0.001) continue;
    const bubbleWidth = getKeystrokeBubbleWidthForVisual(ctx, layout, visual);
    items.push({
      active,
      layout,
      visual,
      bubbleWidth,
    });
  }
  items.sort((a, b) => {
    if (a.active.slot !== b.active.slot) return a.active.slot - b.active.slot;
    return b.active.startTime - a.active.startTime;
  });
  return items;
}

export function getKeystrokePairGapPx(primaryFontSize: number, secondaryFontSize: number): number {
  return Math.round(Math.max(14, Math.max(primaryFontSize, secondaryFontSize) * 0.58));
}

export function getKeystrokeBarrierGapPx(
  layoutA: KeystrokeBubbleLayout | null,
  layoutB: KeystrokeBubbleLayout | null
): number {
  const fontA = layoutA?.fontSize ?? 16;
  const fontB = layoutB?.fontSize ?? 16;
  return getKeystrokePairGapPx(fontA, fontB);
}

export function getKeystrokeLaneBubbleGapPx(fontSize: number): number {
  return Math.round(Math.max(8, fontSize * 0.22));
}

export function getKeystrokeSlotWidthHints(
  state: KeystrokeState,
  ctx: CanvasRenderingContext2D,
  cache: KeystrokeRenderCache,
  kind: 'keyboard' | 'mouse',
  canvasHeight: number,
  laneItems: KeystrokeLaneRenderItem[],
  overlayScale: number
): number[] {
  const representatives = kind === 'keyboard'
    ? cache.keyboardSlotRepresentatives
    : cache.mouseSlotRepresentatives;
  const slotWidths: number[] = new Array(representatives.length);
  const measuredWidths: number[] = [];

  for (let slot = 0; slot < representatives.length; slot++) {
    const eventIndex = representatives[slot];
    if (typeof eventIndex !== 'number') continue;
    const event = cache.displayEvents[eventIndex];
    if (!event) continue;
    const layout = getCachedKeystrokeBubbleLayout(state, ctx, event, canvasHeight, overlayScale);
    slotWidths[slot] = layout.width;
    measuredWidths.push(layout.width);
  }

  const laneAverage = laneItems.length > 0
    ? laneItems.reduce((sum, item) => sum + item.bubbleWidth, 0) / laneItems.length
    : 0;
  const medianWidth = measuredWidths.length > 0
    ? computePercentile(measuredWidths, 0.5)
    : 0;
  const fallbackWidth = medianWidth > 0
    ? medianWidth
    : (laneAverage > 0 ? laneAverage : Math.max(110, canvasHeight * 0.09));
  const minWidth = Math.max(36, fallbackWidth * 0.56);
  const maxWidth = Math.max(minWidth, fallbackWidth * 1.24);

  for (let slot = 0; slot < slotWidths.length; slot++) {
    const rawWidth = (typeof slotWidths[slot] === 'number' && !Number.isNaN(slotWidths[slot]))
      ? slotWidths[slot]
      : fallbackWidth;
    slotWidths[slot] = Math.max(minWidth, Math.min(maxWidth, rawWidth));
  }

  return slotWidths;
}

export function getKeystrokeSlotAdvancePx(
  slotIndex: number,
  bubbleGap: number,
  slotWidthHints: number[],
  fallbackSlotWidth: number
): number {
  const slotWidth = slotWidthHints[slotIndex] ?? fallbackSlotWidth;
  const fullAdvance = slotWidth + bubbleGap;
  if (slotIndex < KEYSTROKE_SLOT_SPARSE_GAP_LIMIT) {
    return fullAdvance;
  }
  // Preserve stickiness but compress far slots to avoid runaway width.
  const tailOffset = slotIndex - KEYSTROKE_SLOT_SPARSE_GAP_LIMIT + 1;
  return fullAdvance * (0.64 / Math.sqrt(tailOffset));
}

export function layoutKeystrokeLane(
  laneItems: KeystrokeLaneRenderItem[],
  canvasWidth: number,
  _canvasHeight: number,
  laneGapPx: number,
  align: 'left' | 'right',
  slotWidthHints: number[],
  anchorXPx: number,
  baselineYPx: number
): KeystrokeLanePlacement[] {
  if (!laneItems.length) return [];
  const maxFont = laneItems.reduce((max, item) => Math.max(max, item.layout.fontSize), 16);
  const marginX = Math.round(Math.max(10, maxFont * 0.34));
  const leftBound = marginX;
  const rightBound = Math.max(leftBound, canvasWidth - marginX);
  const barrierX = anchorXPx;
  const centerAnchor = align === 'right'
    ? barrierX - laneGapPx * 0.5
    : barrierX + laneGapPx * 0.5;
  const bubbleGap = getKeystrokeLaneBubbleGapPx(maxFont);
  const hintAverage = slotWidthHints.length > 0
    ? (slotWidthHints.reduce((sum, width) => sum + width, 0) / slotWidthHints.length)
    : (laneItems.reduce((sum, item) => sum + item.bubbleWidth, 0) / laneItems.length);
  const maxActiveSlot = laneItems.reduce((max, item) => Math.max(max, item.active.slot), 0);
  const maxHintSlot = Math.max(0, slotWidthHints.length - 1);
  const maxSlot = Math.max(maxActiveSlot, maxHintSlot);
  const slotOffsets: number[] = new Array(maxSlot + 1).fill(0);
  for (let slot = 1; slot <= maxSlot; slot++) {
    slotOffsets[slot] = slotOffsets[slot - 1] + getKeystrokeSlotAdvancePx(
      slot - 1,
      bubbleGap,
      slotWidthHints,
      hintAverage
    );
  }
  const maxRawOffset = slotOffsets[maxSlot] ?? 0;
  const maxSpreadPx = Math.round(Math.max(150, Math.min(canvasWidth * 0.24, 290)));
  if (maxRawOffset > maxSpreadPx) {
    const preserveSlot = Math.min(2, maxSlot);
    const preservePx = slotOffsets[preserveSlot] ?? 0;
    const compressibleRaw = Math.max(0, maxRawOffset - preservePx);
    const compressibleTarget = Math.max(0, maxSpreadPx - preservePx);
    const compression = compressibleRaw > 0
      ? clamp01(compressibleTarget / compressibleRaw)
      : 1;
    for (let slot = preserveSlot + 1; slot <= maxSlot; slot++) {
      const raw = slotOffsets[slot];
      slotOffsets[slot] = preservePx + ((raw - preservePx) * compression);
    }
  }

  const placements: KeystrokeLanePlacement[] = laneItems.map((item) => {
    const y = Math.round(baselineYPx - item.layout.height - item.layout.marginBottom);
    const slotOffset = slotOffsets[item.active.slot] ?? 0;
    if (align === 'right') {
      const rightEdge = centerAnchor - slotOffset;
      return {
        item,
        x: rightEdge - item.bubbleWidth,
        y,
        align,
      };
    }
    const leftEdge = centerAnchor + slotOffset;
    return {
      item,
      x: leftEdge,
      y,
      align,
    };
  });

  if (align === 'right') {
    for (let i = 1; i < placements.length; i++) {
      const prev = placements[i - 1];
      const cur = placements[i];
      const pairGap = getKeystrokeLaneBubbleGapPx(
        Math.max(prev.item.layout.fontSize, cur.item.layout.fontSize)
      );
      const maxAllowedX = prev.x - pairGap - cur.item.bubbleWidth;
      if (cur.x > maxAllowedX) {
        cur.x = maxAllowedX;
      }
    }
    const leftMost = placements[placements.length - 1];
    const overflow = leftBound - leftMost.x;
    if (overflow > 0.001) {
      let maxShift = Number.POSITIVE_INFINITY;
      for (const placement of placements) {
        const placementMaxX = Math.max(leftBound, rightBound - placement.item.bubbleWidth);
        maxShift = Math.min(maxShift, placementMaxX - placement.x);
      }
      const shift = Math.max(0, Math.min(overflow, maxShift));
      if (shift > 0.001) {
        for (const placement of placements) {
          placement.x += shift;
        }
      }
    }
  } else {
    for (let i = 1; i < placements.length; i++) {
      const prev = placements[i - 1];
      const cur = placements[i];
      const pairGap = getKeystrokeLaneBubbleGapPx(
        Math.max(prev.item.layout.fontSize, cur.item.layout.fontSize)
      );
      const minAllowedX = prev.x + prev.item.bubbleWidth + pairGap;
      if (cur.x < minAllowedX) {
        cur.x = minAllowedX;
      }
    }
    const rightMost = placements[placements.length - 1];
    const rightMostEdge = rightMost.x + rightMost.item.bubbleWidth;
    const overflow = rightMostEdge - rightBound;
    if (overflow > 0.001) {
      let maxLeftShift = Number.POSITIVE_INFINITY;
      for (const placement of placements) {
        maxLeftShift = Math.min(maxLeftShift, placement.x - leftBound);
      }
      const shift = Math.max(0, Math.min(overflow, maxLeftShift));
      if (shift > 0.001) {
        for (const placement of placements) {
          placement.x -= shift;
        }
      }
    }
  }

  for (const placement of placements) {
    const maxX = Math.max(leftBound, rightBound - placement.item.bubbleWidth);
    placement.x = Math.round(Math.max(leftBound, Math.min(maxX, placement.x)));
  }

  return placements;
}

export function getKeystrokeOverlayBoundsFromPlacements(
  keyboardPlacements: KeystrokeLanePlacement[],
  mousePlacements: KeystrokeLanePlacement[]
): { x: number; y: number; width: number; height: number } | null {
  const all = [...keyboardPlacements, ...mousePlacements];
  if (!all.length) return null;
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const placement of all) {
    minX = Math.min(minX, placement.x);
    minY = Math.min(minY, placement.y);
    maxX = Math.max(maxX, placement.x + placement.item.bubbleWidth);
    maxY = Math.max(maxY, placement.y + placement.item.layout.height);
  }
  if (!Number.isFinite(minX) || !Number.isFinite(minY) || !Number.isFinite(maxX) || !Number.isFinite(maxY)) {
    return null;
  }
  return {
    x: minX,
    y: minY,
    width: Math.max(1, maxX - minX),
    height: Math.max(1, maxY - minY),
  };
}

export function buildActiveKeystrokeFrameLayout(
  state: KeystrokeState,
  ctx: CanvasRenderingContext2D,
  segment: VideoSegment,
  cache: KeystrokeRenderCache,
  sampleTime: number,
  delaySec: number,
  canvasWidth: number,
  canvasHeight: number
): ActiveKeystrokeFrameLayout {
  const visibilitySegments = cache.visibilityRef ?? [];
  if (!visibilitySegments.length || !isTimeInsideSegments(sampleTime, visibilitySegments)) {
    return {
      keyboard: [],
      mouse: [],
    };
  }
  const overlayTransform = getKeystrokeOverlayTransform(segment, canvasWidth, canvasHeight);
  const keyboard = findActiveKeystrokeEventsForKind(cache, sampleTime, delaySec, 'keyboard');
  const mouse = cache.mode === 'keyboardMouse'
    ? findActiveKeystrokeEventsForKind(cache, sampleTime, delaySec, 'mouse')
    : [];
  const keyboardItems = buildKeystrokeLaneRenderItems(state, ctx, keyboard, sampleTime, canvasHeight, overlayTransform.scale);
  const mouseItems = buildKeystrokeLaneRenderItems(state, ctx, mouse, sampleTime, canvasHeight, overlayTransform.scale);
  const laneGapPx = getKeystrokeBarrierGapPx(
    keyboardItems[0]?.layout ?? null,
    mouseItems[0]?.layout ?? null
  );
  const keyboardSlotWidths = getKeystrokeSlotWidthHints(
    state,
    ctx,
    cache,
    'keyboard',
    canvasHeight,
    keyboardItems,
    overlayTransform.scale
  );
  const mouseSlotWidths = getKeystrokeSlotWidthHints(
    state,
    ctx,
    cache,
    'mouse',
    canvasHeight,
    mouseItems,
    overlayTransform.scale
  );
  return {
    keyboard: layoutKeystrokeLane(
      keyboardItems,
      canvasWidth,
      canvasHeight,
      laneGapPx,
      'right',
      keyboardSlotWidths,
      overlayTransform.anchorXPx,
      overlayTransform.baselineYPx
    ),
    mouse: layoutKeystrokeLane(
      mouseItems,
      canvasWidth,
      canvasHeight,
      laneGapPx,
      'left',
      mouseSlotWidths,
      overlayTransform.anchorXPx,
      overlayTransform.baselineYPx
    ),
  };
}

export function getKeystrokeOverlayEditBounds(
  state: KeystrokeState,
  segment: VideoSegment,
  canvas: HTMLCanvasElement,
  currentTime: number,
  duration: number
): KeystrokeOverlayEditBounds | null {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'off') return null;
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  const safeDuration = Math.max(0, duration);
  const cache = rebuildKeystrokeRenderCache(state, segment, safeDuration);
  if (!cache) return null;
  const visibilitySegments = cache.visibilityRef ?? [];
  if (!visibilitySegments.length) return null;
  if (!isTimeInsideSegments(currentTime, visibilitySegments)) return null;
  const overlayTransform = getKeystrokeOverlayTransform(segment, canvas.width, canvas.height);
  const delaySec = getKeystrokeDelaySec(segment);
  const keyboard = findActiveKeystrokeEventsForKind(cache, currentTime, delaySec, 'keyboard');
  const mouse = mode === 'keyboardMouse'
    ? findActiveKeystrokeEventsForKind(cache, currentTime, delaySec, 'mouse')
    : [];
  const keyboardItems = buildKeystrokeLaneRenderItems(state, ctx, keyboard, currentTime, canvas.height, overlayTransform.scale);
  const mouseItems = buildKeystrokeLaneRenderItems(state, ctx, mouse, currentTime, canvas.height, overlayTransform.scale);
  const laneGapPx = getKeystrokeBarrierGapPx(
    keyboardItems[0]?.layout ?? null,
    mouseItems[0]?.layout ?? null
  );
  const keyboardSlotWidths = getKeystrokeSlotWidthHints(state, ctx, cache, 'keyboard', canvas.height, keyboardItems, overlayTransform.scale);
  const mouseSlotWidths = getKeystrokeSlotWidthHints(state, ctx, cache, 'mouse', canvas.height, mouseItems, overlayTransform.scale);
  const keyboardPlacements = layoutKeystrokeLane(
    keyboardItems, canvas.width, canvas.height, laneGapPx, 'right',
    keyboardSlotWidths, overlayTransform.anchorXPx, overlayTransform.baselineYPx
  );
  const mousePlacements = layoutKeystrokeLane(
    mouseItems, canvas.width, canvas.height, laneGapPx, 'left',
    mouseSlotWidths, overlayTransform.anchorXPx, overlayTransform.baselineYPx
  );
  const placementBounds = getKeystrokeOverlayBoundsFromPlacements(keyboardPlacements, mousePlacements);
  const fallbackWidth = Math.round(Math.max(150, canvas.width * 0.16) * overlayTransform.scale);
  const fallbackHeight = Math.round(Math.max(46, canvas.height * 0.06) * overlayTransform.scale);
  const fallbackX = overlayTransform.anchorXPx - (fallbackWidth * 0.5);
  const fallbackY = overlayTransform.baselineYPx - fallbackHeight - Math.round(26 * overlayTransform.scale);
  const bounds = placementBounds ?? { x: fallbackX, y: fallbackY, width: fallbackWidth, height: fallbackHeight };
  const pad = Math.round(Math.max(8, 14 * overlayTransform.scale));
  const x = Math.max(0, bounds.x - pad);
  const y = Math.max(0, bounds.y - pad);
  const width = Math.min(canvas.width - x, bounds.width + pad * 2);
  const height = Math.min(canvas.height - y, bounds.height + pad * 2);
  const handleSize = Math.round(Math.max(10, 14 * overlayTransform.scale));
  return { x, y, width, height, handleSize };
}
