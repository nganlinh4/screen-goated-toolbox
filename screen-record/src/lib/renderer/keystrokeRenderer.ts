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
  const bodyW = 13 * unit;
  const bodyH = 21 * unit;
  const bodyX = -bodyW / 2;
  const bodyY = -bodyH / 2;
  const radius = 6.5 * unit;
  const midY = bodyY + bodyH * 0.44;
  const outline = Math.max(1.8, unit * 2.2);
  const holdMix = clamp01(visual.holdMix);

  ctx.save();
  ctx.translate(centerX, centerY);
  const slantRadians = (visual.slnt * Math.PI) / 180;
  ctx.transform(1, 0, Math.tan(slantRadians) * 0.7, 1, 0, 0);

  const baseFill = rgbaToCss(lerpRgba([255, 255, 255, 0.14],[255, 250, 222, 0.26], holdMix));
  const strokeColor = rgbaToCss(lerpRgba([255, 255, 255, 0.94], [255, 246, 189, 1], holdMix));
  const activeFill = rgbaToCss(lerpRgba([255, 255, 255, 0.9],[255, 246, 189, 1], holdMix));

  ctx.lineWidth = outline;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';

  // 1. Base Mouse Body
  ctx.fillStyle = baseFill;
  ctx.strokeStyle = strokeColor;
  ctx.beginPath();
  ctx.roundRect(bodyX, bodyY, bodyW, bodyH, radius);
  ctx.fill();
  ctx.stroke();

  // 2. Active Highlight Overlay
  ctx.fillStyle = activeFill;
  if (event.type === 'wheel' || event.btn === 'middle') {
    // Handled below in the wheel rendering block
  } else if (event.btn === 'right') {
    ctx.beginPath();
    ctx.moveTo(0, midY);
    ctx.lineTo(bodyX + bodyW, midY);
    ctx.lineTo(bodyX + bodyW, bodyY + radius);
    ctx.arcTo(bodyX + bodyW, bodyY, bodyX + bodyW - radius, bodyY, radius);
    ctx.lineTo(0, bodyY);
    ctx.closePath();
    ctx.fill();
  } else {
    // Default to left click
    ctx.beginPath();
    ctx.moveTo(0, midY);
    ctx.lineTo(bodyX, midY);
    ctx.lineTo(bodyX, bodyY + radius);
    ctx.arcTo(bodyX, bodyY, bodyX + radius, bodyY, radius);
    ctx.lineTo(0, bodyY);
    ctx.closePath();
    ctx.fill();
  }

  // 3. Inner Separator Lines
  const wheelW = 2.6 * unit;
  const wheelH = 5.5 * unit;
  const wheelY = bodyY + 2.5 * unit;

  ctx.beginPath();
  // Horizontal divider
  ctx.moveTo(bodyX, midY);
  ctx.lineTo(bodyX + bodyW, midY);
  // Vertical top divider
  ctx.moveTo(0, bodyY);
  ctx.lineTo(0, wheelY);
  // Vertical bottom divider
  ctx.moveTo(0, wheelY + wheelH);
  ctx.lineTo(0, midY);
  ctx.stroke();

  // 4. Scroll Wheel
  ctx.beginPath();
  ctx.roundRect(-wheelW / 2, wheelY, wheelW, wheelH, wheelW / 2);
  ctx.fillStyle = (event.type === 'wheel' || event.btn === 'middle') ? activeFill : baseFill;
  ctx.fill();
  ctx.stroke();

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
  const outline = Math.max(1.8, unit * 2.2);
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
    // Proper Spacebar bracket symbol[__]
    const barW = 16 * unit;
    const barH = 5 * unit;
    const yTop = -1.5 * unit;
    const yBot = yTop + barH;
    const r = 2 * unit;

    ctx.beginPath();
    ctx.moveTo(-barW / 2, yTop);
    ctx.lineTo(-barW / 2, yBot - r);
    ctx.arcTo(-barW / 2, yBot, -barW / 2 + r, yBot, r);
    ctx.lineTo(barW / 2 - r, yBot);
    ctx.arcTo(barW / 2, yBot, barW / 2, yBot - r, r);
    ctx.lineTo(barW / 2, yTop);
    ctx.stroke();
  } else if (iconType === 'enter') {
    // Classic Return symbol ⏎
    ctx.beginPath();
    ctx.moveTo(6.5 * unit, -4 * unit);
    ctx.lineTo(6.5 * unit, 0);
    ctx.arcTo(6.5 * unit, 2.5 * unit, 4 * unit, 2.5 * unit, 2.5 * unit);
    ctx.lineTo(-4.5 * unit, 2.5 * unit);
    ctx.stroke();

    // Arrowhead
    ctx.beginPath();
    ctx.moveTo(-1.5 * unit, -0.5 * unit);
    ctx.lineTo(-4.5 * unit, 2.5 * unit);
    ctx.lineTo(-1.5 * unit, 5.5 * unit);
    ctx.stroke();
  } else if (iconType === 'backspace') {
    // Standard Backspace symbol ⌫
    const leftX = -6.5 * unit;
    const midX = -2 * unit;
    const rightX = 6.5 * unit;
    const topY = -4 * unit;
    const botY = 4 * unit;
    const r = 1.5 * unit;

    ctx.beginPath();
    ctx.moveTo(midX, topY);
    ctx.lineTo(rightX - r, topY);
    ctx.arcTo(rightX, topY, rightX, topY + r, r);
    ctx.lineTo(rightX, botY - r);
    ctx.arcTo(rightX, botY, rightX - r, botY, r);
    ctx.lineTo(midX, botY);
    ctx.lineTo(leftX, 0);
    ctx.closePath();
    ctx.stroke();

    // Inner X
    const xSize = 1.6 * unit;
    const xCenter = 1.5 * unit;
    ctx.beginPath();
    ctx.moveTo(xCenter - xSize, -xSize);
    ctx.lineTo(xCenter + xSize, xSize);
    ctx.moveTo(xCenter + xSize, -xSize);
    ctx.lineTo(xCenter - xSize, xSize);
    ctx.stroke();
  } else if (iconType === 'tab') {
    // Standard Tab symbol ⇥
    ctx.beginPath();
    ctx.moveTo(-6 * unit, 0);
    ctx.lineTo(4.5 * unit, 0);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(1.5 * unit, -3 * unit);
    ctx.lineTo(4.5 * unit, 0);
    ctx.lineTo(1.5 * unit, 3 * unit);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(6.5 * unit, -4 * unit);
    ctx.lineTo(6.5 * unit, 4 * unit);
    ctx.stroke();
  } else if (iconType === 'shift') {
    // Standard Shift symbol ⇧
    ctx.beginPath();
    ctx.moveTo(0, -6.5 * unit);
    ctx.lineTo(-6 * unit, 0.5 * unit);
    ctx.lineTo(-2.5 * unit, 0.5 * unit);
    ctx.lineTo(-2.5 * unit, 6.5 * unit);
    ctx.lineTo(2.5 * unit, 6.5 * unit);
    ctx.lineTo(2.5 * unit, 0.5 * unit);
    ctx.lineTo(6 * unit, 0.5 * unit);
    ctx.closePath();
    ctx.stroke();
  } else if (iconType === 'capslock') {
    // Standard Capslock symbol ⇪
    ctx.beginPath();
    ctx.moveTo(0, -7.5 * unit);
    ctx.lineTo(-6 * unit, -0.5 * unit);
    ctx.lineTo(-2.5 * unit, -0.5 * unit);
    ctx.lineTo(-2.5 * unit, 4.5 * unit);
    ctx.lineTo(2.5 * unit, 4.5 * unit);
    ctx.lineTo(2.5 * unit, -0.5 * unit);
    ctx.lineTo(6 * unit, -0.5 * unit);
    ctx.closePath();
    ctx.stroke();

    ctx.beginPath();
    ctx.roundRect(-6 * unit, 6.5 * unit, 12 * unit, 2.5 * unit, 1 * unit);
    ctx.fill();
  } else if (iconType === 'delete') {
    // Sleek Modern Trash Can
    const topY = -3.5 * unit;
    const botY = 6 * unit;

    // Bin Lid
    ctx.beginPath();
    ctx.moveTo(-5.5 * unit, topY);
    ctx.lineTo(5.5 * unit, topY);
    ctx.stroke();

    // Bin Handle
    ctx.beginPath();
    ctx.moveTo(-2 * unit, topY);
    ctx.lineTo(-2 * unit, topY - 1.5 * unit);
    ctx.arcTo(-2 * unit, topY - 2.5 * unit, -1 * unit, topY - 2.5 * unit, 1 * unit);
    ctx.lineTo(1 * unit, topY - 2.5 * unit);
    ctx.arcTo(2 * unit, topY - 2.5 * unit, 2 * unit, topY - 1.5 * unit, 1 * unit);
    ctx.lineTo(2 * unit, topY);
    ctx.stroke();

    // Bin Body
    ctx.beginPath();
    ctx.moveTo(-4.5 * unit, topY);
    ctx.lineTo(-3.5 * unit, botY - 1.5 * unit);
    ctx.arcTo(-3.5 * unit, botY, -2 * unit, botY, 1.5 * unit);
    ctx.lineTo(2 * unit, botY);
    ctx.arcTo(3.5 * unit, botY, 3.5 * unit, botY - 1.5 * unit, 1.5 * unit);
    ctx.lineTo(4.5 * unit, topY);
    ctx.stroke();

    // Vertical Ribs
    ctx.beginPath();
    ctx.moveTo(-1.5 * unit, topY + 2 * unit);
    ctx.lineTo(-1 * unit, botY - 2 * unit);
    ctx.moveTo(1.5 * unit, topY + 2 * unit);
    ctx.lineTo(1 * unit, botY - 2 * unit);
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
