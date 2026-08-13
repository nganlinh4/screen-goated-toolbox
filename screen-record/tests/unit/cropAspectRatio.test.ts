import { describe, expect, it } from "vitest";
import {
  getActiveCropAspectRatioId,
  getAspectRatioCrop,
  isSourceCrop,
  resizeCropWithAspectRatio,
} from "@/lib/cropAspectRatio";
import { resolveCodecAlignedCropGeometry } from "@/lib/videoGeometry";
import { CROP_ASPECT_RATIO_PRESETS } from "@/lib/aspectRatioPresets";

describe("crop aspect ratio geometry", () => {
  it("fits portrait presets inside the source with codec-aligned dimensions", () => {
    const crop = getAspectRatioCrop(1920, 1080, 9, 16);
    const geometry = resolveCodecAlignedCropGeometry(1920, 1080, crop);

    expect(geometry.width).toBe(608);
    expect(geometry.height).toBe(1080);
    expect(geometry.width % 2).toBe(0);
    expect(geometry.height % 2).toBe(0);
    expect(getActiveCropAspectRatioId(1920, 1080, crop)).toBe("portrait-9-16");
  });

  it("keeps the current crop center when the target ratio fits within bounds", () => {
    const crop = getAspectRatioCrop(1920, 1080, 1, 1, {
      x: 0.1,
      y: 0.1,
      width: 0.4,
      height: 0.6,
    });
    const geometry = resolveCodecAlignedCropGeometry(1920, 1080, crop);

    expect(geometry.width).toBe(1080);
    expect(geometry.height).toBe(1080);
    expect(crop.x + crop.width / 2).toBeCloseTo(0.3, 3);
    expect(crop.y + crop.height / 2).toBeCloseTo(0.5, 3);
  });

  it("clamps a preserved center so the preset never leaves the source", () => {
    const crop = getAspectRatioCrop(1920, 1080, 1, 1, {
      x: 0.8,
      y: 0.8,
      width: 0.2,
      height: 0.2,
    });

    expect(crop.x + crop.width).toBeLessThanOrEqual(1);
    expect(crop.y + crop.height).toBeLessThanOrEqual(1);
    expect(crop.x).toBeGreaterThanOrEqual(0);
    expect(crop.y).toBeGreaterThanOrEqual(0);
  });

  it("recognizes full-frame codec alignment on odd-sized sources", () => {
    const alignedSource = resolveCodecAlignedCropGeometry(1365, 767, {
      x: 0,
      y: 0,
      width: 1,
      height: 1,
    }).crop;

    expect(isSourceCrop(1365, 767, alignedSource)).toBe(true);
    expect(isSourceCrop(1365, 767, getAspectRatioCrop(1365, 767, 1, 1))).toBe(false);
  });

  it("reports custom geometry when no preset matches", () => {
    expect(getActiveCropAspectRatioId(1920, 1080, {
      x: 0.1,
      y: 0.1,
      width: 0.45,
      height: 0.7,
    })).toBeNull();
  });

  it.each(CROP_ASPECT_RATIO_PRESETS)(
    "recognizes $label after codec alignment",
    (preset) => {
      const crop = getAspectRatioCrop(
        3840,
        2160,
        preset.width,
        preset.height,
      );

      expect(getActiveCropAspectRatioId(3840, 2160, crop)).toBe(preset.id);
    },
  );

  it("keeps a corner resize locked while preserving the opposite anchor", () => {
    const start = getAspectRatioCrop(1920, 1080, 9, 16);
    const resized = resizeCropWithAspectRatio(
      1920,
      1080,
      start,
      "se",
      -0.08,
      -0.1,
      9,
      16,
    );
    const geometry = resolveCodecAlignedCropGeometry(1920, 1080, resized);

    expect(geometry.width / geometry.height).toBeCloseTo(9 / 16, 2);
    expect(resized.x).toBeCloseTo(start.x, 3);
    expect(resized.y).toBeCloseTo(start.y, 3);
    expect(resized.width).toBeLessThan(start.width);
    expect(resized.height).toBeLessThan(start.height);
  });

  it("centers the secondary axis during a locked edge resize", () => {
    const start = getAspectRatioCrop(1920, 1080, 1, 1);
    const resized = resizeCropWithAspectRatio(
      1920,
      1080,
      start,
      "e",
      -0.12,
      0,
      1,
      1,
    );
    const geometry = resolveCodecAlignedCropGeometry(1920, 1080, resized);

    expect(geometry.width).toBe(geometry.height);
    expect(resized.x).toBeCloseTo(start.x, 3);
    expect(resized.y + resized.height / 2).toBeCloseTo(
      start.y + start.height / 2,
      3,
    );
  });

  it("clamps locked resizing at the source boundary and minimum size", () => {
    const start = getAspectRatioCrop(1920, 1080, 21, 9);
    const expanded = resizeCropWithAspectRatio(
      1920,
      1080,
      start,
      "nw",
      -1,
      -1,
      21,
      9,
    );
    const collapsed = resizeCropWithAspectRatio(
      1920,
      1080,
      start,
      "se",
      -1,
      -1,
      21,
      9,
    );

    expect(expanded.x).toBeGreaterThanOrEqual(0);
    expect(expanded.y).toBeGreaterThanOrEqual(0);
    expect(expanded.x + expanded.width).toBeLessThanOrEqual(1);
    expect(expanded.y + expanded.height).toBeLessThanOrEqual(1);
    expect(collapsed.width).toBeGreaterThanOrEqual(0.05);
    expect(collapsed.height).toBeGreaterThanOrEqual(0.05);
  });

  it.each([
    ["nw", 0.04, 0.04],
    ["n", 0, 0.04],
    ["ne", -0.04, 0.04],
    ["w", 0.04, 0],
    ["e", -0.04, 0],
    ["sw", 0.04, -0.04],
    ["s", 0, -0.04],
    ["se", -0.04, -0.04],
  ] as const)("keeps the %s handle on the selected ratio", (handle, deltaX, deltaY) => {
    const start = getAspectRatioCrop(1920, 1080, 4, 5);
    const resized = resizeCropWithAspectRatio(
      1920,
      1080,
      start,
      handle,
      deltaX,
      deltaY,
      4,
      5,
    );
    const geometry = resolveCodecAlignedCropGeometry(1920, 1080, resized);

    expect(geometry.width / geometry.height).toBeCloseTo(4 / 5, 2);
    expect(resized.x).toBeGreaterThanOrEqual(0);
    expect(resized.y).toBeGreaterThanOrEqual(0);
    expect(resized.x + resized.width).toBeLessThanOrEqual(1);
    expect(resized.y + resized.height).toBeLessThanOrEqual(1);
  });
});
