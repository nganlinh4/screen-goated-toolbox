import type { TextSegment } from '@/types/video';
import {
  DEFAULT_TEXT_LINE_HEIGHT,
  normalizeTextStyle,
} from '@/lib/textStyleDefaults';
import { clamp01 } from './keystrokeRenderer';

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

export interface TextLayout {
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

export interface TextAnimationState {
  alpha: number;
  scale: number;
  translateY: number;
}

export function applyAnimationToRect(
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

export function buildTextLayout(
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
