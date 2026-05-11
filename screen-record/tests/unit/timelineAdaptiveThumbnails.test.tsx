import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useTimelineAdaptiveThumbnails } from "@/hooks/useTimelineAdaptiveThumbnails";
import type { VideoSegment } from "@/types/video";

const segment: VideoSegment = {
  trimStart: 0,
  trimEnd: 12,
  trimSegments: [{ id: "trim-1", startTime: 0, endTime: 12 }],
  zoomKeyframes: [],
  textSegments: [],
  subtitleSegments: [],
};

function Harness({
  thumbnailsLength,
  timelineCanvasWidthPx = 1000,
  currentRawVideoPath = "C:/SGT/source.mp4",
  generateThumbnailsForSource,
}: {
  thumbnailsLength: number;
  timelineCanvasWidthPx?: number;
  currentRawVideoPath?: string | null;
  generateThumbnailsForSource: ReturnType<typeof vi.fn>;
}) {
  useTimelineAdaptiveThumbnails({
    timelineCanvasWidthPx,
    segment,
    currentVideo: "http://localhost/media/source.mp4",
    currentRawVideoPath,
    thumbnailsLength,
    isPlaying: false,
    generateThumbnailsForSource,
  });
  return null;
}

describe("useTimelineAdaptiveThumbnails", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("requests the base trim thumbnail strip after project load clears thumbnails", async () => {
    vi.useFakeTimers();
    const generateThumbnailsForSource = vi.fn().mockResolvedValue(undefined);

    render(
      <Harness
        thumbnailsLength={0}
        generateThumbnailsForSource={generateThumbnailsForSource}
      />,
    );

    await act(async () => {
      vi.advanceTimersByTime(220);
    });

    expect(generateThumbnailsForSource).toHaveBeenCalledWith(
      expect.objectContaining({
        filePath: "C:/SGT/source.mp4",
        thumbnailCount: 6,
      }),
    );
  });

  it("does not regenerate the base strip when thumbnails are already present", async () => {
    vi.useFakeTimers();
    const generateThumbnailsForSource = vi.fn().mockResolvedValue(undefined);

    render(
      <Harness
        thumbnailsLength={6}
        timelineCanvasWidthPx={100}
        generateThumbnailsForSource={generateThumbnailsForSource}
      />,
    );

    await act(async () => {
      vi.advanceTimersByTime(220);
    });

    expect(generateThumbnailsForSource).not.toHaveBeenCalled();
  });

  it("requests a denser strip when timeline zoom increases rendered width", async () => {
    vi.useFakeTimers();
    const generateThumbnailsForSource = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <Harness
        thumbnailsLength={6}
        timelineCanvasWidthPx={100}
        generateThumbnailsForSource={generateThumbnailsForSource}
      />,
    );

    await act(async () => {
      vi.advanceTimersByTime(220);
    });
    expect(generateThumbnailsForSource).not.toHaveBeenCalled();

    rerender(
      <Harness
        thumbnailsLength={6}
        timelineCanvasWidthPx={2000}
        generateThumbnailsForSource={generateThumbnailsForSource}
      />,
    );
    await act(async () => {
      vi.advanceTimersByTime(220);
    });

    expect(generateThumbnailsForSource).toHaveBeenCalledWith(
      expect.objectContaining({
        thumbnailCount: 14,
      }),
    );
  });

  it("keeps super-zoomed thumbnail strips dense enough to avoid stretched blocks", async () => {
    vi.useFakeTimers();
    const generateThumbnailsForSource = vi.fn().mockResolvedValue(undefined);

    render(
      <Harness
        thumbnailsLength={100}
        timelineCanvasWidthPx={30_000}
        generateThumbnailsForSource={generateThumbnailsForSource}
      />,
    );

    await act(async () => {
      vi.advanceTimersByTime(220);
    });

    expect(generateThumbnailsForSource).toHaveBeenCalledWith(
      expect.objectContaining({
        thumbnailCount: 200,
      }),
    );
  });
});
