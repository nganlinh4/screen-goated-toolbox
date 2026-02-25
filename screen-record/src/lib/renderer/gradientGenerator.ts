// Gradient generation utilities extracted from VideoRenderer.
// Color math, per-pixel gradient canvas generators, fill helpers,
// and background-style resolver.

import type { BackgroundConfig } from '@/types/video';

// ---------------------------------------------------------------------------
// Gradient style sentinel tokens – used by the renderer to detect pixel-fill
// gradients that cannot be expressed as a simple CanvasGradient/pattern.
// ---------------------------------------------------------------------------

export const GRADIENT4_STYLE_TOKEN = '__gradient4__';
export const GRADIENT5_STYLE_TOKEN = '__gradient5__';
export const GRADIENT6_STYLE_TOKEN = '__gradient6__';
export const GRADIENT7_STYLE_TOKEN = '__gradient7__';

// ---------------------------------------------------------------------------
// Cache interfaces – the VideoRenderer owns one instance of each and passes
// them into the free functions below so they can read/write cached canvases.
// ---------------------------------------------------------------------------

export interface GradientCache {
  gradient4Canvas: HTMLCanvasElement | null;
  gradient4CacheKey: string | undefined;
  gradient5Canvas: HTMLCanvasElement | null;
  gradient5CacheKey: string | undefined;
  gradient6Canvas: HTMLCanvasElement | null;
  gradient6CacheKey: string | undefined;
  gradient7Canvas: HTMLCanvasElement | null;
  gradient7CacheKey: string | undefined;
}

export interface CustomBgCache {
  customBackgroundImage: HTMLImageElement | null;
  customBackgroundPattern: CanvasPattern | null;
  lastCustomBackground: string | undefined;
  customBackgroundCacheKey: string | undefined;
}

// ---------------------------------------------------------------------------
// Color math utilities
// ---------------------------------------------------------------------------

export function clamp01(v: number): number {
  return Math.max(0, Math.min(1, v));
}

export function mix(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

export function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = clamp01((x - edge0) / (edge1 - edge0));
  return t * t * (3 - 2 * t);
}

export function linearToSrgb(c: number): number {
  if (c <= 0.0031308) return c * 12.92;
  return 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
}

export function hexToLinear(hex: string): [number, number, number] {
  const raw = hex.replace('#', '');
  const r = parseInt(raw.slice(0, 2), 16) / 255;
  const g = parseInt(raw.slice(2, 4), 16) / 255;
  const b = parseInt(raw.slice(4, 6), 16) / 255;
  const toLinear = (c: number) => c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  return [toLinear(r), toLinear(g), toLinear(b)];
}

// ---------------------------------------------------------------------------
// Gradient canvas generators – produce a cached HTMLCanvasElement with the
// per-pixel gradient baked in.  Callers pass a GradientCache so the canvas
// can be reused across frames.
// ---------------------------------------------------------------------------

export function getGradient4Canvas(cache: GradientCache, width: number, height: number): HTMLCanvasElement {
  const key = `${width}x${height}`;
  if (cache.gradient4Canvas && cache.gradient4CacheKey === key) {
    return cache.gradient4Canvas;
  }

  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const gctx = canvas.getContext('2d');
  if (!gctx) {
    cache.gradient4Canvas = canvas;
    cache.gradient4CacheKey = key;
    return canvas;
  }

  const img = gctx.createImageData(width, height);
  const data = img.data;

  const c1 = hexToLinear('#061a40');
  const cMid = hexToLinear('#0353a4');
  const c2 = hexToLinear('#f97316');
  const cool: [number, number, number] = [0.03, 0.33, 0.67];
  const warm: [number, number, number] = [0.98, 0.47, 0.09];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;

      const diag = clamp01((ux * 0.68) + ((1 - uy) * 0.32));
      let baseR: number;
      let baseG: number;
      let baseB: number;
      if (diag < 0.55) {
        const t = diag / 0.55;
        baseR = mix(c1[0], cMid[0], t);
        baseG = mix(c1[1], cMid[1], t);
        baseB = mix(c1[2], cMid[2], t);
      } else {
        const t = (diag - 0.55) / 0.45;
        baseR = mix(cMid[0], c2[0], t);
        baseG = mix(cMid[1], c2[1], t);
        baseB = mix(cMid[2], c2[2], t);
      }

      const dCool = Math.hypot(ux - 0.18, uy - 0.78);
      const dWarm = Math.hypot(ux - 0.86, uy - 0.22);
      const coolGlow = smoothstep(0.78, 0.05, dCool);
      const warmGlow = smoothstep(0.80, 0.08, dWarm);

      let litR = baseR + (cool[0] * coolGlow * 0.18) + (warm[0] * warmGlow * 0.14);
      let litG = baseG + (cool[1] * coolGlow * 0.18) + (warm[1] * warmGlow * 0.14);
      let litB = baseB + (cool[2] * coolGlow * 0.18) + (warm[2] * warmGlow * 0.14);

      const dCenter = Math.hypot(ux - 0.5, uy - 0.5);
      const vignette = smoothstep(0.20, 1.05, dCenter);
      const shadeT = vignette * 0.12;
      litR = mix(litR, litR * 0.82, shadeT);
      litG = mix(litG, litG * 0.82, shadeT);
      litB = mix(litB, litB * 0.82, shadeT);

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }

  gctx.putImageData(img, 0, 0);
  cache.gradient4Canvas = canvas;
  cache.gradient4CacheKey = key;
  return canvas;
}

