import { BackgroundConfig } from '@/types/video';
import { shouldCursorRotate, getCursorRotationPivot, getCursorShadowStrength } from './cursorDynamics';
import {
  CursorRenderType,
  CursorImageSet,
  CursorRenderState,
  getScreenStudioCursorImage,
  getMacos26CursorImage,
  getSgtcuteCursorImage,
  getSgtcoolCursorImage,
  getSgtaiCursorImage,
  getSgtpixelCursorImage,
  getJepriwin11CursorImage,
} from './cursorTypes';
import { getPreviewFrame } from './cursorAnimationCapture';

// Re-export everything from cursorTypes for backwards compatibility
export {
  type CursorRenderType,
  type CursorImageSet,
  type CursorRenderState,
  getCursorPack,
  resolveCursorRenderType,
  getMacos26CursorImage,
  getSgtcuteCursorImage,
  getSgtcoolCursorImage,
  getSgtaiCursorImage,
  getSgtpixelCursorImage,
  getJepriwin11CursorImage,
  getScreenStudioCursorImage,
} from './cursorTypes';

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
    const alpha = Math.min(1, (0.9 * base) + (0.6 * overdrive));
    ctx.shadowColor = `rgba(0, 0, 0, ${alpha.toFixed(3)})`;
    ctx.shadowBlur = (1.2 + (9.0 * base) + (8.0 * overdrive)) * shadowScale;
    ctx.shadowOffsetX = ((1.1 * base) + (1.0 * overdrive)) * shadowScale;
    ctx.shadowOffsetY = ((2.2 * base) + (1.8 * overdrive)) * shadowScale;
  } else {
    ctx.shadowColor = 'rgba(0,0,0,0)';
    ctx.shadowBlur = 0;
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
  }

  let effectiveType = lowerType;
  if (effectiveType.endsWith('-screenstudio')) {
    const image = getScreenStudioCursorImage(images, effectiveType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }
  if (effectiveType === 'pointer-screenstudio' && (!images.pointerScreenStudioImage.complete || images.pointerScreenStudioImage.naturalWidth === 0)) {
    effectiveType = 'default-screenstudio';
  }
  if (effectiveType === 'openhand-screenstudio' && (!images.openHandScreenStudioImage.complete || images.openHandScreenStudioImage.naturalWidth === 0)) {
    effectiveType = 'pointer-screenstudio';
  }
  if (effectiveType.endsWith('-macos26')) {
    const image = getMacos26CursorImage(images, effectiveType as CursorRenderType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }
  if (effectiveType.endsWith('-sgtcute')) {
    const image = getSgtcuteCursorImage(images, effectiveType as CursorRenderType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }
  if (effectiveType.endsWith('-sgtcool')) {
    const image = getSgtcoolCursorImage(images, effectiveType as CursorRenderType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }
  if (effectiveType.endsWith('-sgtai')) {
    const image = getSgtaiCursorImage(images, effectiveType as CursorRenderType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }
  if (effectiveType.endsWith('-sgtpixel')) {
    const image = getSgtpixelCursorImage(images, effectiveType as CursorRenderType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }
  if (effectiveType.endsWith('-jepriwin11')) {
    const image = getJepriwin11CursorImage(images, effectiveType as CursorRenderType);
    if (!image || !image.complete || image.naturalWidth === 0) {
      effectiveType = 'default-screenstudio';
    }
  }

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
    const debugImg =
      getScreenStudioCursorImage(images, effectiveType) ??
      getMacos26CursorImage(images, effectiveType as CursorRenderType) ??
      getSgtcuteCursorImage(images, effectiveType as CursorRenderType) ??
      getSgtcoolCursorImage(images, effectiveType as CursorRenderType) ??
      getSgtaiCursorImage(images, effectiveType as CursorRenderType) ??
      getSgtpixelCursorImage(images, effectiveType as CursorRenderType) ??
      getJepriwin11CursorImage(images, effectiveType as CursorRenderType);
    console.log('[CursorDebug] loaded', {
      effectiveType,
      src: debugImg?.src,
      naturalWidth: debugImg?.naturalWidth,
      naturalHeight: debugImg?.naturalHeight,
      complete: debugImg?.complete,
    });
  }

  switch (effectiveType) {
    case 'text-screenstudio':
    case 'pointer-screenstudio':
    case 'openhand-screenstudio':
    case 'closehand-screenstudio':
    case 'wait-screenstudio':
    case 'appstarting-screenstudio':
    case 'crosshair-screenstudio':
    case 'resize-ns-screenstudio':
    case 'resize-we-screenstudio':
    case 'resize-nwse-screenstudio':
    case 'resize-nesw-screenstudio': {
      const img = getScreenStudioCursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-macos26':
    case 'text-macos26':
    case 'pointer-macos26':
    case 'openhand-macos26':
    case 'closehand-macos26':
    case 'wait-macos26':
    case 'appstarting-macos26':
    case 'crosshair-macos26':
    case 'resize-ns-macos26':
    case 'resize-we-macos26':
    case 'resize-nwse-macos26':
    case 'resize-nesw-macos26': {
      const img = getMacos26CursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-sgtcute':
    case 'text-sgtcute':
    case 'pointer-sgtcute':
    case 'openhand-sgtcute':
    case 'closehand-sgtcute':
    case 'wait-sgtcute':
    case 'appstarting-sgtcute':
    case 'crosshair-sgtcute':
    case 'resize-ns-sgtcute':
    case 'resize-we-sgtcute':
    case 'resize-nwse-sgtcute':
    case 'resize-nesw-sgtcute': {
      const img = getSgtcuteCursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-sgtcool':
    case 'text-sgtcool':
    case 'pointer-sgtcool':
    case 'openhand-sgtcool':
    case 'closehand-sgtcool':
    case 'wait-sgtcool':
    case 'appstarting-sgtcool':
    case 'crosshair-sgtcool':
    case 'resize-ns-sgtcool':
    case 'resize-we-sgtcool':
    case 'resize-nwse-sgtcool':
    case 'resize-nesw-sgtcool': {
      const img = getSgtcoolCursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-sgtai':
    case 'text-sgtai':
    case 'pointer-sgtai':
    case 'openhand-sgtai':
    case 'closehand-sgtai':
    case 'wait-sgtai':
    case 'appstarting-sgtai':
    case 'crosshair-sgtai':
    case 'resize-ns-sgtai':
    case 'resize-we-sgtai':
    case 'resize-nwse-sgtai':
    case 'resize-nesw-sgtai': {
      const img = getSgtaiCursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-sgtpixel':
    case 'text-sgtpixel':
    case 'pointer-sgtpixel':
    case 'openhand-sgtpixel':
    case 'closehand-sgtpixel':
    case 'wait-sgtpixel':
    case 'appstarting-sgtpixel':
    case 'crosshair-sgtpixel':
    case 'resize-ns-sgtpixel':
    case 'resize-we-sgtpixel':
    case 'resize-nwse-sgtpixel':
    case 'resize-nesw-sgtpixel': {
      const img = getSgtpixelCursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-jepriwin11':
    case 'text-jepriwin11':
    case 'pointer-jepriwin11':
    case 'openhand-jepriwin11':
    case 'closehand-jepriwin11':
    case 'wait-jepriwin11':
    case 'appstarting-jepriwin11':
    case 'crosshair-jepriwin11':
    case 'resize-ns-jepriwin11':
    case 'resize-we-jepriwin11':
    case 'resize-nwse-jepriwin11':
    case 'resize-nesw-jepriwin11': {
      const img = getJepriwin11CursorImage(images, effectiveType);
      if (img) drawCenteredCursorImage(ctx, img);
      break;
    }

    case 'default-screenstudio': {
      const img = images.defaultScreenStudioImage;
      drawCenteredCursorImage(ctx, img);
      break;
    }

    default: {
      const img = images.defaultScreenStudioImage;
      drawCenteredCursorImage(ctx, img);
      break;
    }
  }
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
  const shadowBlur = 1.2 + (9.0 * Math.min(normalizedShadow, 1)) + (8.0 * shadowOverdrive);
  const shadowOffset = (2.2 * Math.min(normalizedShadow, 1)) + (1.8 * shadowOverdrive);
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
