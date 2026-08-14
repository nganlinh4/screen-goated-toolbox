import { describe, expect, it } from "vitest";
import { buildPlaybackRenderOptions } from "@/hooks/videoPlaybackRenderState";
import type { BackgroundConfig, VideoSegment } from "@/types/video";

function segment(): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    zoomKeyframes: [],
    textSegments: [],
    subtitleSegments: [],
  };
}

describe("buildPlaybackRenderOptions", () => {
  it("copies the latest background and webcam settings into render state", () => {
    const background = {
      scale: 87,
      borderRadius: 14,
      backgroundType: "solid",
      shadow: 2,
      cursorScale: 4,
    } as BackgroundConfig;
    const webcam = { visible: true, maxSizePercent: 31 };
    const options = buildPlaybackRenderOptions({
      segment: segment(),
      backgroundConfig: background,
      webcamConfig: webcam,
      mousePositions: [],
      isCropping: false,
      outputFrameRate: 30,
    });

    expect(options.backgroundConfig.scale).toBe(87);
    expect(options.webcamConfig?.maxSizePercent).toBe(31);
    expect(options.outputFrameRate).toBe(30);
    expect(options.backgroundConfig).not.toBe(background);
    expect(options.webcamConfig).not.toBe(webcam);
  });

  it("uses the canonical crop editing render overrides", () => {
    const options = buildPlaybackRenderOptions({
      segment: { ...segment(), crop: { x: 0.1, y: 0.1, width: 0.8, height: 0.8 } },
      backgroundConfig: { scale: 70 } as BackgroundConfig,
      mousePositions: [],
      isCropping: true,
      outputFrameRate: 60,
    });

    expect(options.segment.crop).toBeUndefined();
    expect(options.backgroundConfig.scale).toBe(100);
    expect(options.backgroundConfig.borderRadius).toBe(0);
  });
});
