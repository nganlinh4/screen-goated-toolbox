import type { CSSProperties } from 'react';
import {
  BUILT_IN_BACKGROUND_PANEL_ORDER,
  getBuiltInBackgroundPreset,
  isBuiltInBackgroundId,
  type BuiltInBackgroundId,
  type DiagonalGlowBackgroundPreset,
  type EdgeRibbonBackgroundPreset,
  type LinearBackgroundPreset,
  type StackedRadialBackgroundPreset,
} from '@/lib/backgroundPresets';
import { clamp01, hexToLinear, linearToSrgb, mix, smoothstep } from './gradientMath';

const BUILT_IN_BACKGROUND_TOKEN_PREFIX = '__builtin_background__:';
const swatchStyleCache = new Map<BuiltInBackgroundId, CSSProperties>();

export interface BuiltInBackgroundCache {
  renderedCanvasByKey: Map<string, HTMLCanvasElement>;
}

export function getBuiltInBackgroundToken(id: BuiltInBackgroundId): string {
  return `${BUILT_IN_BACKGROUND_TOKEN_PREFIX}${id}`;
}

export function parseBuiltInBackgroundToken(token: string): BuiltInBackgroundId | null {
  if (!token.startsWith(BUILT_IN_BACKGROUND_TOKEN_PREFIX)) return null;
  const id = token.slice(BUILT_IN_BACKGROUND_TOKEN_PREFIX.length);
  return isBuiltInBackgroundId(id) ? id : null;
}

function fillLinearBackground(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  width: number,
  height: number,
  preset: LinearBackgroundPreset
): void {
  const gradient = preset.axis === 'vertical'
    ? ctx.createLinearGradient(0, 0, 0, height)
    : ctx.createLinearGradient(0, 0, width, 0);
  gradient.addColorStop(0, preset.colors.start);
  gradient.addColorStop(1, preset.colors.end);
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);
}

function hexToRgba(hex: string, alpha: number): string {
  const raw = hex.replace('#', '');
  const r = parseInt(raw.slice(0, 2), 16);
  const g = parseInt(raw.slice(2, 4), 16);
  const b = parseInt(raw.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function fillStackedRadialBackground(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  width: number,
  height: number,
  preset: StackedRadialBackgroundPreset
): void {
  const gradient = preset.gradientAxis === 'horizontal'
    ? ctx.createLinearGradient(0, 0, width, 0)
    : ctx.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(0, preset.colors.start);
  gradient.addColorStop(0.5, preset.colors.mid);
  gradient.addColorStop(1, preset.colors.end);

  const radial = ctx.createRadialGradient(
    width * preset.overlayCenter[0],
    height * preset.overlayCenter[1],
    0,
    width * preset.overlayCenter[0],
    height * preset.overlayCenter[1],
    width * preset.overlayRadius
  );
  radial.addColorStop(0, hexToRgba(preset.colors.overlay, preset.overlayOpacity));
  radial.addColorStop(1, hexToRgba(preset.colors.overlay, 0));

  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = radial;
  ctx.fillRect(0, 0, width, height);
}

function fillDiagonalGlowBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: DiagonalGlowBackgroundPreset
): void {
  const startColor = hexToLinear(preset.colors.start);
  const midColor = hexToLinear(preset.colors.mid);
  const endColor = hexToLinear(preset.colors.end);

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const diag = clamp01((ux * preset.diagWeights[0]) + ((1 - uy) * preset.diagWeights[1]));

      let litR: number;
      let litG: number;
      let litB: number;
      if (diag < preset.split) {
        const t = diag / preset.split;
        litR = mix(startColor[0], midColor[0], t);
        litG = mix(startColor[1], midColor[1], t);
        litB = mix(startColor[2], midColor[2], t);
      } else {
        const t = (diag - preset.split) / (1 - preset.split);
        litR = mix(midColor[0], endColor[0], t);
        litG = mix(midColor[1], endColor[1], t);
        litB = mix(midColor[2], endColor[2], t);
      }

      const glowADistance = Math.hypot(ux - preset.glowACenter[0], uy - preset.glowACenter[1]);
      const glowBDistance = Math.hypot(ux - preset.glowBCenter[0], uy - preset.glowBCenter[1]);
      const glowA =
        smoothstep(preset.glowAOuterRadius, preset.glowAInnerRadius, glowADistance) * preset.glowAStrength;
      const glowB =
        smoothstep(preset.glowBOuterRadius, preset.glowBInnerRadius, glowBDistance) * preset.glowBStrength;

      litR += (preset.colors.glowAColorLinear[0] * glowA) + (preset.colors.glowBColorLinear[0] * glowB);
      litG += (preset.colors.glowAColorLinear[1] * glowA) + (preset.colors.glowBColorLinear[1] * glowB);
      litB += (preset.colors.glowAColorLinear[2] * glowA) + (preset.colors.glowBColorLinear[2] * glowB);

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot(ux - 0.5, uy - 0.5)
      ) * preset.vignetteStrength;
      litR = mix(litR, litR * 0.82, vignette);
      litG = mix(litG, litG * 0.82, vignette);
      litB = mix(litB, litB * 0.82, vignette);

      if (preset.noiseIntensity > 0) {
        const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
        const noiseUnit = noiseSeed - Math.floor(noiseSeed);
        const noise = (noiseUnit - 0.5) * (preset.noiseIntensity / 255.0);
        litR += noise;
        litG += noise;
        litB += noise;
      }

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }
}

