import { describe, expect, it } from "vitest";
import { buildPlaybackStructureSignature } from "@/hooks/videoPlaybackRenderState";
import type { VideoSegment } from "@/types/video";

function segment(deviceAudioOffsetSec: number): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    zoomKeyframes: [],
    textSegments: [],
    subtitleSegments: [],
    deviceAudioOffsetSec,
  };
}

describe("buildPlaybackStructureSignature", () => {
  it("invalidates active playback when the Device Audio delay changes", () => {
    expect(buildPlaybackStructureSignature(segment(0))).not.toBe(
      buildPlaybackStructureSignature(segment(0.5)),
    );
  });

  it("invalidates active playback when a manual zoom transition is linked", () => {
    const unlinked = {
      ...segment(0),
      zoomBlocks: [{
        id: "zoom-1",
        startTime: 1,
        endTime: 2,
        easeIn: 0.2,
        easeOut: 0.2,
        zoomFactor: 2,
        positionX: 0.5,
        positionY: 0.5,
        enabled: true,
      }],
    } satisfies VideoSegment;
    const linked = {
      ...unlinked,
      zoomBlocks: unlinked.zoomBlocks.map((block) => ({
        ...block,
        directTransitionToNext: true,
      })),
    } satisfies VideoSegment;

    expect(buildPlaybackStructureSignature(unlinked)).not.toBe(
      buildPlaybackStructureSignature(linked),
    );
  });
});
