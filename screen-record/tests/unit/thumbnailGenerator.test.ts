import { describe, expect, it, vi, afterEach } from "vitest";
import { thumbnailGenerator } from "@/lib/thumbnailGenerator";
import type { VideoSegment } from "@/types/video";

type TestWindow = Window & {
  invoke?: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
};

const segment: VideoSegment = {
  id: "video-1",
  startTime: 0,
  endTime: 10,
  trimStart: 0,
  trimEnd: 10,
  trimSegments: [
    { id: "a", startTime: 1, endTime: 3 },
    { id: "b", startTime: 7, endTime: 9 },
  ],
  effects: [],
  audioEnabled: true,
  volume: 1,
  playbackRate: 1,
  speedPoints: [],
  zoomKeyframes: [],
};

describe("thumbnailGenerator", () => {
  afterEach(() => {
    delete (window as TestWindow).invoke;
    thumbnailGenerator.destroy();
  });

  it("uses native exact source times for trimmed timeline thumbnails", async () => {
    const invoke = vi.fn(async () => ["frame-a", "frame-b", "frame-c"]);
    (window as TestWindow).invoke = invoke as TestWindow["invoke"];

    const thumbnails = await thumbnailGenerator.generateSegmentThumbnails(
      "http://localhost/video.mp4",
      segment,
      10,
      3,
      {
        filePath: "C:\\SGT-Test\\video.mp4",
        width: 240,
        height: 135,
        quality: 0.72,
      },
    );

    expect(thumbnails).toEqual(["frame-a", "frame-b", "frame-c"]);
    expect(invoke).toHaveBeenCalledWith("generate_timeline_thumbnails", {
      path: "C:\\SGT-Test\\video.mp4",
      times: [1, 3, 9],
      width: 240,
      height: 135,
      quality: 0.72,
    });
  });

  it("reuses cached native timeline thumbnails", async () => {
    const invoke = vi.fn(async () => ["frame-a", "frame-b"]);
    (window as TestWindow).invoke = invoke as TestWindow["invoke"];

    const options = {
      filePath: "C:\\SGT-Test\\cached.mp4",
      width: 240,
      height: 135,
      quality: 0.72,
    };
    await thumbnailGenerator.generateSegmentThumbnails(
      "http://localhost/video.mp4",
      segment,
      10,
      2,
      options,
    );
    const thumbnails = await thumbnailGenerator.generateSegmentThumbnails(
      "http://localhost/video.mp4",
      segment,
      10,
      2,
      options,
    );

    expect(thumbnails).toEqual(["frame-a", "frame-b"]);
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