function sampleRibbon(
  point: [number, number],
  start: [number, number],
  end: [number, number],
  width: number,
  curveAmp: number,
  curveFreq: number,
  intensity: number
): { band: number; core: number } {
  const segX = end[0] - start[0];
  const segY = end[1] - start[1];
  const segLenSq = Math.max((segX * segX) + (segY * segY), 1e-6);
  const rawT = ((point[0] - start[0]) * segX + (point[1] - start[1]) * segY) / segLenSq;
  const t = clamp01(rawT);
  const segLen = Math.sqrt(segLenSq);
  const normalX = -segY / segLen;
  const normalY = segX / segLen;
  const curve = Math.sin(t * Math.PI * curveFreq) * curveAmp;
  const curveX = start[0] + (segX * t) + (normalX * curve);
  const curveY = start[1] + (segY * t) + (normalY * curve);
  const distance = Math.hypot(point[0] - curveX, point[1] - curveY);
  const edgeFade = smoothstep(0.01, 0.14, t) * (1 - smoothstep(0.84, 0.99, t));
  const band = (1 - smoothstep(width * 0.55, width * 2.25, distance)) * edgeFade * intensity;
  const core = (1 - smoothstep(width * 0.10, width * 0.72, distance)) * edgeFade * intensity;
  return { band, core };
}

function fillEdgeRibbonBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: EdgeRibbonBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const depthColor = hexToLinear(preset.colors.depth);
  const ribbonAColor = hexToLinear(preset.colors.ribbonA);
  const ribbonBColor = hexToLinear(preset.colors.ribbonB);
  const glowColor = hexToLinear(preset.colors.glow);
  const aspect = width / Math.max(1, height);
  const ribbonAStart: [number, number] = [preset.ribbonAStart[0] * aspect, preset.ribbonAStart[1]];
  const ribbonAEnd: [number, number] = [preset.ribbonAEnd[0] * aspect, preset.ribbonAEnd[1]];
  const ribbonBStart: [number, number] = [preset.ribbonBStart[0] * aspect, preset.ribbonBStart[1]];
  const ribbonBEnd: [number, number] = [preset.ribbonBEnd[0] * aspect, preset.ribbonBEnd[1]];
  const glowCenter: [number, number] = [preset.glowCenter[0] * aspect, preset.glowCenter[1]];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];

      const depthMix = clamp01((uy * 0.86) + ((1 - ux) * 0.14));
      let litR = mix(baseColor[0], depthColor[0], depthMix);
      let litG = mix(baseColor[1], depthColor[1], depthMix);
      let litB = mix(baseColor[2], depthColor[2], depthMix);

      const ribbonA = sampleRibbon(
        point,
        ribbonAStart,
        ribbonAEnd,
        preset.ribbonAWidth,
        preset.ribbonACurveAmp,
        preset.ribbonACurveFreq,
        preset.ribbonAIntensity
      );
      const ribbonB = sampleRibbon(
        point,
        ribbonBStart,
        ribbonBEnd,
        preset.ribbonBWidth,
        preset.ribbonBCurveAmp,
        preset.ribbonBCurveFreq,
        preset.ribbonBIntensity
      );

      litR += (ribbonAColor[0] * ribbonA.band) + (ribbonBColor[0] * ribbonB.band);
      litG += (ribbonAColor[1] * ribbonA.band) + (ribbonBColor[1] * ribbonB.band);
      litB += (ribbonAColor[2] * ribbonA.band) + (ribbonBColor[2] * ribbonB.band);

      const coreGlow = (ribbonA.core * 0.42) + (ribbonB.core * 0.28);
      litR += glowColor[0] * coreGlow;
      litG += glowColor[1] * coreGlow;
      litB += glowColor[2] * coreGlow;

      const glowDistance = Math.hypot(point[0] - glowCenter[0], point[1] - glowCenter[1]);
      const glowStrength = (1 - smoothstep(0, preset.glowRadius, glowDistance)) * preset.glowIntensity;
      litR += glowColor[0] * glowStrength;
      litG += glowColor[1] * glowStrength;
      litB += glowColor[2] * glowStrength;

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot((ux - 0.5) * aspect, uy - 0.5)
      ) * preset.vignetteStrength;
      litR = mix(litR, litR * 0.82, vignette);
      litG = mix(litG, litG * 0.82, vignette);
      litB = mix(litB, litB * 0.82, vignette);

      if (preset.noiseIntensity > 0) {
        const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
        const noiseUnit = noiseSeed - Math.floor(noiseSeed);
        const noise = (noiseUnit - 0.5) * (preset.noiseIntensity / 255.0);
        litR += noise;
        litG += noise;
        litB += noise;
      }

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }
}

