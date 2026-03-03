import type {
  TextSegment,
  VideoSegment,
  BakedOverlayPayload,
  OverlayFrame,
  OverlayQuad,
} from '@/types/video';
import { getTrimSegments } from '@/lib/trimSegments';
import { getSpeedAtTime } from '@/lib/exportEstimator';
import {
  type KeystrokeState,
  type KeystrokeBubbleLayout,
  clamp01,
  rebuildKeystrokeRenderCache,
  getKeystrokeOverlayTransform,
  getCachedKeystrokeBubbleLayout,
  drawKeystrokeBubble,
  buildActiveKeystrokeFrameLayout,
  getKeystrokeDelaySec,
} from './keystrokeRenderer';

// ---------------------------------------------------------------------------
// Text drag state
// ---------------------------------------------------------------------------

export interface TextDragState {
  isDraggingText: boolean;
  draggedTextId: string | null;
  dragOffset: { x: number; y: number };
}

// ---------------------------------------------------------------------------
// Font variation helper (text overlay version)
// ---------------------------------------------------------------------------

/**
 * Apply font-variation-settings as CSS on the canvas element.
 * Canvas 2D has no native API for font-variation-settings -- the only
 * working workaround is setting it on the element's CSS style so the
 * context inherits it during font resolution for fillText/measureText.
 */
export function applyFontVariations(
  ctx: CanvasRenderingContext2D,
  vars: TextSegment['style']['fontVariations']
): void {
  const parts: string[] = [];
  const wdth = vars?.wdth ?? 100;
  const slnt = vars?.slnt ?? 0;
  const rond = vars?.ROND ?? 0;
  if (wdth !== 100) parts.push(`'wdth' ${wdth}`);
  if (slnt !== 0) parts.push(`'slnt' ${slnt}`);
  if (rond !== 0) parts.push(`'ROND' ${rond}`);
  ctx.canvas.style.fontVariationSettings = parts.length > 0 ? parts.join(', ') : 'normal';
}

// ---------------------------------------------------------------------------
// Text overlay rendering
// ---------------------------------------------------------------------------