export function getGradient5Canvas(cache: GradientCache, width: number, height: number): HTMLCanvasElement {
  const key = `${width}x${height}`;
  if (cache.gradient5Canvas && cache.gradient5CacheKey === key) {
    return cache.gradient5Canvas;
  }

  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const gctx = canvas.getContext('2d');
  if (!gctx) {
    cache.gradient5Canvas = canvas;
    cache.gradient5CacheKey = key;
    return canvas;
  }

  const img = gctx.createImageData(width, height);
  const data = img.data;

  const c1 = hexToLinear('#0d1b4c');
  const cMid = hexToLinear('#4b4c99');
  const c2 = hexToLinear('#ef476f');
  const cool: [number, number, number] = [0.14, 0.48, 0.62];
  const warm: [number, number, number] = [0.93, 0.28, 0.44];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;

      const diag = clamp01((ux * 0.62) + ((1 - uy) * 0.38));
      let baseR: number;
      let baseG: number;
      let baseB: number;
      if (diag < 0.52) {
        const t = diag / 0.52;
        baseR = mix(c1[0], cMid[0], t);
        baseG = mix(c1[1], cMid[1], t);
        baseB = mix(c1[2], cMid[2], t);
      } else {
        const t = (diag - 0.52) / 0.48;
        baseR = mix(cMid[0], c2[0], t);
        baseG = mix(cMid[1], c2[1], t);
        baseB = mix(cMid[2], c2[2], t);
      }

      const dCool = Math.hypot(ux - 0.22, uy - 0.86);
      const dWarm = Math.hypot(ux - 0.82, uy - 0.26);
      const coolGlow = smoothstep(0.76, 0.10, dCool);
      const warmGlow = smoothstep(0.74, 0.10, dWarm);

      let litR = baseR + (cool[0] * coolGlow * 0.14) + (warm[0] * warmGlow * 0.16);
      let litG = baseG + (cool[1] * coolGlow * 0.14) + (warm[1] * warmGlow * 0.16);
      let litB = baseB + (cool[2] * coolGlow * 0.14) + (warm[2] * warmGlow * 0.16);

      const dCenter = Math.hypot(ux - 0.5, uy - 0.5);
      const vignette = smoothstep(0.24, 1.02, dCenter);
      const shadeT = vignette * 0.09;
      litR = mix(litR, litR * 0.84, shadeT);
      litG = mix(litG, litG * 0.84, shadeT);
      litB = mix(litB, litB * 0.84, shadeT);

      // Tiny deterministic dithering to reduce visible bands in low-frequency gradients.
      const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
      const noiseUnit = noiseSeed - Math.floor(noiseSeed);
      const noise = (noiseUnit - 0.5) * (1.6 / 255.0);
      litR += noise;
      litG += noise;
      litB += noise;

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }

  gctx.putImageData(img, 0, 0);
  cache.gradient5Canvas = canvas;
  cache.gradient5CacheKey = key;
  return canvas;
}

