import type { KeystrokeEvent, VideoSegment } from '@/types/video';
import {
  type KeystrokeVisualState,
  type KeystrokeLanePlacement,
  clamp01,
  lerp,
  lerpRgba,
  rgbaToCss,
  applyKeystrokeFontVariations,
  getKeystrokeBorderColor,
  getKeystrokeFillColor,
  getKeystrokeTextColor,
  rebuildKeystrokeRenderCache,
} from './keystrokeTypes';
import {
  getKeystrokeOverlayTransform,
  findActiveKeystrokeEvents,
  buildKeystrokeLaneRenderItems,
  getKeystrokeBarrierGapPx,
  getKeystrokeSlotWidthHints,
  layoutKeystrokeLane,
} from './keystrokeLayout';

// Re-export everything from keystrokeTypes for backwards compatibility
export {
  DEFAULT_KEYSTROKE_DELAY_SEC,
  KEYSTROKE_ANIM_ENTER_SEC,
  KEYSTROKE_ANIM_EXIT_SEC,
  KEYSTROKE_SLOT_SPARSE_GAP_LIMIT,
  DEFAULT_KEYSTROKE_OVERLAY_X,
  DEFAULT_KEYSTROKE_OVERLAY_Y,
  DEFAULT_KEYSTROKE_OVERLAY_SCALE,
  KEYSTROKE_OVERLAY_MIN_SCALE,
  KEYSTROKE_OVERLAY_MAX_SCALE,
  clamp01,
  lerp,
  computePercentile,
  lerpRgba,
  rgbaToCss,
  easeOutCubic,
  easeInCubic,
  debugKeystrokeDurations,
  applyKeystrokeFontVariations,
  getKeystrokeVisualState,
  translateLabel,
  getKeystrokeLabel,
  getKeystrokeBorderColor,
  getKeystrokeFillColor,
  getKeystrokeTextColor,
  getKeystrokeIdentity,
  upperBound,
  isTimeInsideSegments,
  assignKeystrokeLaneSlots,
  rebuildKeystrokeRenderCache,
} from './keystrokeTypes';

export type {
  KeystrokeVisualState,
  KeystrokeBubbleLayout,
  KeystrokeRenderCache,
  ActiveKeystrokeEvent,
  ActiveKeystrokeLanes,
  KeystrokeLaneRenderItem,
  KeystrokeLanePlacement,
  ActiveKeystrokeFrameLayout,
  KeystrokeOverlayTransform,
  KeystrokeOverlayEditBounds,
  KeystrokeState,
} from './keystrokeTypes';

// Re-export everything from keystrokeLayout for backwards compatibility
export {
  getKeystrokeDelaySec,
  getKeystrokeOverlayConfig,
  getKeystrokeOverlayTransform,
  getDelayedKeystrokeRange,
  findActiveKeystrokeEventsForKind,
  findActiveKeystrokeEvents,
  measureKeystrokeBubble,
  getCachedKeystrokeBubbleLayout,
  getKeystrokeBubbleWidthForVisual,
  buildKeystrokeLaneRenderItems,
  getKeystrokePairGapPx,
  getKeystrokeBarrierGapPx,
  getKeystrokeLaneBubbleGapPx,
  getKeystrokeSlotWidthHints,
  getKeystrokeSlotAdvancePx,
  layoutKeystrokeLane,
  getKeystrokeOverlayBoundsFromPlacements,
  getKeystrokeOverlayEditBounds,
  buildActiveKeystrokeFrameLayout,
} from './keystrokeLayout';

// --- DRAWING ---