export function drawTextOverlay(
  ctx: CanvasRenderingContext2D,
  textSegment: TextSegment,
  width: number,
  height: number,
  fadeAlpha: number = 1.0
): { x: number; y: number; width: number; height: number } {
  const { style } = textSegment;
  const textAlign = style.textAlign ?? 'center';
  const opacity = style.opacity ?? 1;
  const letterSpacing = style.letterSpacing ?? 0;
  const background = style.background;
  const fontSize = style.fontSize;

  const vars = style.fontVariations;
  const wght = vars?.wght ?? (style.fontWeight === 'bold' ? 700 : 400);

  ctx.save();
  // Do NOT reset the transform here. During export atlas baking the caller
  // has applied a translation to place text inside its sprite slot.
  ctx.globalAlpha = opacity * fadeAlpha;

  // Set font-variation-settings on canvas element CSS -- the only way to control
  // variable font axes (wdth, slnt, ROND) in Canvas 2D (no native API exists).
  applyFontVariations(ctx, vars);
  ctx.font = `${wght} ${fontSize}px 'Google Sans Flex', sans-serif`;

  ctx.textBaseline = 'middle';

  // Split text by newlines for multi-line
  const lines = textSegment.text.split('\n');
  const lineHeight = fontSize * 1.25;

  // Measure each line width (account for letter spacing)
  const measureLine = (line: string): number => {
    const baseWidth = ctx.measureText(line).width;
    if (letterSpacing !== 0 && line.length > 1) {
      return baseWidth + letterSpacing * (line.length - 1);
    }
    return baseWidth;
  };

  const lineWidths = lines.map(measureLine);
  const maxLineWidth = Math.max(...lineWidths);
  const totalHeight = lines.length * lineHeight;

  // Anchor position (0-100% based)
  const anchorX = (style.x / 100) * width;
  const anchorY = (style.y / 100) * height;

  // Background pill padding
  const bgPadX = background?.enabled ? (background.paddingX ?? 16) : 0;
  const bgPadY = background?.enabled ? (background.paddingY ?? 8) : 0;

  // Hit area encompasses all lines + padding
  const hitPad = 10;
  let blockLeft: number;
  if (textAlign === 'left') {
    blockLeft = anchorX;
  } else if (textAlign === 'right') {
    blockLeft = anchorX - maxLineWidth;
  } else {
    blockLeft = anchorX - maxLineWidth / 2;
  }
  const blockTop = anchorY - totalHeight / 2;

  const hitArea = {
    x: blockLeft - bgPadX - hitPad,
    y: blockTop - bgPadY - hitPad,
    width: maxLineWidth + bgPadX * 2 + hitPad * 2,
    height: totalHeight + bgPadY * 2 + hitPad * 2
  };

  // Background pill
  if (background?.enabled) {
    const pillX = blockLeft - bgPadX;
    const pillY = blockTop - bgPadY;
    const pillW = maxLineWidth + bgPadX * 2;
    const pillH = totalHeight + bgPadY * 2;
    const r = Math.min(background.borderRadius ?? 8, pillW / 2, pillH / 2);

    const savedAlpha = ctx.globalAlpha;
    ctx.globalAlpha = savedAlpha * (background.opacity ?? 0.6);
    ctx.beginPath();
    ctx.roundRect(pillX, pillY, pillW, pillH, r);
    ctx.fillStyle = background.color ?? '#000000';
    ctx.fill();
    ctx.globalAlpha = savedAlpha;
  }

  // Draw each line
  ctx.shadowColor = 'rgba(0,0,0,0.7)';
  ctx.shadowBlur = 4;
  ctx.shadowOffsetX = 2;
  ctx.shadowOffsetY = 2;
  ctx.fillStyle = style.color;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const ly = blockTop + i * lineHeight + lineHeight / 2;
    let lx: number;
    if (textAlign === 'left') {
      lx = blockLeft;
    } else if (textAlign === 'right') {
      lx = blockLeft + maxLineWidth;
    } else {
      lx = blockLeft + maxLineWidth / 2;
    }

    if (letterSpacing !== 0 && line.length > 1) {
      // Char-by-char rendering for letter spacing
      drawTextWithSpacing(ctx, line, lx, ly, letterSpacing, textAlign, lineWidths[i]);
    } else {
      ctx.textAlign = textAlign;
      ctx.fillText(line, lx, ly);
    }
  }

  ctx.restore();
  return hitArea;
}

// ---------------------------------------------------------------------------
// Text with custom letter spacing
// ---------------------------------------------------------------------------

export function drawTextWithSpacing(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  spacing: number,
  align: CanvasTextAlign,
  totalWidth: number
): void {
  ctx.textAlign = 'left';
  let startX: number;
  if (align === 'center') {
    startX = x - totalWidth / 2;
  } else if (align === 'right') {
    startX = x - totalWidth;
  } else {
    startX = x;
  }

  let cx = startX;
  for (let i = 0; i < text.length; i++) {
    ctx.fillText(text[i], cx, y);
    cx += ctx.measureText(text[i]).width + spacing;
  }
}

// ---------------------------------------------------------------------------
// Text hit area calculation
// ---------------------------------------------------------------------------