export function getGradient6Canvas(cache: GradientCache, width: number, height: number): HTMLCanvasElement {
  const key = `${width}x${height}`;
  if (cache.gradient6Canvas && cache.gradient6CacheKey === key) {
    return cache.gradient6Canvas;
  }

  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const gctx = canvas.getContext('2d');
  if (!gctx) {
    cache.gradient6Canvas = canvas;
    cache.gradient6CacheKey = key;
    return canvas;
  }

  const img = gctx.createImageData(width, height);
  const data = img.data;

  const c1 = hexToLinear('#00d4ff');
  const cMid = hexToLinear('#ffe45e');
  const c2 = hexToLinear('#ff3d81');
  const cool: [number, number, number] = [0.00, 0.78, 0.98];
  const warm: [number, number, number] = [1.00, 0.89, 0.37];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;

      const diag = clamp01((ux * 0.66) + ((1 - uy) * 0.34));
      let baseR: number;
      let baseG: number;
      let baseB: number;
      if (diag < 0.50) {
        const t = diag / 0.50;
        baseR = mix(c1[0], cMid[0], t);
        baseG = mix(c1[1], cMid[1], t);
        baseB = mix(c1[2], cMid[2], t);
      } else {
        const t = (diag - 0.50) / 0.50;
        baseR = mix(cMid[0], c2[0], t);
        baseG = mix(cMid[1], c2[1], t);
        baseB = mix(cMid[2], c2[2], t);
      }

      const dCool = Math.hypot(ux - 0.20, uy - 0.80);
      const dWarm = Math.hypot(ux - 0.78, uy - 0.22);
      const coolGlow = smoothstep(0.78, 0.10, dCool);
      const warmGlow = smoothstep(0.72, 0.08, dWarm);

      let litR = baseR + (cool[0] * coolGlow * 0.16) + (warm[0] * warmGlow * 0.18);
      let litG = baseG + (cool[1] * coolGlow * 0.16) + (warm[1] * warmGlow * 0.18);
      let litB = baseB + (cool[2] * coolGlow * 0.16) + (warm[2] * warmGlow * 0.18);

      const dCenter = Math.hypot(ux - 0.5, uy - 0.5);
      const vignette = smoothstep(0.26, 1.02, dCenter);
      const shadeT = vignette * 0.06;
      litR = mix(litR, litR * 0.88, shadeT);
      litG = mix(litG, litG * 0.88, shadeT);
      litB = mix(litB, litB * 0.88, shadeT);

      const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
      const noiseUnit = noiseSeed - Math.floor(noiseSeed);
      const noise = (noiseUnit - 0.5) * (1.6 / 255.0);
      litR += noise;
      litG += noise;
      litB += noise;

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }

  gctx.putImageData(img, 0, 0);
  cache.gradient6Canvas = canvas;
  cache.gradient6CacheKey = key;
  return canvas;
}

