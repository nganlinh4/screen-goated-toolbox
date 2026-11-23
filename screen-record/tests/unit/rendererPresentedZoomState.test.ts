import { beforeAll, describe, expect, it, vi } from "vitest";
import type { VideoSegment, ZoomKeyframe } from "@/types/video";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";

type RendererModule = typeof import("@/lib/renderer");
let renderer: RendererModule["videoRenderer"];

beforeAll(async () => {
  const baseGetContext = HTMLCanvasElement.prototype.getContext;
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement, id: string) {
    const ctx = (baseGetContext as (id: string) => Record<string, unknown> | null).call(this, id);
    if (!ctx) return ctx;
    return new Proxy(ctx, {
      get: (target, key) => key in target ? target[key as string]
        : key === "createImageData"
          ? (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h })
          : () => ({ addColorStop: () => {} }),
    });
  } as typeof HTMLCanvasElement.prototype.getContext);
  vi.stubGlobal("OffscreenCanvas", class {
    constructor(public width: number, public height: number) {}
    getContext() { return document.createElement("canvas").getContext("2d"); }
  });
  ({ videoRenderer: renderer } = await import("@/lib/renderer"));
});

const segment = {
  trimStart: 0, trimEnd: 10, trimSegments: [{ id: "t", startTime: 0, endTime: 10 }],
  zoomBlocks: [{ id: "z", startTime: 1, endTime: 9, zoomFactor: 2, positionX: 0.3, positionY: 0.7,
    easeIn: 0.5, easeOut: 0.5, followCursor: false, enabled: true }],
} as unknown as VideoSegment;

describe("presented camera state", () => {
  it("is not overwritten by sub-sample zoom evaluations or the export webcam bake", () => {
    const state = (renderer as unknown as { state: {
      calculateCurrentZoomState: (t: number, s: VideoSegment, w: number, h: number) => ZoomKeyframe;
      recordPresentedZoomState: (z: ZoomKeyframe) => void;
    } }).state;
    const presented = state.calculateCurrentZoomState(5, segment, 1280, 720);
    state.recordPresentedZoomState(presented);

    state.calculateCurrentZoomState(1.1, segment, 1280, 720);
    const frames = renderer.iterateBakedWebcamFrames(
      segment, { ...DEFAULT_WEBCAM_CONFIG, visible: true }, 3840, 2160,
      { width: 3840, height: 2160 }, 16 / 9, 2,
    );
    for (const frame of frames) void frame;

    expect(renderer.getLastCalculatedState()).toBe(presented);
  });
});
