import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TrimTrack, buildTrimThumbnailMask } from "@/components/timeline/TrimTrack";
import type { VideoSegment } from "@/types/video";

function segmentWithTrims(count: number, duration: number): VideoSegment {
  const span = duration / count;
  return {
    trimStart: 0,
    trimEnd: duration,
    zoomKeyframes: [],
    textSegments: [],
    trimSegments: Array.from({ length: count }, (_, index) => ({
      id: `trim-${index}`,
      startTime: index * span,
      endTime: index * span + span * 0.6,
    })),
  } as VideoSegment;
}

describe("TrimTrack thumbnails", () => {
  it("renders the thumbnail strip once however many trim segments exist", () => {
    const thumbnails = Array.from({ length: 240 }, (_, index) => `data:image/png;base64,${index}`);
    const { container } = render(
      <TrimTrack
        segment={segmentWithTrims(30, 600)}
        duration={600}
        thumbnails={thumbnails}
        onTrimDragStart={vi.fn()}
        onTrimSplit={vi.fn()}
        onTrimAddSegment={vi.fn()}
      />,
    );
    expect(container.querySelectorAll(".trim-thumb")).toHaveLength(240);
    expect(container.querySelectorAll(".trim-active-region")).toHaveLength(30);
    const strip = container.querySelector<HTMLElement>(".trim-thumbnails")!;
    expect(strip.style.maskImage).toBe(buildTrimThumbnailMask(segmentWithTrims(30, 600).trimSegments!, 600));
  });

  it("masks kept segments opaque and trimmed-out time dim", () => {
    const mask = buildTrimThumbnailMask([{ id: "a", startTime: 10, endTime: 30 }], 100, 0.06);
    expect(mask).toBe(
      "linear-gradient(to right, rgba(0, 0, 0, 0.06) 0%, rgba(0, 0, 0, 0.06) 10%, rgb(0, 0, 0) 10%, rgb(0, 0, 0) 30%, rgba(0, 0, 0, 0.06) 30%, rgba(0, 0, 0, 0.06) 100%)",
    );
  });
});