export function getGradient7Canvas(cache: GradientCache, width: number, height: number): HTMLCanvasElement {
  const key = `${width}x${height}`;
  if (cache.gradient7Canvas && cache.gradient7CacheKey === key) {
    return cache.gradient7Canvas;
  }

  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const gctx = canvas.getContext('2d');
  if (!gctx) {
    cache.gradient7Canvas = canvas;
    cache.gradient7CacheKey = key;
    return canvas;
  }

  const img = gctx.createImageData(width, height);
  const data = img.data;

  const c1 = hexToLinear('#3fa7d6');
  const cMid = hexToLinear('#8d7ae6');
  const c2 = hexToLinear('#f29e6d');
  const cool: [number, number, number] = [0.25, 0.60, 0.78];
  const warm: [number, number, number] = [0.90, 0.58, 0.36];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;

      const diag = clamp01((ux * 0.64) + ((1 - uy) * 0.36));
      let baseR: number;
      let baseG: number;
      let baseB: number;
      if (diag < 0.52) {
        const t = diag / 0.52;
        baseR = mix(c1[0], cMid[0], t);
        baseG = mix(c1[1], cMid[1], t);
        baseB = mix(c1[2], cMid[2], t);
      } else {
        const t = (diag - 0.52) / 0.48;
        baseR = mix(cMid[0], c2[0], t);
        baseG = mix(cMid[1], c2[1], t);
        baseB = mix(cMid[2], c2[2], t);
      }

      const dCool = Math.hypot(ux - 0.24, uy - 0.78);
      const dWarm = Math.hypot(ux - 0.78, uy - 0.26);
      const coolGlow = smoothstep(0.78, 0.12, dCool);
      const warmGlow = smoothstep(0.76, 0.12, dWarm);

      let litR = baseR + (cool[0] * coolGlow * 0.10) + (warm[0] * warmGlow * 0.10);
      let litG = baseG + (cool[1] * coolGlow * 0.10) + (warm[1] * warmGlow * 0.10);
      let litB = baseB + (cool[2] * coolGlow * 0.10) + (warm[2] * warmGlow * 0.10);

      const dCenter = Math.hypot(ux - 0.5, uy - 0.5);
      const vignette = smoothstep(0.26, 1.02, dCenter);
      const shadeT = vignette * 0.08;
      litR = mix(litR, litR * 0.90, shadeT);
      litG = mix(litG, litG * 0.90, shadeT);
      litB = mix(litB, litB * 0.90, shadeT);

      const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
      const noiseUnit = noiseSeed - Math.floor(noiseSeed);
      const noise = (noiseUnit - 0.5) * (1.2 / 255.0);
      litR += noise;
      litG += noise;
      litB += noise;

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }

  gctx.putImageData(img, 0, 0);
  cache.gradient7Canvas = canvas;
  cache.gradient7CacheKey = key;
  return canvas;
}

// ---------------------------------------------------------------------------
// Gradient fill helpers – draw a cached gradient canvas into a target context.
// ---------------------------------------------------------------------------

export function fillGradient4Background(
  cache: GradientCache,
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  width: number,
  height: number
): void {
  const gradientCanvas = getGradient4Canvas(cache, width, height);
  ctx.drawImage(gradientCanvas, 0, 0, width, height);
}

export function fillGradient5Background(
  cache: GradientCache,
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  width: number,
  height: number
): void {
  const gradientCanvas = getGradient5Canvas(cache, width, height);
  ctx.drawImage(gradientCanvas, 0, 0, width, height);
}

export function fillGradient6Background(
  cache: GradientCache,
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  width: number,
  height: number
): void {
  const gradientCanvas = getGradient6Canvas(cache, width, height);
  ctx.drawImage(gradientCanvas, 0, 0, width, height);
}

export function fillGradient7Background(
  cache: GradientCache,
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  width: number,
  height: number
): void {
  const gradientCanvas = getGradient7Canvas(cache, width, height);
  ctx.drawImage(gradientCanvas, 0, 0, width, height);
}

// ---------------------------------------------------------------------------
// Background style resolver
// ---------------------------------------------------------------------------

/**
 * Callback invoked when a custom-background image finishes loading so the
 * renderer can schedule a repaint.
 */
export type OnCustomBgLoaded = () => void;

