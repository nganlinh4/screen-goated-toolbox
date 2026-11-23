import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VideoController } from "@/lib/videoController";
import type { VideoSegment } from "@/types/video";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: { stopAnimation: vi.fn() } }));
vi.mock("@/lib/videoControllerRendering", () => ({
  renderFrame: vi.fn(), generateThumbnail: vi.fn(), renderImmediate: vi.fn(),
}));

describe("play request cancellation", () => {
  let video: HTMLVideoElement;
  let controller: VideoController;
  let playSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(async () => {
    vi.stubGlobal("MediaError", { MEDIA_ERR_SRC_NOT_SUPPORTED: 4 });
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
    playSpy = vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(async () => {});
    video = document.createElement("video");
    let mediaTime = 0;
    Object.defineProperties(video, {
      duration: { value: 10, configurable: true },
      videoWidth: { value: 640, configurable: true },
      videoHeight: { value: 360, configurable: true },
      paused: { get: () => true, configurable: true },
      currentTime: { get: () => mediaTime, set: (value: number) => { mediaTime = value; }, configurable: true },
    });
    controller = new VideoController({ videoRef: video, canvasRef: document.createElement("canvas"),
      tempCanvasRef: document.createElement("canvas") });
    const loaded = controller.loadVideo({ videoUrl: "blob:clip" });
    video.dispatchEvent(new Event("loadedmetadata"));
    video.dispatchEvent(new Event("canplaythrough"));
    await loaded;
    controller.updateRenderOptions({
      segment: { trimStart: 0, trimEnd: 10, trimSegments: [{ id: "t", startTime: 4, endTime: 8 }] } as VideoSegment,
      backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
      mousePositions: [],
    } as Parameters<VideoController["updateRenderOptions"]>[0]);
  });
  afterEach(() => { controller.destroy(); vi.unstubAllGlobals(); });

  it("does not start playback after the request was cancelled by a pause during its seek", () => {
    controller.play();
    expect(playSpy).not.toHaveBeenCalled();
    video.dispatchEvent(new Event("pause"));
    video.dispatchEvent(new Event("seeked"));
    expect(playSpy).not.toHaveBeenCalled();
  });

  it("starts playback once the seek for an uncancelled request completes", () => {
    controller.play();
    video.dispatchEvent(new Event("seeked"));
    expect(playSpy).toHaveBeenCalledOnce();
  });
});
