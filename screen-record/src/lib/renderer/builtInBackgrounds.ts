import type { CSSProperties } from 'react';
import {
  BUILT_IN_BACKGROUND_PANEL_ORDER,
  getBuiltInBackgroundPreset,
  isBuiltInBackgroundId,
  type BuiltInBackgroundId,
  type DiagonalGlowBackgroundPreset,
  type EdgeRibbonBackgroundPreset,
  type LinearBackgroundPreset,
  type PrismFoldBackgroundPreset,
  type StackedRadialBackgroundPreset,
  type TopographicFlowBackgroundPreset,
} from '@/lib/backgroundPresets';
import { clamp01, hexToLinear, linearToSrgb, mix, smoothstep } from './gradientMath';
import { getBuiltInBackgroundRenderSize, setCachedBuiltInBackground } from './builtInBackgroundPreview';

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

const PRISM_FOLD_ROLE_POINTS: ReadonlyArray<[number, number]> = [
  [0.02, 0.02],
  [0.98, 0.02],
  [0.98, 0.48],
  [0.38, 0.98],
];

const PRISM_FOLD_ROLE_WEIGHTS = [1.0, 0.92, 0.84, 0.96] as const;

function scaleLineForAspect(
  line: [number, number, number, number],
  aspect: number
): [number, number, number, number] {
  return [line[0] * aspect, line[1], line[2] * aspect, line[3]];
}

function signedDistanceToLine(
  point: [number, number],
  line: [number, number, number, number]
): number {
  const dx = line[2] - line[0];
  const dy = line[3] - line[1];
  const invLen = 1 / Math.max(Math.hypot(dx, dy), 1e-6);
  return (((point[0] - line[0]) * -dy) + ((point[1] - line[1]) * dx)) * invLen;
}

function samplePrismPane(
  point: [number, number],
  line: [number, number, number, number],
  referencePoint: [number, number],
  softness: number
): { mask: number; glow: number } {
  const signedDistance = signedDistanceToLine(point, line);
  const referenceSide = signedDistanceToLine(referencePoint, line) >= 0 ? 1 : -1;
  const inside = signedDistance * referenceSide;
  const mask = smoothstep(-softness * 1.2, softness * 3.2, inside);
  const body = smoothstep(softness * 1.4, softness * 7.5, inside);
  const glow = body * (1 - smoothstep(softness * 7.5, softness * 15.0, inside));
  return { mask, glow };
}

function fillPrismFoldBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: PrismFoldBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const paneColors = [
    hexToLinear(preset.colors.paneA),
    hexToLinear(preset.colors.paneB),
    hexToLinear(preset.colors.paneC),
    hexToLinear(preset.colors.paneD),
  ] as const;
  const aspect = width / Math.max(1, height);
  const paneLines = [
    scaleLineForAspect(preset.paneALine, aspect),
    scaleLineForAspect(preset.paneBLine, aspect),
    scaleLineForAspect(preset.paneCLine, aspect),
    scaleLineForAspect(preset.paneDLine, aspect),
  ] as const;
  const referencePoints = PRISM_FOLD_ROLE_POINTS.map(([x, y]) => [x * aspect, y] as [number, number]);

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];
      const ambient = clamp01(((1 - ux) * 0.52) + ((1 - uy) * 0.48));

      let litR = baseColor[0] * mix(0.84, 1.12, ambient);
      let litG = baseColor[1] * mix(0.84, 1.12, ambient);
      let litB = baseColor[2] * mix(0.84, 1.12, ambient);
      let paneAccumR = 0;
      let paneAccumG = 0;
      let paneAccumB = 0;
      let paneMaskSum = 0;

      for (let paneIndex = 0; paneIndex < paneLines.length; paneIndex++) {
        const { mask, glow } = samplePrismPane(
          point,
          paneLines[paneIndex],
          referencePoints[paneIndex],
          preset.softness
        );
        const roleWeight = PRISM_FOLD_ROLE_WEIGHTS[paneIndex];
        const paneMask = mask * roleWeight;
        const paneColor = paneColors[paneIndex];
        const paneContribution = (paneMask * preset.paneStrength) + (glow * preset.foldStrength * roleWeight);

        litR += paneColor[0] * paneContribution;
        litG += paneColor[1] * paneContribution;
        litB += paneColor[2] * paneContribution;

        paneAccumR += paneColor[0] * paneMask;
        paneAccumG += paneColor[1] * paneMask;
        paneAccumB += paneColor[2] * paneMask;
        paneMaskSum += paneMask;
      }

      const overlap = Math.max(paneMaskSum - 1, 0) * preset.overlapGain;
      if (overlap > 0) {
        const denom = Math.max(paneMaskSum, 1e-4);
        const avgR = paneAccumR / denom;
        const avgG = paneAccumG / denom;
        const avgB = paneAccumB / denom;
        litR += mix(avgR, 1, 0.35) * overlap;
        litG += mix(avgG, 1, 0.35) * overlap;
        litB += mix(avgB, 1, 0.35) * overlap;
      }

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

function fillTopographicFlowBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: TopographicFlowBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const lineAColor = hexToLinear(preset.colors.lineA);
  const lineBColor = hexToLinear(preset.colors.lineB);
  const glowColor = hexToLinear(preset.colors.glow);
  const ink = hexToLinear(preset.colors.ink);
  const aspect = width / Math.max(1, height);
  const sourceA: [number, number] = [preset.sourceA[0] * aspect, preset.sourceA[1]];
  const sourceB: [number, number] = [preset.sourceB[0] * aspect, preset.sourceB[1]];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const centeredX = (ux - 0.5) * aspect;
      const centeredY = uy - 0.5;
      const point: [number, number] = [ux * aspect, uy];
      const distA = Math.hypot(point[0] - sourceA[0], point[1] - sourceA[1]);
      const distB = Math.hypot(point[0] - sourceB[0], point[1] - sourceB[1]);
      const warp =
        (Math.sin(((point[0] * 0.82) + (point[1] * 1.14)) * Math.PI * 2 * preset.warpFreq) * preset.warpAmp) +
        (Math.sin(((point[0] * -0.58) + (point[1] * 0.92)) * Math.PI * 2 * preset.warpFreq * 0.72) * preset.warpAmp * 0.6);
      const field = ((distA * 0.92) + (distB * 0.78) + warp) * preset.lineScale;
      const line = 1 - smoothstep(preset.lineWidth, preset.lineWidth + 0.22, Math.abs(Math.sin(field * Math.PI)));
      const glow = 1 - smoothstep(
        preset.lineWidth * 2.6,
        (preset.lineWidth * 2.6) + 0.24,
        Math.abs(Math.sin((field + 0.32) * Math.PI))
      );
      const edgeBias = mix(
        preset.centerCalm,
        1,
        smoothstep(0.18, 0.84, Math.hypot(centeredX, centeredY))
      );
      const phaseMix = clamp01((Math.sin((distA - distB) * 4.6) * 0.5) + 0.5);
      const lineR = mix(lineAColor[0], lineBColor[0], phaseMix);
      const lineG = mix(lineAColor[1], lineBColor[1], phaseMix);
      const lineB = mix(lineAColor[2], lineBColor[2], phaseMix);

      let litR = baseColor[0];
      let litG = baseColor[1];
      let litB = baseColor[2];
      litR += lineR * line * preset.lineStrength * edgeBias;
      litG += lineG * line * preset.lineStrength * edgeBias;
      litB += lineB * line * preset.lineStrength * edgeBias;
      litR += glowColor[0] * glow * preset.glowStrength * edgeBias;
      litG += glowColor[1] * glow * preset.glowStrength * edgeBias;
      litB += glowColor[2] * glow * preset.glowStrength * edgeBias;

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot(centeredX, centeredY)
      ) * preset.vignetteStrength;
      litR = mix(litR, ink[0], vignette);
      litG = mix(litG, ink[1], vignette);
      litB = mix(litB, ink[2], vignette);

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
  } else if (preset.family === 'edge-ribbons') {
    fillEdgeRibbonBackgroundPixels(img.data, width, height, preset);
  } else if (preset.family === 'prism-fold') {
    fillPrismFoldBackgroundPixels(img.data, width, height, preset);
  } else {
    fillTopographicFlowBackgroundPixels(img.data, width, height, preset);
  }
  ctx.putImageData(img, 0, 0);
}

function getRenderedCanvas(
  cache: BuiltInBackgroundCache,
  id: BuiltInBackgroundId,
  width: number,
  height: number,
  interactive = false
): HTMLCanvasElement {
  const renderSize = getBuiltInBackgroundRenderSize(width, height, interactive);
  const key = `${interactive ? 'interactive:' : ''}${id}:${renderSize.width}x${renderSize.height}`;
  const cached = cache.renderedCanvasByKey.get(key);
  if (cached) return cached;
  const canvas = document.createElement('canvas');
  paintBuiltInBackgroundCanvas(canvas, id, renderSize.width, renderSize.height);
  setCachedBuiltInBackground(cache.renderedCanvasByKey, key, canvas);
  return canvas;
}

export function fillBuiltInBackground(
  cache: BuiltInBackgroundCache,
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  id: BuiltInBackgroundId,
  width: number,
  height: number,
  interactive = false
): void {
  const canvas = getRenderedCanvas(cache, id, width, height, interactive);
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
