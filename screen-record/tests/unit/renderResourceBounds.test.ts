import { describe, expect, it } from "vitest";
import {
  MAX_OVERLAY_ATLAS_SIZE,
  packOverlayAtlasRects,
} from "@/lib/renderer/overlayBaker";
import {
  getBoundedCanvasScale,
  getBoundedCanvasSize,
  MAX_OUTPUT_CANVAS_DIMENSION,
  MAX_OUTPUT_CANVAS_PIXELS,
  resolveOutputCanvasDimensions,
} from "@/lib/canvasRenderBudget";
import { resolveExportDimensions } from "@/lib/exportEstimator";
import {
  getBuiltInBackgroundRenderSize,
  MAX_CACHED_BACKGROUND_PIXELS,
  setCachedBuiltInBackground,
} from "@/lib/renderer/builtInBackgroundPreview";
import { resolveMotionBlurTiming } from "@/lib/renderer/motionBlurTiming";
import type { VideoSegment } from "@/types/video";
import { getActiveTimedSegments } from "@/lib/timedSegmentIndex";
import { getLruCacheValue, setLruCacheValue } from "@/lib/boundedCache";

describe("render resource bounds", () => {
  it("allocates only the used atlas area", () => {
    const atlas = packOverlayAtlasRects([
      { width: 120, height: 40 },
      { width: 80, height: 30 },
    ]);

    expect(atlas.width).toBeLessThan(MAX_OVERLAY_ATLAS_SIZE);
    expect(atlas.height).toBe(40);
  });

  it("rejects atlas content before it can be clipped", () => {
    expect(() => packOverlayAtlasRects([
      { width: MAX_OVERLAY_ATLAS_SIZE + 1, height: 20 },
    ])).toThrow(/too large/i);
    expect(() => packOverlayAtlasRects(
      Array.from({ length: 17 }, () => ({ width: 4096, height: 256 })),
    )).toThrow(/exceed export capacity/i);
  });

  it("caps temporary canvases by both area and dimension", () => {
    const scale = getBoundedCanvasScale(3840, 2160, 4, 4096 * 4096, 8192);
    expect(3840 * 2160 * scale * scale).toBeLessThanOrEqual(4096 * 4096 + 1);
    expect(3840 * scale).toBeLessThanOrEqual(8192);

    const huge = getBoundedCanvasSize(1_000_000, 500_000, 1_000_000, 2048);
    expect(huge.width).toBeLessThanOrEqual(2048);
    expect(huge.width * huge.height).toBeLessThanOrEqual(1_000_000 + 1);
  });

  it("uses the same bounded dimensions for preview and export", () => {
    const preview = resolveOutputCanvasDimensions(100_001, 50_001);
    const exported = resolveExportDimensions(0, 0, 100_001, 50_001);
    expect(exported).toEqual(preview);
    expect(preview.width).toBeLessThanOrEqual(MAX_OUTPUT_CANVAS_DIMENSION);
    expect(preview.height).toBeLessThanOrEqual(MAX_OUTPUT_CANVAS_DIMENSION);
    expect(preview.width * preview.height).toBeLessThanOrEqual(
      MAX_OUTPUT_CANVAS_PIXELS,
    );
    expect(preview.width % 2).toBe(0);
    expect(preview.height % 2).toBe(0);
  });

  it("derives preview blur timing from export fps and source speed", () => {
    const segment = {
      trimStart: 0,
      trimEnd: 1,
      speedPoints: [{ time: 0, speed: 2 }, { time: 1, speed: 2 }],
    } as VideoSegment;
    const timing = resolveMotionBlurTiming(segment, 0.5, 30, 100, 50, 0);

    expect(timing.zoomShutterSec).toBeCloseTo(2 / 30, 12);
    expect(timing.panShutterSec).toBeCloseTo(1 / 30, 12);
    expect(timing.sampleCount).toBe(8);
  });

  it("bounds quality background rasters and their cache", () => {
    const size = getBuiltInBackgroundRenderSize(7680, 4320, false);
    expect(size.width * size.height).toBeLessThanOrEqual(3840 * 2160 + 1);
    expect(Math.max(size.width, size.height)).toBeLessThanOrEqual(4096);

    const cache = new Map<string, HTMLCanvasElement>();
    for (let index = 0; index < 20; index += 1) {
      setCachedBuiltInBackground(cache, String(index), {
        width: 2048,
        height: 1024,
      } as HTMLCanvasElement);
    }
    const cachedPixels = [...cache.values()].reduce(
      (total, canvas) => total + canvas.width * canvas.height,
      0,
    );
    expect(cachedPixels).toBeLessThanOrEqual(MAX_CACHED_BACKGROUND_PIXELS);
  });

  it("queries only active timed overlays while preserving layer order", () => {
    const segments = [
      { id: "long", startTime: 0, endTime: 10 },
      { id: "past", startTime: 1, endTime: 2 },
      { id: "top", startTime: 4, endTime: 6 },
    ];
    expect(getActiveTimedSegments(segments, 5).map((entry) => entry.id)).toEqual([
      "long",
      "top",
    ]);
  });

  it("evicts the oldest cached media entry and refreshes cache hits", () => {
    const cache = new Map<string, number>();
    setLruCacheValue(cache, "a", 1, 2);
    setLruCacheValue(cache, "b", 2, 2);
    expect(getLruCacheValue(cache, "a")).toBe(1);
    setLruCacheValue(cache, "c", 3, 2);

    expect([...cache.keys()]).toEqual(["a", "c"]);
  });
});
