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
  getKeystrokeOverlayConfig,
  getCachedKeystrokeBubbleLayout,
  drawKeystrokeBubble,
  buildActiveKeystrokeFrameLayout,
  getKeystrokeDelaySec,
  DEFAULT_KEYSTROKE_OVERLAY_X,
  DEFAULT_KEYSTROKE_OVERLAY_Y,
  DEFAULT_KEYSTROKE_OVERLAY_SCALE,
} from './keystrokeRenderer';
import {
  DEFAULT_TEXT_LINE_HEIGHT,
  normalizeTextStyle,
} from '@/lib/textStyleDefaults';

// ---------------------------------------------------------------------------
// Text drag state
// ---------------------------------------------------------------------------

export interface TextDragState {
  isDraggingText: boolean;
  draggedTextId: string | null;
  dragOffset: { x: number; y: number };
}

function getOverlayTextSegments(segment: VideoSegment): TextSegment[] {
  return [...(segment.subtitleSegments ?? []), ...(segment.textSegments ?? [])];
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

interface TextLayout {
  style: ReturnType<typeof normalizeTextStyle>;
  lines: string[];
  lineWidths: number[];
  maxLineWidth: number;
  totalHeight: number;
  lineHeightPx: number;
  blockLeft: number;
  blockTop: number;
  bgPadX: number;
  bgPadY: number;
  hitArea: { x: number; y: number; width: number; height: number };
  pivotX: number;
  pivotY: number;
}

interface TextAnimationState {
  alpha: number;
  scale: number;
  translateY: number;
}

function applyAnimationToRect(
  rect: { x: number; y: number; width: number; height: number },
  pivotX: number,
  pivotY: number,
  animation: TextAnimationState,
) {
  return {
    x: pivotX + (rect.x - pivotX) * animation.scale,
    y: pivotY + (rect.y - pivotY) * animation.scale + animation.translateY,
    width: rect.width * animation.scale,
    height: rect.height * animation.scale,
  };
}

function applyColorOpacity(color: string, opacity: number): string {
  const safeOpacity = clamp01(opacity);
  if (safeOpacity >= 0.999) return color;
  const hex = color.trim();
  if (hex.startsWith('#')) {
    const raw = hex.slice(1);
    const expanded = raw.length === 3
      ? raw.split('').map((part) => part + part).join('')
      : raw.length === 4
        ? raw.split('').map((part) => part + part).join('')
        : raw;
    if (expanded.length === 6 || expanded.length === 8) {
      const r = parseInt(expanded.slice(0, 2), 16);
      const g = parseInt(expanded.slice(2, 4), 16);
      const b = parseInt(expanded.slice(4, 6), 16);
      const a = expanded.length === 8 ? parseInt(expanded.slice(6, 8), 16) / 255 : 1;
      return `rgba(${r}, ${g}, ${b}, ${Math.max(0, Math.min(1, a * safeOpacity))})`;
    }
  }
  const rgbaMatch = hex.match(/^rgba?\((.+)\)$/i);
  if (rgbaMatch) {
    const parts = rgbaMatch[1].split(',').map((part) => part.trim());
    if (parts.length >= 3) {
      const [r, g, b] = parts;
      const baseAlpha = parts[3] ? Number(parts[3]) : 1;
      return `rgba(${r}, ${g}, ${b}, ${Math.max(0, Math.min(1, baseAlpha * safeOpacity))})`;
    }
  }
  return color;
}

function measureTextLine(
  ctx: CanvasRenderingContext2D,
  line: string,
  letterSpacing: number,
): number {
  const baseWidth = ctx.measureText(line).width;
  if (letterSpacing !== 0 && line.length > 1) {
    return baseWidth + letterSpacing * (line.length - 1);
  }
  return baseWidth;
}

function breakWordByWidth(
  ctx: CanvasRenderingContext2D,
  word: string,
  maxWidth: number,
  letterSpacing: number,
): string[] {
  const parts: string[] = [];
  let current = '';
  for (const char of Array.from(word)) {
    const next = current + char;
    if (current && measureTextLine(ctx, next, letterSpacing) > maxWidth) {
      parts.push(current);
      current = char;
    } else {
      current = next;
    }
  }
  if (current) parts.push(current);
  return parts.length > 0 ? parts : [''];
}

function wrapParagraph(
  ctx: CanvasRenderingContext2D,
  paragraph: string,
  maxWidth: number,
  letterSpacing: number,
): string[] {
  if (!paragraph) return [''];
  const tokens = paragraph.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return [''];
  const lines: string[] = [];
  let currentLine = '';

  for (const token of tokens) {
    const candidate = currentLine ? `${currentLine} ${token}` : token;
    if (measureTextLine(ctx, candidate, letterSpacing) <= maxWidth) {
      currentLine = candidate;
      continue;
    }
    if (currentLine) {
      lines.push(currentLine);
      currentLine = '';
    }
    if (measureTextLine(ctx, token, letterSpacing) <= maxWidth) {
      currentLine = token;
      continue;
    }
    const broken = breakWordByWidth(ctx, token, maxWidth, letterSpacing);
    lines.push(...broken.slice(0, -1));
    currentLine = broken[broken.length - 1] ?? '';
  }

  if (currentLine) lines.push(currentLine);
  return lines.length > 0 ? lines : [''];
}

function buildTextLines(
  ctx: CanvasRenderingContext2D,
  textSegment: TextSegment,
  width: number,
  letterSpacing: number,
): string[] {
  const style = normalizeTextStyle(textSegment.style);
  const paragraphs = textSegment.text.split('\n');
  if (!style.wrap?.enabled) return paragraphs.length > 0 ? paragraphs : [''];
  const maxWidth = Math.max(32, (style.wrap.maxWidthPercent / 100) * width);
  return paragraphs.flatMap((paragraph) =>
    wrapParagraph(ctx, paragraph, maxWidth, letterSpacing),
  );
}

function buildTextLayout(
  ctx: CanvasRenderingContext2D,
  textSegment: TextSegment,
  width: number,
  height: number,
): TextLayout {
  const style = normalizeTextStyle(textSegment.style);
  const textAlign = style.textAlign ?? 'center';
  const letterSpacing = style.letterSpacing ?? 0;
  const fontSize = style.fontSize;
  const background = style.background;
  const vars = style.fontVariations;
  const wght = vars?.wght ?? (style.fontWeight === 'bold' ? 700 : 400);

  applyFontVariations(ctx, vars);
  ctx.font = `${wght} ${fontSize}px 'Google Sans Flex', sans-serif`;

  const lines = buildTextLines(ctx, textSegment, width, letterSpacing);
  const lineHeightPx = fontSize * (style.lineHeight ?? DEFAULT_TEXT_LINE_HEIGHT);
  const lineWidths = lines.map((line) => measureTextLine(ctx, line, letterSpacing));
  const maxLineWidth = Math.max(...lineWidths, 0);
  const totalHeight = Math.max(lineHeightPx, lines.length * lineHeightPx);
  const anchorX = (style.x / 100) * width;
  const anchorY = (style.y / 100) * height;
  const bgPadX = background?.enabled ? (background.paddingX ?? 16) : 0;
  const bgPadY = background?.enabled ? (background.paddingY ?? 8) : 0;
  const strokeWidth = style.stroke?.enabled ? (style.stroke.width ?? 0) : 0;
  const shadow = style.shadow;
  const shadowExtentX = shadow?.enabled
    ? Math.abs(shadow.offsetX ?? 0) + (shadow.blur ?? 0)
    : 0;
  const shadowExtentY = shadow?.enabled
    ? Math.abs(shadow.offsetY ?? 0) + (shadow.blur ?? 0)
    : 0;
  const hitPad = 10 + strokeWidth + Math.max(shadowExtentX, shadowExtentY);

  let blockLeft: number;
  if (textAlign === 'left') {
    blockLeft = anchorX;
  } else if (textAlign === 'right') {
    blockLeft = anchorX - maxLineWidth;
  } else {
    blockLeft = anchorX - maxLineWidth / 2;
  }
  const blockTop = anchorY - totalHeight / 2;
  const pivotX = blockLeft + maxLineWidth / 2;
  const pivotY = blockTop + totalHeight / 2;

  return {
    style,
    lines,
    lineWidths,
    maxLineWidth,
    totalHeight,
    lineHeightPx,
    blockLeft,
    blockTop,
    bgPadX,
    bgPadY,
    pivotX,
    pivotY,
    hitArea: {
      x: blockLeft - bgPadX - hitPad,
      y: blockTop - bgPadY - hitPad,
      width: maxLineWidth + bgPadX * 2 + hitPad * 2,
      height: totalHeight + bgPadY * 2 + hitPad * 2,
    },
  };
}

export function getTextAnimationState(
  textSegment: TextSegment,
  currentTime: number,
): TextAnimationState {
  const style = normalizeTextStyle(textSegment.style);
  const animation = style.animation ?? {
    preset: 'fade',
    inDuration: 0.3,
    outDuration: 0.3,
  };
  const elapsed = currentTime - textSegment.startTime;
  const remaining = textSegment.endTime - currentTime;
  const inDuration = Math.max(0.001, animation.inDuration);
  const outDuration = Math.max(0.001, animation.outDuration);
  const enterT = clamp01(elapsed / inDuration);
  const exitT = clamp01(remaining / outDuration);
  const preset = animation.preset;

  if (preset === 'none') {
    return { alpha: 1, scale: 1, translateY: 0 };
  }

  let alpha = 1;
  if (elapsed < inDuration) alpha = enterT;
  if (remaining < outDuration) alpha = Math.min(alpha, exitT);

  if (preset === 'fade') {
    return { alpha, scale: 1, translateY: 0 };
  }

  if (preset === 'slide-up') {
    const enterOffset = Math.max(18, style.fontSize * 0.35);
    const exitOffset = Math.max(12, style.fontSize * 0.22);
    let translateY = 0;
    if (elapsed < inDuration) translateY = (1 - easeOutCubic(enterT)) * enterOffset;
    if (remaining < outDuration) {
      translateY = Math.max(translateY, (1 - easeOutCubic(exitT)) * exitOffset);
    }
    return { alpha, scale: 1, translateY };
  }

  const enterScale = 0.9 + easeOutBack(enterT) * 0.1;
  const exitScale = 1 + (1 - easeOutCubic(exitT)) * 0.05;
  let scale = 1;
  if (elapsed < inDuration) scale = enterScale;
  if (remaining < outDuration) scale = exitScale;
  return { alpha, scale, translateY: 0 };
}

function easeOutCubic(t: number): number {
  const p = 1 - clamp01(t);
  return 1 - p * p * p;
}

function easeOutBack(t: number): number {
  const p = clamp01(t) - 1;
  const s = 1.70158;
  return 1 + (s + 1) * p * p * p + s * p * p;
}

// ---------------------------------------------------------------------------
// Text overlay rendering
// ---------------------------------------------------------------------------

export function drawTextOverlay(
  ctx: CanvasRenderingContext2D,
  textSegment: TextSegment,
  width: number,
  height: number,
  fadeAlpha: number = 1.0,
  currentTime?: number,
): { x: number; y: number; width: number; height: number } {
  ctx.save();
  const layout = buildTextLayout(ctx, textSegment, width, height);
  const { style } = layout;
  const textAlign = style.textAlign ?? 'center';
  const opacity = style.opacity ?? 1;
  const letterSpacing = style.letterSpacing ?? 0;
  const background = style.background;
  const animation = currentTime !== undefined
    ? getTextAnimationState(textSegment, currentTime)
    : { alpha: 1, scale: 1, translateY: 0 };
  ctx.globalAlpha = opacity * fadeAlpha * animation.alpha;
  ctx.textBaseline = 'middle';
  ctx.translate(layout.pivotX, layout.pivotY + animation.translateY);
  ctx.scale(animation.scale, animation.scale);
  ctx.translate(-layout.pivotX, -layout.pivotY);

  // Background pill
  if (background?.enabled) {
    const pillX = layout.blockLeft - layout.bgPadX;
    const pillY = layout.blockTop - layout.bgPadY;
    const pillW = layout.maxLineWidth + layout.bgPadX * 2;
    const pillH = layout.totalHeight + layout.bgPadY * 2;
    const r = Math.min(background.borderRadius ?? 8, pillW / 2, pillH / 2);

    const savedAlpha = ctx.globalAlpha;
    ctx.globalAlpha = savedAlpha * (background.opacity ?? 0.6);
    ctx.beginPath();
    ctx.roundRect(pillX, pillY, pillW, pillH, r);
    ctx.fillStyle = background.color ?? '#000000';
    ctx.fill();
    ctx.globalAlpha = savedAlpha;
  }

  const shadow = style.shadow;
  if (shadow?.enabled) {
    ctx.shadowColor = applyColorOpacity(shadow.color, shadow.opacity);
    ctx.shadowBlur = shadow.blur;
    ctx.shadowOffsetX = shadow.offsetX;
    ctx.shadowOffsetY = shadow.offsetY;
  } else {
    ctx.shadowColor = 'transparent';
    ctx.shadowBlur = 0;
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
  }

  for (let i = 0; i < layout.lines.length; i++) {
    const line = layout.lines[i];
    const ly = layout.blockTop + i * layout.lineHeightPx + layout.lineHeightPx / 2;
    let lx: number;
    if (textAlign === 'left') {
      lx = layout.blockLeft;
    } else if (textAlign === 'right') {
      lx = layout.blockLeft + layout.maxLineWidth;
    } else {
      lx = layout.blockLeft + layout.maxLineWidth / 2;
    }

    if (style.stroke?.enabled && style.stroke.width > 0) {
      const savedAlpha = ctx.globalAlpha;
      ctx.globalAlpha = savedAlpha * (style.stroke.opacity ?? 1);
      ctx.lineJoin = 'round';
      ctx.miterLimit = 2;
      ctx.lineWidth = style.stroke.width;
      ctx.strokeStyle = applyColorOpacity(style.stroke.color, style.stroke.opacity);
      if (letterSpacing !== 0 && line.length > 1) {
        drawTextWithSpacing(ctx, line, lx, ly, letterSpacing, textAlign, layout.lineWidths[i], true);
      } else {
        ctx.textAlign = textAlign;
        ctx.strokeText(line, lx, ly);
      }
      ctx.globalAlpha = savedAlpha;
    }

    if (letterSpacing !== 0 && line.length > 1) {
      ctx.fillStyle = style.color;
      drawTextWithSpacing(ctx, line, lx, ly, letterSpacing, textAlign, layout.lineWidths[i]);
    } else {
      ctx.textAlign = textAlign;
      ctx.fillStyle = style.color;
      ctx.fillText(line, lx, ly);
    }
  }

  ctx.restore();
  return applyAnimationToRect(layout.hitArea, layout.pivotX, layout.pivotY, animation);
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
  totalWidth: number,
  strokeOnly: boolean = false,
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
    if (strokeOnly) {
      ctx.strokeText(text[i], cx, y);
    } else {
      ctx.fillText(text[i], cx, y);
    }
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
  ctx.save();
  const layout = buildTextLayout(ctx, textSegment, width, height);
  ctx.restore();
  return layout.hitArea;
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
    return { atlasBase64: '', atlasWidth: 1, atlasHeight: 1, frames: [], totalFrameCount: 0 };
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
  for (const text of getOverlayTextSegments(segment)) {
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
  // Extract raw RGBA for SharedBuffer zero-copy transfer.
  // Also produce PNG base64 as fallback for older WebView2 runtimes.
  const atlasRgba = atlasCtx.getImageData(0, 0, MAX_ATLAS_SIZE, actualAtlasHeight).data;
  const finalCanvas = document.createElement('canvas');
  finalCanvas.width = MAX_ATLAS_SIZE;
  finalCanvas.height = actualAtlasHeight;
  finalCanvas.getContext('2d')!.drawImage(atlasCanvas, 0, 0);
  const atlasBase64 = finalCanvas.toDataURL('image/png');
  atlasCanvas.remove();
  finalCanvas.remove();

  // Build compact atlas metadata for Rust-side frame quad generation.
  // This eliminates the need to send 40K+ frame objects over IPC.
  const overlayConfig = getKeystrokeOverlayConfig(segment);
  const textEntries = Array.from(textMap.entries()).map(([id, m]) => {
    const text = getOverlayTextSegments(segment).find(t => t.id === id);
    const style = text ? normalizeTextStyle(text.style) : null;
    const layout = text ? (() => {
      atlasCtx.save();
      const result = buildTextLayout(atlasCtx, text, outputWidth, outputHeight);
      atlasCtx.restore();
      return result;
    })() : null;
    return {
      id,
      startTime: text?.startTime ?? 0,
      endTime: text?.endTime ?? 0,
      rectX: m.rect.x,
      rectY: m.rect.y,
      rectW: m.rect.w,
      rectH: m.rect.h,
      hitX: m.baseHitArea.x,
      hitY: m.baseHitArea.y,
      hitW: m.baseHitArea.width,
      hitH: m.baseHitArea.height,
      pivotX: layout?.pivotX ?? (m.baseHitArea.x + m.baseHitArea.width / 2),
      pivotY: layout?.pivotY ?? (m.baseHitArea.y + m.baseHitArea.height / 2),
      pad: m.pad,
      animationPreset: style?.animation?.preset ?? 'fade',
      animationInDuration: style?.animation?.inDuration ?? 0.3,
      animationOutDuration: style?.animation?.outDuration ?? 0.3,
    };
  });
  const keystrokeEntries = Array.from(keystrokeUniqueMap.entries()).map(([uniqueKey, m]) => ({
    uniqueKey,
    normalRectX: m.rectNormal.x,
    normalRectY: m.rectNormal.y,
    normalRectW: m.rectNormal.w,
    normalRectH: m.rectNormal.h,
    heldRectX: m.rectHeld.x,
    heldRectY: m.rectHeld.y,
    heldRectW: m.rectHeld.w,
    heldRectH: m.rectHeld.h,
    layoutWidth: m.layout.width,
    layoutHeight: m.layout.height,
    layoutFontSize: m.layout.fontSize,
    layoutMarginBottom: m.layout.marginBottom,
    pad: m.pad,
    bubbleWidth: m.layout.width,
  }));

  const atlasMetadata = cache ? {
    atlasWidth: MAX_ATLAS_SIZE,
    atlasHeight: actualAtlasHeight,
    textEntries,
    keystrokeEntries,
    keystrokeMode: segment.keystrokeMode ?? 'off',
    keystrokeDelaySec: segment.keystrokeDelaySec ?? 0,
    overlayX: overlayConfig.x,
    overlayY: overlayConfig.y,
    overlayScale: overlayConfig.scale,
    visibilitySegments: cache.visibilityRef ?? [],
    displayEvents: cache.displayEvents.map(e => ({
      id: e.id,
      uniqueKey: keystrokeEventMap.get(e.id) ?? '',
      type: e.type,
      startTime: e.startTime,
      endTime: e.endTime,
      isHold: Boolean(e.isHold),
    })),
    keyboardStartTimes: cache.keyboardStartTimes,
    keyboardIndices: cache.keyboardIndices,
    mouseStartTimes: cache.mouseStartTimes,
    mouseIndices: cache.mouseIndices,
    keyboardMaxDuration: cache.keyboardMaxDuration,
    mouseMaxDuration: cache.mouseMaxDuration,
    eventSlots: cache.eventSlots,
    eventIdentities: cache.eventIdentities,
    keyboardSlotRepresentativeWidths: cache.keyboardSlotRepresentatives.map(idx => {
      if (typeof idx !== 'number') return 0;
      const ev = cache.displayEvents[idx];
      if (!ev) return 0;
      const layout = getCachedKeystrokeBubbleLayout(keystrokeState, atlasCtx, ev, outputHeight, overlayConfig.scale);
      return layout.width;
    }),
    mouseSlotRepresentativeWidths: cache.mouseSlotRepresentatives.map(idx => {
      if (typeof idx !== 'number') return 0;
      const ev = cache.displayEvents[idx];
      if (!ev) return 0;
      const layout = getCachedKeystrokeBubbleLayout(keystrokeState, atlasCtx, ev, outputHeight, overlayConfig.scale);
      return layout.width;
    }),
  } : (textEntries.length > 0 ? {
    atlasWidth: MAX_ATLAS_SIZE,
    atlasHeight: actualAtlasHeight,
    textEntries,
    keystrokeEntries: [],
    keystrokeMode: 'off',
    keystrokeDelaySec: 0,
    overlayX: DEFAULT_KEYSTROKE_OVERLAY_X,
    overlayY: DEFAULT_KEYSTROKE_OVERLAY_Y,
    overlayScale: DEFAULT_KEYSTROKE_OVERLAY_SCALE,
    visibilitySegments: [],
    displayEvents: [],
    keyboardStartTimes: [],
    keyboardIndices: [],
    mouseStartTimes: [],
    mouseIndices: [],
    keyboardMaxDuration: 0,
    mouseMaxDuration: 0,
    eventSlots: [],
    eventIdentities: [],
    keyboardSlotRepresentativeWidths: [],
    mouseSlotRepresentativeWidths: [],
  } : null);

  // When metadata is available, skip the expensive JS frame loop entirely.
  // Rust will generate overlay frames from the metadata in ~1ms.
  if (atlasMetadata) {
    return {
      atlasBase64,
      atlasRgba: new Uint8Array(atlasRgba.buffer),
      atlasWidth: MAX_ATLAS_SIZE,
      atlasHeight: actualAtlasHeight,
      frames: [],
      totalFrameCount: 0,
      atlasMetadata,
    };
  }

  // Fallback: generate per-frame quad arrays in JS (used by composition export
  // or when metadata is not available).
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

    for (const text of getOverlayTextSegments(segment)) {
      if (t >= text.startTime && t <= text.endTime) {
        const animation = getTextAnimationState(text, t);
        const mapping = textMap.get(text.id);
        if (mapping && animation.alpha > 0.001) {
          const animatedRect = applyAnimationToRect(
            {
              x: mapping.baseHitArea.x - mapping.pad,
              y: mapping.baseHitArea.y - mapping.pad,
              width: mapping.rect.w,
              height: mapping.rect.h,
            },
            mapping.baseHitArea.x + mapping.baseHitArea.width / 2,
            mapping.baseHitArea.y + mapping.baseHitArea.height / 2,
            animation,
          );
          quads.push({
            x: animatedRect.x,
            y: animatedRect.y,
            w: animatedRect.width,
            h: animatedRect.height,
            u: mapping.rect.x / MAX_ATLAS_SIZE,
            v: mapping.rect.y / actualAtlasHeight,
            uw: mapping.rect.w / MAX_ATLAS_SIZE,
            vh: mapping.rect.h / actualAtlasHeight,
            alpha: animation.alpha,
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

    // Only emit non-empty frames (sparse output) — Rust expands to dense array.
    if (quads.length > 0) {
      frames.push({ frameIndex: frameCount, quads });
    }
    frameCount++;
    // Yield every 500 frames to keep the browser responsive during long exports.
    if (frameCount % 500 === 0) await new Promise(r => setTimeout(r, 0));

    // Advance source time by speed-adjusted output step — identical to Rust build_frame_times:
    //   speed = get_speed(current_source_time, speed_points).clamp(0.1, 16.0)
    //   current_source_time += speed * out_dt
    const speed = Math.max(0.1, Math.min(16.0, getSpeedAtTime(t, speedPoints)));
    t += speed * outDt;
  }

  return { atlasBase64, atlasWidth: MAX_ATLAS_SIZE, atlasHeight: actualAtlasHeight, frames, totalFrameCount: frameCount, atlasMetadata: null };
}
