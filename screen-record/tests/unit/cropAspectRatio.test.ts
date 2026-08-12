import { describe, expect, it } from "vitest";
import {
  getActiveCropAspectRatioId,
  getAspectRatioCrop,
  isSourceCrop,
} from "@/lib/cropAspectRatio";
import { resolveCodecAlignedCropGeometry } from "@/lib/videoGeometry";

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
});