export function paintBuiltInBackgroundCanvas(
  canvas: HTMLCanvasElement,
  id: BuiltInBackgroundId,
  width: number,
  height: number
): void {
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const preset = getBuiltInBackgroundPreset(id);
  if (preset.family === 'linear') {
    fillLinearBackground(ctx, width, height, preset);
    return;
  }

  if (preset.family === 'stacked-radial') {
    fillStackedRadialBackground(ctx, width, height, preset);
    return;
  }

  const img = ctx.createImageData(width, height);
  if (preset.family === 'diagonal-glow') {
    fillDiagonalGlowBackgroundPixels(img.data, width, height, preset);
  } else {
    fillEdgeRibbonBackgroundPixels(img.data, width, height, preset);
  }
  ctx.putImageData(img, 0, 0);
}

function getRenderedCanvas(
  cache: BuiltInBackgroundCache,
  id: BuiltInBackgroundId,
  width: number,
  height: number
): HTMLCanvasElement {
  const key = `${id}:${width}x${height}`;
  const cached = cache.renderedCanvasByKey.get(key);
  if (cached) return cached;
  const canvas = document.createElement('canvas');
  paintBuiltInBackgroundCanvas(canvas, id, width, height);
  cache.renderedCanvasByKey.set(key, canvas);
  return canvas;
}

export function fillBuiltInBackground(
  cache: BuiltInBackgroundCache,
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  id: BuiltInBackgroundId,
  width: number,
  height: number
): void {
  const canvas = getRenderedCanvas(cache, id, width, height);
  ctx.drawImage(canvas, 0, 0, width, height);
}

export function getBuiltInBackgroundSwatchStyle(id: BuiltInBackgroundId): CSSProperties {
  const cached = swatchStyleCache.get(id);
  if (cached) return cached;
  if (typeof document === 'undefined') {
    const fallback = { background: '#000000' };
    swatchStyleCache.set(id, fallback);
    return fallback;
  }

  const swatch = document.createElement('canvas');
  paintBuiltInBackgroundCanvas(swatch, id, 128, 128);
  if (!swatch.getContext('2d')) {
    const fallback = { background: '#000000' };
    swatchStyleCache.set(id, fallback);
    return fallback;
  }

  const style = {
    backgroundImage: `url("${swatch.toDataURL('image/png')}")`,
    backgroundSize: '100% 100%',
    backgroundPosition: 'center',
  } satisfies CSSProperties;
  swatchStyleCache.set(id, style);
  return style;
}

export const BUILT_IN_BACKGROUND_SWATCHES: Record<BuiltInBackgroundId, CSSProperties> =
  Object.fromEntries(
    BUILT_IN_BACKGROUND_PANEL_ORDER.map((id) => [id, getBuiltInBackgroundSwatchStyle(id)])
  ) as Record<BuiltInBackgroundId, CSSProperties>;