export function drawMouseIndicatorIcon(
  ctx: CanvasRenderingContext2D,
  event: KeystrokeEvent,
  centerX: number,
  centerY: number,
  iconSize: number,
  visual: KeystrokeVisualState
) {
  const unit = iconSize / 24;
  const bodyX = -7 * unit;
  const bodyY = -10 * unit;
  const bodyW = 14 * unit;
  const bodyH = 20 * unit;
  const corner = 7 * unit;
  const outline = Math.max(1, unit * 1.6);
  const holdMix = clamp01(visual.holdMix);

  ctx.save();
  ctx.translate(centerX, centerY);
  const slantRadians = (visual.slnt * Math.PI) / 180;
  ctx.transform(1, 0, Math.tan(slantRadians) * 0.7, 1, 0, 0);

  ctx.fillStyle = rgbaToCss(lerpRgba([255, 255, 255, 0.14], [255, 250, 222, 0.26], holdMix));
  ctx.strokeStyle = rgbaToCss(lerpRgba([255, 255, 255, 0.94], [255, 246, 189, 1], holdMix));
  ctx.lineWidth = outline;
  ctx.beginPath();
  ctx.roundRect(bodyX, bodyY, bodyW, bodyH, corner);
  ctx.fill();
  ctx.stroke();

  ctx.beginPath();
  ctx.moveTo(0, -6 * unit);
  ctx.lineTo(0, -2 * unit);
  ctx.stroke();

  ctx.fillStyle = rgbaToCss(lerpRgba([255, 255, 255, 0.9], [255, 246, 189, 1], holdMix));
  if (event.type === 'wheel') {
    ctx.beginPath();
    ctx.roundRect(-1.5 * unit, -5.2 * unit, 3 * unit, 4.4 * unit, 1.4 * unit);
    ctx.fill();
  } else if (event.btn === 'right') {
    ctx.beginPath();
    ctx.roundRect(0.35 * unit, -9 * unit, 5.8 * unit, 5.8 * unit, 2.8 * unit);
    ctx.fill();
  } else if (event.btn === 'middle') {
    ctx.beginPath();
    ctx.roundRect(-1.3 * unit, -8.9 * unit, 2.6 * unit, 5.3 * unit, 1.4 * unit);
    ctx.fill();
  } else {
    ctx.beginPath();
    ctx.roundRect(-6.15 * unit, -9 * unit, 5.8 * unit, 5.8 * unit, 2.8 * unit);
    ctx.fill();
  }
  ctx.restore();
}

export function drawKeyIconInBubble(
  ctx: CanvasRenderingContext2D,
  iconType: string,
  centerX: number,
  centerY: number,
  iconSize: number,
  visual: KeystrokeVisualState
) {
  const unit = iconSize / 24;
  const outline = Math.max(2, unit * 2.8);
  const holdMix = clamp01(visual.holdMix);
  const color = rgbaToCss(lerpRgba([255, 255, 255, 0.92], [244, 255, 249, 1.0], holdMix));

  ctx.save();
  ctx.translate(centerX, centerY);
  const slantRadians = (visual.slnt * Math.PI) / 180;
  ctx.transform(1, 0, Math.tan(slantRadians) * 0.7, 1, 0, 0);

  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = outline;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';

  if (iconType === 'space') {
    // Horizontal spacebar bar in lower-center of icon area
    const barW = 17 * unit;
    const barH = 3.5 * unit;
    ctx.beginPath();
    ctx.roundRect(-barW / 2, 3 * unit, barW, barH, barH / 2);
    ctx.fill();
  } else if (iconType === 'enter') {
    // Return shape: horizontal arm + vertical arm down + arrowhead pointing left
    ctx.beginPath();
    ctx.moveTo(-6 * unit, 3 * unit);
    ctx.lineTo(7 * unit, 3 * unit);
    ctx.lineTo(7 * unit, -6 * unit);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-6 * unit, 3 * unit);
    ctx.lineTo(-1 * unit, -1.5 * unit);
    ctx.moveTo(-6 * unit, 3 * unit);
    ctx.lineTo(-1 * unit, 7.5 * unit);
    ctx.stroke();
  } else if (iconType === 'backspace') {
    // Left arrow shaft + arrowhead
    ctx.beginPath();
    ctx.moveTo(-7 * unit, 0);
    ctx.lineTo(8 * unit, 0);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-7 * unit, 0);
    ctx.lineTo(-2 * unit, -4.5 * unit);
    ctx.moveTo(-7 * unit, 0);
    ctx.lineTo(-2 * unit, 4.5 * unit);
    ctx.stroke();
  } else if (iconType === 'tab') {
    // |-> (left vertical bar + right-pointing arrow)
    ctx.beginPath();
    ctx.moveTo(-8 * unit, -5 * unit);
    ctx.lineTo(-8 * unit, 5 * unit);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-4 * unit, 0);
    ctx.lineTo(7 * unit, 0);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(7 * unit, 0);
    ctx.lineTo(2 * unit, -4.5 * unit);
    ctx.moveTo(7 * unit, 0);
    ctx.lineTo(2 * unit, 4.5 * unit);
    ctx.stroke();
  } else if (iconType === 'shift') {
    // Upward hollow chevron with stem
    const tipY = -9 * unit;
    const midY = -1 * unit;
    const baseY = 7 * unit;
    const stemW = 3.5 * unit;
    const outerW = 9 * unit;
    ctx.beginPath();
    ctx.moveTo(0, tipY);
    ctx.lineTo(-outerW, midY);
    ctx.lineTo(-stemW, midY);
    ctx.lineTo(-stemW, baseY);
    ctx.lineTo(stemW, baseY);
    ctx.lineTo(stemW, midY);
    ctx.lineTo(outerW, midY);
    ctx.closePath();
    ctx.stroke();
  } else if (iconType === 'capslock') {
    // Shift chevron + underline bar
    const tipY = -8 * unit;
    const midY = -1 * unit;
    const stemTop = 2 * unit;
    const stemW = 3.5 * unit;
    const outerW = 9 * unit;
    ctx.beginPath();
    ctx.moveTo(0, tipY);
    ctx.lineTo(-outerW, midY);
    ctx.lineTo(-stemW, midY);
    ctx.lineTo(-stemW, stemTop);
    ctx.lineTo(stemW, stemTop);
    ctx.lineTo(stemW, midY);
    ctx.lineTo(outerW, midY);
    ctx.closePath();
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-outerW, 6 * unit);
    ctx.lineTo(outerW, 6 * unit);
    ctx.stroke();
  } else if (iconType === 'delete') {
    // Trash can icon
    const bodyW = 13 * unit;
    const bodyH = 11 * unit;
    const bodyTop = -3 * unit;
    const lidH = 2.5 * unit;
    const lidW = 16 * unit;
    const handleW = 5 * unit;
    const handleH = 2.5 * unit;
    // Handle (small rounded rect centered above lid)
    ctx.beginPath();
    ctx.roundRect(-handleW / 2, bodyTop - lidH - handleH, handleW, handleH, handleH / 2);
    ctx.stroke();
    // Lid (filled bar)
    ctx.beginPath();
    ctx.roundRect(-lidW / 2, bodyTop - lidH, lidW, lidH, lidH / 2);
    ctx.fill();
    // Body (rounded bottom corners)
    ctx.beginPath();
    ctx.roundRect(-bodyW / 2, bodyTop, bodyW, bodyH, [0, 0, 3 * unit, 3 * unit]);
    ctx.stroke();
    // Two vertical lines inside body
    const lineTop = bodyTop + 2.5 * unit;
    const lineBot = bodyTop + bodyH - 2.5 * unit;
    ctx.beginPath();
    ctx.moveTo(-3 * unit, lineTop);
    ctx.lineTo(-3 * unit, lineBot);
    ctx.moveTo(3 * unit, lineTop);
    ctx.lineTo(3 * unit, lineBot);
    ctx.stroke();
  }

  ctx.restore();
}

