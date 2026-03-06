import { BackgroundConfig } from '@/types/video';
import { shouldCursorRotate, getCursorRotationPivot, getCursorShadowStrength } from './cursorDynamics';
import {
  CursorRenderType,
  CursorImageSet,
  CursorRenderState,
  getCursorImage,
} from './cursorTypes';
import { getPreviewFrame } from './cursorAnimationCapture';

// Re-export shared cursor renderer types and helpers.
export {
  type CursorRenderType,
  type CursorPack,
  type CursorRenderKind,
  type CursorImageSet,
  type CursorRenderState,
  CURSOR_PACKS,
  CURSOR_RENDER_KINDS,
  getCursorPack,
  resolveCursorRenderType,
  getCursorImage,
} from './cursorTypes';

function isDrawableCursorImage(image: HTMLImageElement | null | undefined): image is HTMLImageElement {
  return Boolean(image?.complete && image.naturalWidth > 0 && image.naturalHeight > 0);
}

function resolveDrawableCursorType(images: CursorImageSet, requestedType: string): CursorRenderType {
  if (isDrawableCursorImage(getCursorImage(images, requestedType))) {
    return requestedType as CursorRenderType;
  }

  const fallbackChain: CursorRenderType[] = requestedType === 'openhand-screenstudio'
    ? ['pointer-screenstudio', 'default-screenstudio']
    : ['default-screenstudio'];

  for (const fallbackType of fallbackChain) {
    if (isDrawableCursorImage(getCursorImage(images, fallbackType))) {
      return fallbackType;
    }
  }

  return 'default-screenstudio';
}

// ---------------------------------------------------------------------------
// drawCenteredCursorImage – render a cursor image centered on the current
// canvas transform origin with large-source normalization
// ---------------------------------------------------------------------------

export function drawCenteredCursorImage(ctx: CanvasRenderingContext2D, img: HTMLImageElement): void {
  // Check if this cursor has pre-rendered animation frames.
  // Returns a blob URL <img> pointing to a frozen SVG — Chrome renders it as
  // vector graphics at the final display resolution, perfectly crisp at any scale.
  const animImg = getPreviewFrame(img);
  if (animImg && animImg.complete && animImg.naturalWidth > 0 && animImg.naturalHeight > 0) {
    const sourceMax = Math.max(animImg.naturalWidth, animImg.naturalHeight);
    const normalizeScale = sourceMax > 96 ? (48 / sourceMax) : 1;
    const drawW = animImg.naturalWidth * normalizeScale;
    const drawH = animImg.naturalHeight * normalizeScale;
    ctx.translate(-drawW * 0.5, -drawH * 0.5);
    ctx.drawImage(animImg, 0, 0, drawW, drawH);
    return;
  }

  // Static cursor — fallback when no animation or frame not ready.
  if (!img.complete || img.naturalWidth === 0 || img.naturalHeight === 0) return;
  const sourceMax = Math.max(img.naturalWidth, img.naturalHeight);
  const normalizeScale = sourceMax > 96 ? (48 / sourceMax) : 1;
  const drawW = img.naturalWidth * normalizeScale;
  const drawH = img.naturalHeight * normalizeScale;
  ctx.translate(-drawW * 0.5, -drawH * 0.5);
  ctx.drawImage(img, 0, 0, drawW, drawH);
}

// ---------------------------------------------------------------------------
// drawCursorShape – composite a single cursor frame (shadow + image)
// ---------------------------------------------------------------------------