export function getTextHitArea(
  ctx: CanvasRenderingContext2D,
  textSegment: TextSegment,
  width: number,
  height: number
): { x: number; y: number; width: number; height: number } {
  const { style } = textSegment;
  const textAlign = style.textAlign ?? 'center';
  const letterSpacing = style.letterSpacing ?? 0;
  const fontSize = style.fontSize;
  const background = style.background;

  const vars = style.fontVariations;
  const wght = vars?.wght ?? (style.fontWeight === 'bold' ? 700 : 400);

  ctx.save();
  applyFontVariations(ctx, vars);
  ctx.font = `${wght} ${fontSize}px 'Google Sans Flex', sans-serif`;

  const lines = textSegment.text.split('\n');
  const lineHeight = fontSize * 1.25;

  const measureLine = (line: string): number => {
    const baseWidth = ctx.measureText(line).width;
    if (letterSpacing !== 0 && line.length > 1) {
      return baseWidth + letterSpacing * (line.length - 1);
    }
    return baseWidth;
  };

  const maxLineWidth = Math.max(...lines.map(measureLine));
  const totalHeight = lines.length * lineHeight;

  const anchorX = (style.x / 100) * width;
  const anchorY = (style.y / 100) * height;

  const bgPadX = background?.enabled ? (background.paddingX ?? 16) : 0;
  const bgPadY = background?.enabled ? (background.paddingY ?? 8) : 0;
  const hitPad = 10;

  let blockLeft: number;
  if (textAlign === 'left') {
    blockLeft = anchorX;
  } else if (textAlign === 'right') {
    blockLeft = anchorX - maxLineWidth;
  } else {
    blockLeft = anchorX - maxLineWidth / 2;
  }
  const blockTop = anchorY - totalHeight / 2;

  ctx.restore();

  return {
    x: blockLeft - bgPadX - hitPad,
    y: blockTop - bgPadY - hitPad,
    width: maxLineWidth + bgPadX * 2 + hitPad * 2,
    height: totalHeight + bgPadY * 2 + hitPad * 2
  };
}

// ---------------------------------------------------------------------------
// Text drag handlers
// ---------------------------------------------------------------------------

export function handleMouseDown(
  e: MouseEvent,
  segment: VideoSegment,
  canvas: HTMLCanvasElement,
  dragState: TextDragState
): string | null {
  const rect = canvas.getBoundingClientRect();
  const x = (e.clientX - rect.left) * (canvas.width / rect.width);
  const y = (e.clientY - rect.top) * (canvas.height / rect.height);

  for (const text of segment.textSegments) {
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    const hitArea = getTextHitArea(ctx, text, canvas.width, canvas.height);
    if (x >= hitArea.x && x <= hitArea.x + hitArea.width &&
      y >= hitArea.y && y <= hitArea.y + hitArea.height) {
      dragState.isDraggingText = true;
      dragState.draggedTextId = text.id;
      dragState.dragOffset.x = x - (text.style.x / 100 * canvas.width);
      dragState.dragOffset.y = y - (text.style.y / 100 * canvas.height);
      return text.id;
    }
  }
  return null;
}

export function handleMouseMove(
  e: MouseEvent,
  _segment: VideoSegment,
  canvas: HTMLCanvasElement,
  onTextMove: (id: string, x: number, y: number) => void,
  dragState: TextDragState
): void {
  if (!dragState.isDraggingText || !dragState.draggedTextId) return;

  const rect = canvas.getBoundingClientRect();
  const x = (e.clientX - rect.left) * (canvas.width / rect.width);
  const y = (e.clientY - rect.top) * (canvas.height / rect.height);

  const newX = Math.max(0, Math.min(100, ((x - dragState.dragOffset.x) / canvas.width) * 100));
  const newY = Math.max(0, Math.min(100, ((y - dragState.dragOffset.y) / canvas.height) * 100));

  onTextMove(dragState.draggedTextId, newX, newY);
}

export function handleMouseUp(dragState: TextDragState): void {
  dragState.isDraggingText = false;
  dragState.draggedTextId = null;
}

// ---------------------------------------------------------------------------
// Keystroke bake padding helper
// ---------------------------------------------------------------------------

export function getKeystrokeBakePadding(layout: KeystrokeBubbleLayout): number {
  return Math.max(28, Math.round(layout.fontSize * 1.35));
}

// ---------------------------------------------------------------------------
// Overlay atlas baking (main export function)
// ---------------------------------------------------------------------------

/**
 * Bake all text and keystroke overlays into a single sprite atlas and compute
 * per-frame quad arrays for GPU compositing. Replaces the old per-bitmap bakers.
 */
