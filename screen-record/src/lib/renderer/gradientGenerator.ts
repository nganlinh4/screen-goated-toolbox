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
      const cacheKey = customBackground;
      if (!customBgCache.customBackgroundPattern || customBgCache.customBackgroundCacheKey !== cacheKey) {
        customBgCache.customBackgroundPattern = ctx.createPattern(img, 'no-repeat');
        customBgCache.customBackgroundCacheKey = cacheKey;
      }
    }

    if (customBgCache.customBackgroundPattern) {
      const canvasWidth = ctx.canvas.width;
      const canvasHeight = ctx.canvas.height;
      const imageWidth = customBgCache.customBackgroundImage?.naturalWidth ?? 1;
      const imageHeight = customBgCache.customBackgroundImage?.naturalHeight ?? 1;
      const coverScale = Math.max(
        canvasWidth / imageWidth,
        canvasHeight / imageHeight,
      );
      const offsetX = (canvasWidth - imageWidth * coverScale) / 2;
      const offsetY = (canvasHeight - imageHeight * coverScale) / 2;
      customBgCache.customBackgroundPattern.setTransform(
        new DOMMatrix()
          .translate(offsetX, offsetY)
          .scale(coverScale),
      );
      return customBgCache.customBackgroundPattern;
    }
  }

  return '#000000';
}
