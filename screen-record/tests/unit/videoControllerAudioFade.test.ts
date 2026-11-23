import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VideoController } from "@/lib/videoController";
import { getAudioFootprintEdgeFadeMultiplier } from "@/lib/audioEdgeFade";
import type { VideoSegment } from "@/types/video";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: { stopAnimation: vi.fn() } }));
vi.mock("@/lib/videoControllerRendering", () => ({
  renderFrame: vi.fn(), generateThumbnail: vi.fn(), renderImmediate: vi.fn(),
}));

function segment(overrides: Partial<VideoSegment> = {}): VideoSegment {
  return {
    trimStart: 2,
    trimEnd: 8,
    trimSegments: [{ id: "keep", startTime: 2, endTime: 8 }],
    zoomKeyframes: [],
    textSegments: [],
    speedPoints: [{ time: 0, speed: 1 }, { time: 10, speed: 1 }],
    deviceAudioPoints: [{ time: 0, volume: 1 }, { time: 10, volume: 1 }],
    deviceAudioOffsetSec: 0,
    ...overrides,
  } as VideoSegment;
}

describe("preview audio edge fade follows the trimmed export footprint", () => {
  it("is silent outside the kept region and fades in at the trimmed start", () => {
    const options = { trimSegments: segment().trimSegments!, speedPoints: segment().speedPoints, sourceStartSec: 0 };
    expect(getAudioFootprintEdgeFadeMultiplier(0.06, options)).toBe(0);
    expect(getAudioFootprintEdgeFadeMultiplier(2.03, options)).toBeGreaterThan(0);
    expect(getAudioFootprintEdgeFadeMultiplier(2.03, options)).toBeLessThan(0.5);
    expect(getAudioFootprintEdgeFadeMultiplier(5, options)).toBe(1);
    expect(getAudioFootprintEdgeFadeMultiplier(7.97, options)).toBeLessThan(0.5);
  });

  it("measures the fade in output time through the speed curve", () => {
    const base = { trimSegments: segment().trimSegments!, sourceStartSec: 0 };
    // 0.15 source seconds in: 0.15 s of output at 1x (past the 0.12 s fade), 0.075 s at 2x.
    expect(getAudioFootprintEdgeFadeMultiplier(2.15, { ...base, speedPoints: [{ time: 0, speed: 1 }] })).toBe(1);
    expect(getAudioFootprintEdgeFadeMultiplier(2.15, { ...base, speedPoints: [{ time: 0, speed: 2 }, { time: 10, speed: 2 }] }))
      .toBeLessThan(1);
  });

  it("starts an offset source's footprint at its offset, like export", () => {
    const options = { trimSegments: [{ id: "all", startTime: 0, endTime: 8 }], speedPoints: [], sourceStartSec: 3 };
    expect(getAudioFootprintEdgeFadeMultiplier(1, options)).toBe(0);
    expect(getAudioFootprintEdgeFadeMultiplier(3.02, options)).toBeLessThan(0.5);
    expect(getAudioFootprintEdgeFadeMultiplier(4, options)).toBe(1);
  });
});

describe("VideoController preview volume and duration ownership", () => {
  let video: HTMLVideoElement;
  let controller: VideoController;
  let currentTime = 0;
  let onDurationChange: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
    video = document.createElement("video");
    Object.defineProperties(video, {
      duration: { value: 10, configurable: true },
      currentTime: { get: () => currentTime, set: (value: number) => { currentTime = value; }, configurable: true },
    });
    onDurationChange = vi.fn();
    controller = new VideoController({ videoRef: video, canvasRef: document.createElement("canvas"),
      tempCanvasRef: document.createElement("canvas"), onDurationChange });
  });
  afterEach(() => { controller.destroy(); });

  const render = (seg: VideoSegment) =>
    controller.updateRenderOptions({ segment: seg, backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" } as never, mousePositions: [] });

  it("plays the trimmed start with the export fade-in instead of full volume", () => {
    currentTime = 2.03;
    render(segment());
    expect(video.volume).toBeGreaterThan(0);
    expect(video.volume).toBeLessThan(0.5);
    currentTime = 5;
    render(segment());
    expect(video.volume).toBe(1);
  });

  it("reports duration changes without writing into the editor's segment", () => {
    const owned = segment({ trimEnd: 20, trimSegments: [{ id: "keep", startTime: 0, endTime: 20 }] });
    const zeroEnd = segment({ trimEnd: 0 });
    render(owned);
    video.dispatchEvent(new Event("durationchange"));
    expect(owned.trimEnd).toBe(20);
    render(zeroEnd);
    video.dispatchEvent(new Event("durationchange"));
    expect(zeroEnd.trimEnd).toBe(0);
    expect(onDurationChange).toHaveBeenCalledWith(10);
  });
});
