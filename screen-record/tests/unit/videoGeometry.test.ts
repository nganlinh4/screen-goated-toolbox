import { describe, expect, it } from 'vitest';
import {
  coversCanvas,
  getVideoPlacementRect,
  normalizeAutoCanvasSegment,
  resolveCodecAlignedCropGeometry,
} from '@/lib/videoGeometry';
import { isUnmaskedCanvasCoveringVideo } from '@/lib/renderer/videoFrameSurface';
import { getCanvasBaseDimensions } from '@/lib/exportEstimator';

describe('video placement geometry', () => {
  const cropWidth = 2560 * 0.1468023255813954;
  const cropHeight = 1080 * 0.10077519379844962;

  it('resolves the crop and auto canvas to one codec-aligned geometry', () => {
    const geometry = resolveCodecAlignedCropGeometry(2560, 1080, {
      x: 0.45421511627906974,
      y: 0.3169681309216193,
      width: 0.1468023255813954,
      height: 0.10077519379844962,
    });

    expect(geometry.width).toBe(376);
    expect(geometry.height).toBe(108);
    expect(geometry.crop.x * 2560).toBe(1163);
    expect(geometry.crop.y * 1080).toBe(343);
    expect(geometry.crop.width * 2560).toBe(376);
    expect(geometry.crop.height * 1080).toBe(108);

    const rect = getVideoPlacementRect(
      geometry.width,
      geometry.height,
      geometry.crop.width * 2560,
      geometry.crop.height * 1080,
      1,
    );

    expect(rect).toEqual({ width: 376, height: 108, left: 0, top: 0 });
    expect(coversCanvas(rect, 376, 108)).toBe(true);
    expect(isUnmaskedCanvasCoveringVideo(rect, 376, 108, 0)).toBe(true);
  });

  it('retains aspect-preserving containment for custom canvas geometry', () => {
    const rect = getVideoPlacementRect(
      376,
      108,
      cropWidth,
      cropHeight,
      1,
    );

    expect(rect.width).toBeCloseTo(372.923076923077, 9);
    expect(rect.height).toBe(108);
    expect(rect.left).toBeCloseTo(1.5384615384615, 9);
    expect(coversCanvas(rect, 376, 108)).toBe(false);
  });

  it('normalizes only the auto-canvas authority segment', () => {
    const segment = {
      crop: { x: 0, y: 0, width: cropWidth / 2560, height: cropHeight / 1080 },
    } as Parameters<typeof normalizeAutoCanvasSegment>[0];

    expect(normalizeAutoCanvasSegment(segment, undefined, 2560, 1080).crop).toEqual({
      x: 0,
      y: 0,
      width: 376 / 2560,
      height: 108 / 1080,
    });
    expect(normalizeAutoCanvasSegment(segment, { canvasMode: 'custom' }, 2560, 1080)).toBe(segment);
    expect(normalizeAutoCanvasSegment(segment, { canvasMode: 'auto' }, 2560, 1080, false)).toBe(segment);
  });

  it('reports the codec-aligned dimensions to export UI and preserves custom canvas size', () => {
    const segment = {
      crop: {
        x: 0.45421511627906974,
        y: 0.3169681309216193,
        width: 0.1468023255813954,
        height: 0.10077519379844962,
      },
    } as Parameters<typeof getCanvasBaseDimensions>[2];

    expect(getCanvasBaseDimensions(2560, 1080, segment, undefined)).toEqual({
      baseW: 376,
      baseH: 108,
    });
    expect(getCanvasBaseDimensions(2560, 1080, segment, {
      canvasMode: 'custom',
      canvasWidth: 640,
      canvasHeight: 480,
    } as Parameters<typeof getCanvasBaseDimensions>[3])).toEqual({
      baseW: 640,
      baseH: 480,
    });
  });

  it('keeps rounded canvas-covering video on the masked path', () => {
    const rect = getVideoPlacementRect(376, 108, 376, 108, 1);
    expect(isUnmaskedCanvasCoveringVideo(rect, 376, 108, 1)).toBe(false);
  });
});