export function drawCursorShape(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  _isClicked: boolean,
  scale: number = 2,
  cursorType: string,
  rotation: number = 0,
  shadowScale: number = 1,
  images: CursorImageSet,
  state: CursorRenderState,
  backgroundConfig?: BackgroundConfig | null
): void {
  const lowerType = cursorType.toLowerCase();
  ctx.save();
  ctx.translate(x, y);
  if (shouldCursorRotate(lowerType) && Math.abs(rotation) > 0.0001) {
    const pivot = getCursorRotationPivot(lowerType);
    ctx.translate(pivot.x, pivot.y);
    ctx.rotate(rotation);
    ctx.translate(-pivot.x, -pivot.y);
  }
  ctx.scale(scale, scale);
  ctx.scale(state.currentSquishScale, state.currentSquishScale);

  const cursorShadowStrength = getCursorShadowStrength(backgroundConfig);
  if (cursorShadowStrength > 0.001) {
    const normalized = cursorShadowStrength / 100;
    const base = Math.pow(Math.min(normalized, 1), 0.8);
    const overdrive = Math.max(0, normalized - 1);
    const alpha = Math.min(1, (0.95 * base) + (0.85 * overdrive));
    ctx.shadowColor = `rgba(0, 0, 0, ${alpha.toFixed(3)})`;
    ctx.shadowBlur = (1.6 + (11.5 * base) + (14.0 * overdrive)) * shadowScale;
    ctx.shadowOffsetX = ((1.3 * base) + (1.7 * overdrive)) * shadowScale;
    ctx.shadowOffsetY = ((2.6 * base) + (3.2 * overdrive)) * shadowScale;
  } else {
    ctx.shadowColor = 'rgba(0,0,0,0)';
    ctx.shadowBlur = 0;
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
  }

  const effectiveType = resolveDrawableCursorType(images, lowerType);

  const mappingKey = `${cursorType}=>${effectiveType}`;
  if (!state.loggedCursorMappings.has(mappingKey)) {
    state.loggedCursorMappings.add(mappingKey);
    console.log('[CursorDebug] map', {
      rawType: cursorType,
      effectiveType,
    });
  }

  if (!state.loggedCursorTypes.has(effectiveType)) {
    state.loggedCursorTypes.add(effectiveType);
    const debugImg = getCursorImage(images, effectiveType);
    console.log('[CursorDebug] loaded', {
      effectiveType,
      src: debugImg?.src,
      naturalWidth: debugImg?.naturalWidth,
      naturalHeight: debugImg?.naturalHeight,
      complete: debugImg?.complete,
    });
  }

  const imageToDraw = getCursorImage(images, effectiveType) ?? images.defaultScreenStudioImage;
  drawCenteredCursorImage(ctx, imageToDraw);
  ctx.restore();
}

// ---------------------------------------------------------------------------
// drawMouseCursor – top-level entry: offscreen-composite then blit to main ctx
// ---------------------------------------------------------------------------

export function drawMouseCursor(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  isClicked: boolean,
  scale: number = 2,
  cursorType: string = 'default',
  rotation: number = 0,
  images: CursorImageSet,
  state: CursorRenderState,
  backgroundConfig?: BackgroundConfig | null
): void {
  // Always render through offscreen so visible->dismiss transition uses identical
  // rasterization and bounds (prevents viewbox "jump" / clipping on fade start).
  const shadowStrength = getCursorShadowStrength(backgroundConfig);
  const normalizedShadow = Math.max(0, shadowStrength) / 100;
  const shadowOverdrive = Math.max(0, normalizedShadow - 1);
  const shadowBlur = 1.6 + (11.5 * Math.min(normalizedShadow, 1)) + (14.0 * shadowOverdrive);
  const shadowOffset = (2.6 * Math.min(normalizedShadow, 1)) + (3.2 * shadowOverdrive);
  const shapeRadius = Math.max(28, scale * 32);
  const margin = Math.ceil(shapeRadius + shadowBlur + shadowOffset + 24);
  const idealSize = margin * 2;

  // Cap offscreen canvas to prevent quadratic cost at high zoom.
  // At 11x zoom the ideal size reaches ~1500px — shadow blur on a 1500x1500
  // canvas every frame is extremely expensive. Cap to 512 and scale up via
  // drawImage; the shadow is soft so bilinear upsampling is imperceptible.
  const maxPreviewSize = 512;
  const ratio = idealSize > maxPreviewSize ? maxPreviewSize / idealSize : 1;
  const size = Math.ceil(idealSize * ratio);

  if (state.cursorOffscreen.width !== size || state.cursorOffscreen.height !== size) {
    state.cursorOffscreen.width = size;
    state.cursorOffscreen.height = size;
    state.cursorOffscreenCtx = state.cursorOffscreen.getContext('2d')!;
  }

  const oCtx = state.cursorOffscreenCtx;
  oCtx.clearRect(0, 0, size, size);
  oCtx.globalAlpha = 1;

  if (ratio < 1) {
    oCtx.save();
    oCtx.scale(ratio, ratio);
    drawCursorShape(oCtx as unknown as CanvasRenderingContext2D, margin, margin, isClicked, scale, cursorType, rotation, ratio, images, state, backgroundConfig);
    oCtx.restore();
  } else {
    drawCursorShape(oCtx as unknown as CanvasRenderingContext2D, margin, margin, isClicked, scale, cursorType, rotation, 1, images, state, backgroundConfig);
  }

  ctx.save();
  if (ratio < 1) {
    // Scale capped canvas back to ideal size — cursor + shadow appear full-res
    ctx.drawImage(state.cursorOffscreen, x - margin, y - margin, idealSize, idealSize);
  } else {
    ctx.drawImage(state.cursorOffscreen, x - margin, y - margin);
  }
  ctx.restore();
}
