import type { BackgroundConfig } from '@/types/video';
import { isBuiltInBackgroundId } from '@/lib/backgroundPresets';
import {
  getBuiltInBackgroundToken,
  parseBuiltInBackgroundToken,
  fillBuiltInBackground,
  type BuiltInBackgroundCache,
} from './builtInBackgrounds';

export type GradientCache = BuiltInBackgroundCache;
export { fillBuiltInBackground, parseBuiltInBackgroundToken };

export interface CustomBgCache {
  customBackgroundImage: HTMLImageElement | null;
  customBackgroundPattern: CanvasPattern | null;
  lastCustomBackground: string | undefined;
  customBackgroundCacheKey: string | undefined;
}

export type OnCustomBgLoaded = () => void;

export function getBackgroundStyle(
  ctx: CanvasRenderingContext2D,
  type: BackgroundConfig['backgroundType'],
  customBgCache: CustomBgCache,
  onCustomBgLoaded: OnCustomBgLoaded,
  customBackground?: string
): string | CanvasPattern {
  if (isBuiltInBackgroundId(type)) {
    return getBuiltInBackgroundToken(type);
  }

  if (type !== 'custom') {
    return '#000000';
  }

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