export function drawKeystrokeBubble(
  ctx: CanvasRenderingContext2D,
  event: KeystrokeEvent,
  x: number,
  y: number,
  width: number,
  height: number,
  label: string,
  fontSize: number,
  radius: number,
  paddingX: number,
  showMouseIcon: boolean,
  keyIcon: string | null,
  iconBoxWidth: number,
  iconGap: number,
  contentAlign: 'left' | 'center' | 'right' = 'center',
  alpha: number = 1,
  visual: KeystrokeVisualState = {
    alpha: 1,
    scale: 1,
    scaleX: 1,
    scaleY: 1,
    translateY: 0,
    wdth: 100,
    wght: 600,
    slnt: 0,
    rond: 88,
    holdMix: 0,
    laneWeight: 1,
  }
) {
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const originalVariations = ctx.canvas.style.fontVariationSettings;
  const finalAlpha = alpha * visual.alpha;
  const holdMix = clamp01(visual.holdMix);
  ctx.globalAlpha = finalAlpha;
  ctx.translate(x + width / 2, y + height / 2 + visual.translateY);
  ctx.scale(visual.scale * visual.scaleX, visual.scale * visual.scaleY);
  ctx.translate(-width / 2, -height / 2);
  ctx.shadowColor = rgbaToCss(lerpRgba([0, 0, 0, 0.45], [0, 0, 0, 0.58], holdMix));
  ctx.shadowBlur = Math.max(8, fontSize * lerp(0.45, 0.62, holdMix));
  ctx.shadowOffsetY = Math.max(2, fontSize * lerp(0.16, 0.22, holdMix));
  ctx.fillStyle = getKeystrokeFillColor(event, holdMix);
  ctx.beginPath();
  ctx.roundRect(0, 0, width, height, radius);
  ctx.fill();

  ctx.shadowColor = 'transparent';
  ctx.shadowBlur = 0;
  ctx.shadowOffsetY = 0;
  ctx.strokeStyle = getKeystrokeBorderColor(event, holdMix);
  ctx.lineWidth = Math.max(1, Math.round(fontSize * 0.07));
  ctx.stroke();

  applyKeystrokeFontVariations(ctx, visual);
  ctx.font = `${Math.round(visual.wght)} ${fontSize}px 'Google Sans Flex', sans-serif`;
  ctx.fillStyle = getKeystrokeTextColor(holdMix);
  ctx.textBaseline = 'middle';
  if (showMouseIcon) {
    const iconSize = Math.round(fontSize * 0.86);
    const iconCenterX = paddingX + iconBoxWidth * 0.5;
    drawMouseIndicatorIcon(ctx, event, iconCenterX, height / 2, iconSize, visual);
    ctx.font = `${Math.round(visual.wght)} ${fontSize}px 'Google Sans Flex', sans-serif`;
    ctx.textAlign = 'left';
    const textX = paddingX + iconBoxWidth + iconGap;
    ctx.fillText(label, textX, height / 2);
  } else if (keyIcon) {
    const iconSize = Math.round(fontSize * 0.82);
    const iconCenterX = paddingX + iconBoxWidth * 0.5;
    drawKeyIconInBubble(ctx, keyIcon, iconCenterX, height / 2, iconSize, visual);
    ctx.font = `${Math.round(visual.wght)} ${fontSize}px 'Google Sans Flex', sans-serif`;
    ctx.fillStyle = getKeystrokeTextColor(holdMix);
    ctx.textAlign = 'left';
    const textX = paddingX + iconBoxWidth + iconGap;
    ctx.fillText(label, textX, height / 2);
  } else {
    if (contentAlign === 'right') {
      ctx.textAlign = 'right';
      ctx.fillText(label, width - paddingX, height / 2);
    } else if (contentAlign === 'left') {
      ctx.textAlign = 'left';
      ctx.fillText(label, paddingX, height / 2);
    } else {
      ctx.textAlign = 'center';
      ctx.fillText(label, width / 2, height / 2);
    }
  }
  ctx.canvas.style.fontVariationSettings = originalVariations || 'normal';
  ctx.restore();
}