export async function bakeOverlayAtlasAndPaths(
  segment: VideoSegment,
  outputWidth: number,
  outputHeight: number,
  fps: number = 60,
  keystrokeState: KeystrokeState
): Promise<BakedOverlayPayload> {
  keystrokeState.keystrokeLanguage = segment.keystrokeLanguage ?? 'en';
  const duration = Math.max(
    segment.trimEnd,
    ...(segment.trimSegments || []).map(s => s.endTime),
    0
  );

  const MAX_ATLAS_SIZE = 4096;
  const atlasCanvas = document.createElement('canvas');
  atlasCanvas.width = MAX_ATLAS_SIZE;
  atlasCanvas.height = MAX_ATLAS_SIZE;
  atlasCanvas.style.cssText = 'position:fixed;left:-9999px;top:-9999px;pointer-events:none;';
  document.body.appendChild(atlasCanvas);

  const atlasCtx = atlasCanvas.getContext('2d', { willReadFrequently: true });
  if (!atlasCtx) {
    atlasCanvas.remove();
    return { atlasBase64: '', atlasWidth: 1, atlasHeight: 1, frames: [] };
  }

  let packX = 0;
  let packY = 0;
  let rowH = 0;
  const pack = (w: number, h: number) => {
    if (packX + w > MAX_ATLAS_SIZE) { packX = 0; packY += rowH + 2; rowH = 0; }
    const rect = { x: packX, y: packY, w, h };
    packX += w + 2;
    rowH = Math.max(rowH, h);
    return rect;
  };

  type AtlasRect = { x: number; y: number; w: number; h: number };
  const textMap = new Map<string, { rect: AtlasRect; baseHitArea: { x: number; y: number; width: number; height: number }; pad: number }>();

  // Pack text overlays
  const textPad = 24;
  for (const text of segment.textSegments || []) {
    const hitArea = getTextHitArea(atlasCtx, text, outputWidth, outputHeight);
    const w = Math.ceil(hitArea.width + textPad * 2);
    const h = Math.ceil(hitArea.height + textPad * 2);
    const rect = pack(w, h);
    atlasCtx.save();
    atlasCtx.translate(rect.x + textPad - hitArea.x, rect.y + textPad - hitArea.y);
    drawTextOverlay(atlasCtx, text, outputWidth, outputHeight, 1.0);
    atlasCtx.restore();
    textMap.set(text.id, { rect, baseHitArea: hitArea, pad: textPad });
  }

  // Pack keystroke overlays -- dual-state baking (normal + held) per unique bubble.
  // keystrokeUniqueMap: uniqueKey -> {rectNormal, rectHeld, layout, pad}
  // keystrokeEventMap:  eventId  -> uniqueKey
  const keystrokeUniqueMap = new Map<string, { rectNormal: AtlasRect; rectHeld: AtlasRect; layout: KeystrokeBubbleLayout; pad: number }>();
  const keystrokeEventMap = new Map<string, string>(); // eventId -> uniqueKey
  const cache = rebuildKeystrokeRenderCache(keystrokeState, segment, duration);
  if (cache && cache.displayEvents.length > 0) {
    const overlayTransform = getKeystrokeOverlayTransform(segment, outputWidth, outputHeight);
    let uniqueCount = 0;
    for (const event of cache.displayEvents) {
      const layout = getCachedKeystrokeBubbleLayout(keystrokeState, atlasCtx, event, outputHeight, overlayTransform.scale);
      const uniqueKey = `${layout.label}|${layout.showMouseIcon}|${layout.keyIcon ?? ''}|${layout.fontSize}`;
      keystrokeEventMap.set(event.id, uniqueKey);
      if (!keystrokeUniqueMap.has(uniqueKey)) {
        const pad = getKeystrokeBakePadding(layout);
        const w = layout.width + pad * 2;
        const h = layout.height + pad * 2;

        const isMouse = event.type === 'mousedown' || event.type === 'wheel';
        const baseSlnt = isMouse ? -6 : 0;
        const baseRond = isMouse ? 96 : 88;

        // Bake Normal state (holdMix = 0)
        const rectNormal = pack(w, h);
        atlasCtx.clearRect(rectNormal.x, rectNormal.y, w, h);
        drawKeystrokeBubble(
          atlasCtx, event,
          rectNormal.x + pad, rectNormal.y + pad,
          layout.width, layout.height,
          layout.label, layout.fontSize, layout.radius, layout.paddingX,
          layout.showMouseIcon, layout.keyIcon, layout.iconBoxWidth, layout.iconGap,
          'center', 1.0,
          { alpha: 1, scale: 1, scaleX: 1, scaleY: 1, translateY: 0, wdth: 100, wght: 600, slnt: baseSlnt, rond: baseRond, holdMix: 0, laneWeight: 1 }
        );

        // Bake Held state (holdMix = 1) -- saturated color, slant, narrower width
        const rectHeld = pack(w, h);
        atlasCtx.clearRect(rectHeld.x, rectHeld.y, w, h);
        drawKeystrokeBubble(
          atlasCtx, event,
          rectHeld.x + pad, rectHeld.y + pad,
          layout.width, layout.height,
          layout.label, layout.fontSize, layout.radius, layout.paddingX,
          layout.showMouseIcon, layout.keyIcon, layout.iconBoxWidth, layout.iconGap,
          'center', 1.0,
          { alpha: 1, scale: 1, scaleX: 1, scaleY: 1, translateY: 0, wdth: isMouse ? 95 : 97, wght: isMouse ? 675 : 655, slnt: isMouse ? -12 : -2, rond: isMouse ? 82 : 78, holdMix: 1, laneWeight: 1 }
        );

        keystrokeUniqueMap.set(uniqueKey, { rectNormal, rectHeld, layout, pad });
        uniqueCount++;
        // Yield to UI every 10 unique renders so the browser stays responsive.
        if (uniqueCount % 10 === 0) await new Promise(r => setTimeout(r, 0));
      }
    }
  }

  const actualAtlasHeight = Math.max(1, packY + rowH + 2);
  const finalCanvas = document.createElement('canvas');
  finalCanvas.width = MAX_ATLAS_SIZE;
  finalCanvas.height = actualAtlasHeight;
  finalCanvas.getContext('2d')!.drawImage(atlasCanvas, 0, 0);
  const atlasBase64 = finalCanvas.toDataURL('image/png');
  atlasCanvas.remove();

  // Generate per-frame quad arrays.
  // IMPORTANT: This loop must mirror gpu_pipeline.rs `build_frame_times` exactly so that
  // frames[i] corresponds to output frame i. The Rust compositor indexes overlay_frames by
  // frame_idx directly — so any mismatch here causes keystrokes to be invisible at non-1x
  // speed segments (at 1x speed source_time == output_time, masking the bug).
  //
  // Algorithm (identical to Rust build_frame_times):
  //   current_source_time starts at trimSegments[0].startTime
  //   per output frame: advance by clamp(speed(t), 0.1, 16) * (1/fps)
  //   when current_source_time crosses a segment boundary: jump to next segment's startTime
  const frames: OverlayFrame[] = [];
  const outDt = 1 / fps;
  const speedPoints = segment.speedPoints || [];
  const trimSegments = getTrimSegments(segment, duration);
  const endTime = trimSegments[trimSegments.length - 1].endTime;
  const delaySec = getKeystrokeDelaySec(segment);
  const fadeDur = 0.3;

  let segIdx = 0;
  let t = trimSegments[0].startTime;
  let frameCount = 0;

  while (t < endTime - 1e-9) {
    // Advance to the next trim segment if source time has passed the current segment's end
    // (mirrors the inner while in Rust build_frame_times)
    while (segIdx < trimSegments.length && t >= trimSegments[segIdx].endTime) {
      segIdx++;
      if (segIdx < trimSegments.length) {
        t = trimSegments[segIdx].startTime;
      }
    }
    if (segIdx >= trimSegments.length) break;

    const quads: OverlayQuad[] = [];

    for (const text of segment.textSegments || []) {
      if (t >= text.startTime && t <= text.endTime) {
        const elapsed = t - text.startTime;
        const remaining = text.endTime - t;
        let alpha = 1.0;
        if (elapsed < fadeDur) alpha = elapsed / fadeDur;
        if (remaining < fadeDur) alpha = Math.min(alpha, remaining / fadeDur);
        const mapping = textMap.get(text.id);
        if (mapping && alpha > 0.001) {
          quads.push({
            x: mapping.baseHitArea.x - mapping.pad,
            y: mapping.baseHitArea.y - mapping.pad,
            w: mapping.rect.w,
            h: mapping.rect.h,
            u: mapping.rect.x / MAX_ATLAS_SIZE,
            v: mapping.rect.y / actualAtlasHeight,
            uw: mapping.rect.w / MAX_ATLAS_SIZE,
            vh: mapping.rect.h / actualAtlasHeight,
            alpha,
          });
        }
      }
    }

    if (cache) {
      const layout = buildActiveKeystrokeFrameLayout(keystrokeState, atlasCtx, segment, cache, t, delaySec, outputWidth, outputHeight);
      const drawPlacements = (placements: any[]) => {
        for (const p of placements) {
          const uniqueKey = keystrokeEventMap.get(p.item.active.event.id);
          const mapping = uniqueKey ? keystrokeUniqueMap.get(uniqueKey) : undefined;
          if (!mapping) continue;
          const visual = p.item.visual;
          if (visual.alpha <= 0.001) continue;
          const baseW = p.item.layout.width + mapping.pad * 2;
          const baseH = p.item.layout.height + mapping.pad * 2;
          const drawW = baseW * visual.scale * visual.scaleX;
          const drawH = baseH * visual.scale * visual.scaleY;
          const cx = p.x + p.item.bubbleWidth / 2;
          const cy = p.y + p.item.layout.height / 2 + visual.translateY;
          const quadX = cx - drawW / 2;
          const quadY = cy - drawH / 2;
          const mix = clamp01(visual.holdMix);

          // Crossfade two opaque states (Normal + Held) using Premultiplied SrcOver.
          // Math: A_total = A_held + A_normal - A_held * A_normal.
          // Solving for A_total = visual.alpha gives the alphaNormal coefficient below.
          const alphaHeld = visual.alpha * mix;
          const alphaNormal = alphaHeld >= 0.999 ? 0 : (visual.alpha * (1 - mix)) / (1 - alphaHeld);
          if (alphaNormal > 0.001) {
            quads.push({
              x: quadX,
              y: quadY,
              w: drawW,
              h: drawH,
              u: mapping.rectNormal.x / MAX_ATLAS_SIZE,
              v: mapping.rectNormal.y / actualAtlasHeight,
              uw: mapping.rectNormal.w / MAX_ATLAS_SIZE,
              vh: mapping.rectNormal.h / actualAtlasHeight,
              alpha: alphaNormal,
            });
          }
          if (alphaHeld > 0.001) {
            quads.push({
              x: quadX,
              y: quadY,
              w: drawW,
              h: drawH,
              u: mapping.rectHeld.x / MAX_ATLAS_SIZE,
              v: mapping.rectHeld.y / actualAtlasHeight,
              uw: mapping.rectHeld.w / MAX_ATLAS_SIZE,
              vh: mapping.rectHeld.h / actualAtlasHeight,
              alpha: alphaHeld,
            });
          }
        }
      };
      drawPlacements(layout.keyboard);
      drawPlacements(layout.mouse);
    }

    frames.push({ time: t, quads });
    frameCount++;
    // Yield every 500 frames to keep the browser responsive during long exports.
    if (frameCount % 500 === 0) await new Promise(r => setTimeout(r, 0));

    // Advance source time by speed-adjusted output step — identical to Rust build_frame_times:
    //   speed = get_speed(current_source_time, speed_points).clamp(0.1, 16.0)
    //   current_source_time += speed * out_dt
    const speed = Math.max(0.1, Math.min(16.0, getSpeedAtTime(t, speedPoints)));
    t += speed * outDt;
  }

  return { atlasBase64, atlasWidth: MAX_ATLAS_SIZE, atlasHeight: actualAtlasHeight, frames };
}
