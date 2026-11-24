import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VideoController } from "@/lib/videoController";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: { stopAnimation: vi.fn() } }));
vi.mock("@/lib/videoControllerRendering", () => ({
  renderFrame: vi.fn(), generateThumbnail: vi.fn(), renderImmediate: vi.fn(),
}));

describe("video source loading", () => {
  let video: HTMLVideoElement;
  let controller: VideoController;
  let onReady: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("MediaError", { MEDIA_ERR_SRC_NOT_SUPPORTED: 4 });
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
    video = document.createElement("video");
    Object.defineProperties(video, {
      duration: { value: 10, configurable: true },
      videoWidth: { value: 640, configurable: true },
      videoHeight: { value: 360, configurable: true },
    });
    onReady = vi.fn();
    controller = new VideoController({ videoRef: video, canvasRef: document.createElement("canvas"),
      tempCanvasRef: document.createElement("canvas"), onVideoReady: onReady });
  });
  afterEach(() => { controller.destroy(); vi.useRealTimers(); vi.unstubAllGlobals(); });

  const makeReady = () => {
    video.dispatchEvent(new Event("loadedmetadata"));
    video.dispatchEvent(new Event("canplaythrough"));
  };

  it("rejects media errors and removes listeners before the next source", async () => {
    const first = controller.loadVideo({ videoUrl: "blob:bad" });
    const rejected = expect(first).rejects.toThrow("Failed to load video");
    Object.defineProperty(video, "error", { value: { message: "", code: 4 }, configurable: true });
    video.dispatchEvent(new Event("error"));
    await rejected;
    expect(controller.isReady).toBe(false);
    video.dispatchEvent(new Event("loadeddata"));
    expect(controller.isReady).toBe(false);
    Object.defineProperty(video, "error", { value: null, configurable: true });
    const second = controller.loadVideo({ videoUrl: "blob:good" });
    video.dispatchEvent(new Event("error"));
    makeReady();
    await expect(second).resolves.toBe("blob:good");
    expect(controller.isReady).toBe(true);
    expect(onReady.mock.calls.filter(([ready]) => ready)).toHaveLength(1);
  });

  it("cancels an older fetch even when it resolves after a newer load", async () => {
    let resolveFetch!: (response: Response) => void;
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>((resolve) => { resolveFetch = resolve; })));
    const first = controller.loadVideo({ videoUrl: "https://example.test/slow.mp4" });
    const rejected = expect(first).rejects.toMatchObject({ name: "AbortError" });
    const second = controller.loadVideo({ videoUrl: "blob:new" });
    await rejected;
    resolveFetch(new Response("old"));
    makeReady();
    await expect(second).resolves.toBe("blob:new");
    expect(video.getAttribute("src")).toBe("blob:new");
  });

  it("cancels a pending initial seek without completing the replacement load", async () => {
    const first = controller.loadVideo({ videoUrl: "blob:old", initialTime: 3 });
    makeReady();
    const rejected = expect(first).rejects.toMatchObject({ name: "AbortError" });
    const second = controller.loadVideo({ videoUrl: "blob:new" });
    await rejected;
    video.dispatchEvent(new Event("seeked"));
    await vi.advanceTimersByTimeAsync(900);
    expect(controller.isReady).toBe(false);
    makeReady();
    await expect(second).resolves.toBe("blob:new");
    expect(onReady.mock.calls.filter(([ready]) => ready)).toHaveLength(1);
  });

  it("times out stalled loads and cancels pending loads on destruction", async () => {
    const stalled = controller.loadVideo({ videoUrl: "blob:stalled" });
    const rejected = expect(stalled).rejects.toThrow("Video load timed out");
    await vi.advanceTimersByTimeAsync(30000);
    await rejected;
    expect(controller.isReady).toBe(false);
    const next = controller.loadVideo({ videoUrl: "blob:pending" });
    const cancelled = expect(next).rejects.toMatchObject({ name: "AbortError" });
    controller.destroy();
    await cancelled;
    makeReady();
    expect(controller.isReady).toBe(false);
  });

  it("revokes newly created blob URLs when loading fails", async () => {
    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:created"), revokeObjectURL: vi.fn() }));
    const load = controller.loadVideo({ videoBlob: new Blob(["video"]) });
    const rejected = expect(load).rejects.toThrow("Failed to load video");
    Object.defineProperty(video, "error", { value: { message: "", code: 4 }, configurable: true });
    video.dispatchEvent(new Event("error"));
    await rejected;
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:created");
  });
});