export function drawActiveKeystrokeOverlays(
  state: import('./keystrokeTypes').KeystrokeState,
  ctx: CanvasRenderingContext2D,
  segment: VideoSegment,
  currentTime: number,
  canvasWidth: number,
  canvasHeight: number,
  duration: number
) {
  state.keystrokeLanguage = segment.keystrokeLanguage ?? 'en';
  const cache = rebuildKeystrokeRenderCache(state, segment, duration);
  if (!cache) return;
  const overlayTransform = getKeystrokeOverlayTransform(segment, canvasWidth, canvasHeight);
  const activeLanes = findActiveKeystrokeEvents(state, segment, currentTime, duration);
  if (!activeLanes) return;

  const keyboardItems = buildKeystrokeLaneRenderItems(
    state,
    ctx,
    activeLanes.keyboard,
    currentTime,
    canvasHeight,
    overlayTransform.scale
  );
  const mouseItems = buildKeystrokeLaneRenderItems(
    state,
    ctx,
    activeLanes.mouse,
    currentTime,
    canvasHeight,
    overlayTransform.scale
  );
  if (!keyboardItems.length && !mouseItems.length) return;

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
  const keyboardPlacements = layoutKeystrokeLane(
    keyboardItems,
    canvasWidth,
    canvasHeight,
    laneGapPx,
    'right',
    keyboardSlotWidths,
    overlayTransform.anchorXPx,
    overlayTransform.baselineYPx
  );
  const mousePlacements = layoutKeystrokeLane(
    mouseItems,
    canvasWidth,
    canvasHeight,
    laneGapPx,
    'left',
    mouseSlotWidths,
    overlayTransform.anchorXPx,
    overlayTransform.baselineYPx
  );
  const drawPlacements = (placements: KeystrokeLanePlacement[]) => {
    for (let index = placements.length - 1; index >= 0; index--) {
      const placement = placements[index];
      drawKeystrokeBubble(
        ctx,
        placement.item.active.event,
        placement.x,
        placement.y,
        placement.item.bubbleWidth,
        placement.item.layout.height,
        placement.item.layout.label,
        placement.item.layout.fontSize,
        placement.item.layout.radius,
        placement.item.layout.paddingX,
        placement.item.layout.showMouseIcon,
        placement.item.layout.keyIcon,
        placement.item.layout.iconBoxWidth,
        placement.item.layout.iconGap,
        'center', // Center alignment for perfectly matched consistency with export
        1,
        placement.item.visual
      );
    }
  };

  drawPlacements(keyboardPlacements);
  drawPlacements(mousePlacements);
}