export function getBackgroundStyle(
  ctx: CanvasRenderingContext2D,
  type: BackgroundConfig['backgroundType'],
  customBgCache: CustomBgCache,
  onCustomBgLoaded: OnCustomBgLoaded,
  customBackground?: string
): string | CanvasGradient | CanvasPattern {
  switch (type) {
    case 'gradient1': {
      const gradient = ctx.createLinearGradient(0, 0, ctx.canvas.width, 0);
      gradient.addColorStop(0, '#4f7fd9');
      gradient.addColorStop(1, '#8a72d8');
      return gradient;
    }
    case 'gradient2': {
      const gradient = ctx.createLinearGradient(0, 0, ctx.canvas.width, 0);
      gradient.addColorStop(0, '#fb7185');
      gradient.addColorStop(1, '#fdba74');
      return gradient;
    }
    case 'gradient3': {
      const gradient = ctx.createLinearGradient(0, 0, ctx.canvas.width, 0);
      gradient.addColorStop(0, '#10b981');
      gradient.addColorStop(1, '#2dd4bf');
      return gradient;
    }
    case 'gradient5':
      return GRADIENT5_STYLE_TOKEN;
    case 'gradient6':
      return GRADIENT6_STYLE_TOKEN;
    case 'gradient7':
      return GRADIENT7_STYLE_TOKEN;
    case 'gradient4':
      return GRADIENT4_STYLE_TOKEN;
    case 'custom': {
      if (customBackground) {
        if (customBgCache.lastCustomBackground !== customBackground || !customBgCache.customBackgroundImage) {
          const img = new Image();
          img.onload = () => {
            if (customBgCache.customBackgroundImage !== img) return;
            customBgCache.customBackgroundCacheKey = undefined;
            onCustomBgLoaded();
          };
          img.onerror = () => {
            if (customBgCache.customBackgroundImage !== img) return;
            customBgCache.customBackgroundCacheKey = undefined;
          };
          img.src = customBackground;
          customBgCache.customBackgroundImage = img;
          customBgCache.lastCustomBackground = customBackground;
          customBgCache.customBackgroundCacheKey = undefined;
        }

        const img = customBgCache.customBackgroundImage;
        if (img && img.complete && img.naturalWidth > 0 && img.naturalHeight > 0) {
          const cacheKey = `${customBackground}|${ctx.canvas.width}x${ctx.canvas.height}`;
          if (!customBgCache.customBackgroundPattern || customBgCache.customBackgroundCacheKey !== cacheKey) {
            const tempCanvas = document.createElement('canvas');
            const tempCtx = tempCanvas.getContext('2d');

            if (tempCtx) {
              const cw = ctx.canvas.width;
              const ch = ctx.canvas.height;
              const iw = img.naturalWidth;
              const ih = img.naturalHeight;
              const coverScale = Math.max(cw / iw, ch / ih);
              const dw = iw * coverScale;
              const dh = ih * coverScale;
              const dx = (cw - dw) / 2;
              const dy = (ch - dh) / 2;

              tempCanvas.width = cw;
              tempCanvas.height = ch;
              tempCtx.imageSmoothingEnabled = true;
              tempCtx.imageSmoothingQuality = 'high';
              tempCtx.clearRect(0, 0, cw, ch);
              tempCtx.drawImage(img, dx, dy, dw, dh);
              customBgCache.customBackgroundPattern = ctx.createPattern(tempCanvas, 'no-repeat');
              customBgCache.customBackgroundCacheKey = cacheKey;
              tempCanvas.remove();
            }
          }

        }

        if (customBgCache.customBackgroundPattern) {
          customBgCache.customBackgroundPattern.setTransform(new DOMMatrix());
          return customBgCache.customBackgroundPattern;
        }
      }
      return '#000000';
    }
    case 'white': {
      const wGrad = ctx.createLinearGradient(0, 0, 0, ctx.canvas.height);
      wGrad.addColorStop(0, '#f5f5f5');
      wGrad.addColorStop(0.5, '#ffffff');
      wGrad.addColorStop(1, '#f5f5f5');

      const wCx = ctx.canvas.width / 2;
      const wCy = ctx.canvas.height / 2;
      const wRadial = ctx.createRadialGradient(wCx, wCy, 0, wCx, wCy, ctx.canvas.width * 0.8);
      wRadial.addColorStop(0, 'rgba(225, 225, 225, 0.15)');
      wRadial.addColorStop(1, 'rgba(255, 255, 255, 0)');

      ctx.fillStyle = wGrad;
      ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
      ctx.fillStyle = wRadial;
      ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);

      return 'rgba(0,0,0,0)';
    }
    case 'solid': {
      const gradient = ctx.createLinearGradient(0, 0, 0, ctx.canvas.height);
      gradient.addColorStop(0, '#0a0a0a');
      gradient.addColorStop(0.5, '#000000');
      gradient.addColorStop(1, '#0a0a0a');

      const centerX = ctx.canvas.width / 2;
      const centerY = ctx.canvas.height / 2;
      const radialGradient = ctx.createRadialGradient(
        centerX, centerY, 0,
        centerX, centerY, ctx.canvas.width * 0.8
      );
      radialGradient.addColorStop(0, 'rgba(30, 30, 30, 0.15)');
      radialGradient.addColorStop(1, 'rgba(0, 0, 0, 0)');

      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
      ctx.fillStyle = radialGradient;
      ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);

      return 'rgba(0,0,0,0)';
    }
    default:
      return '#000000';
  }
}
