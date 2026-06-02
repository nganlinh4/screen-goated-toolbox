import type { VideoSegment } from '@/types/video';
import {
  type ActiveKeystrokeFrameLayout,
  type KeystrokeLanePlacement,
  type KeystrokeOverlayEditBounds,
  type KeystrokeRenderCache,
  type KeystrokeState,
  isTimeInsideSegments,
  rebuildKeystrokeRenderCache,
} from './keystrokeTypes';
import {
  buildKeystrokeLaneRenderItems,
  findActiveKeystrokeEventsForKind,
  getKeystrokeBarrierGapPx,
  getKeystrokeDelaySec,
  getKeystrokeOverlayTransform,
  getKeystrokeSlotWidthHints,
  layoutKeystrokeLane,
} from './keystrokeLayout';

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
