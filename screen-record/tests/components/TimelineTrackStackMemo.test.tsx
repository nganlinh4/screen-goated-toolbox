import { render } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { TimelineArea } from "@/components/timeline/TimelineArea";
import type { TimelineAreaProps } from "@/components/timeline/TimelineAreaTypes";
import type { VideoSegment } from "@/types/video";

const stackProps: Array<Record<string, unknown>> = [];

vi.mock("@/components/timeline/TimelineTrackStack", () => ({
  TimelineTrackStack: (props: Record<string, unknown>) => {
    stackProps.push(props);
    return null;
  },
}));

const segment = {
  trimStart: 0,
  trimEnd: 10,
  trimSegments: [{ id: "trim", startTime: 0, endTime: 10 }],
  zoomBlocks: [],
  textSegments: [],
  keystrokeMode: "keyboard",
  keystrokeEvents: [],
} as unknown as VideoSegment;

function baseProps(): TimelineAreaProps {
  return {
    duration: 10,
    currentTime: 0,
    segment,
    thumbnails: [],
    timelineRef: createRef<HTMLDivElement>(),
    videoRef: createRef<HTMLVideoElement>(),
    editingKeyframeId: null,
    editingTextId: null,
    editingSubtitleId: null,
    editingKeystrokeSegmentId: null,
    setCurrentTime: vi.fn(),
    setEditingKeyframeId: vi.fn(),
    setEditingTextId: vi.fn(),
    setEditingSubtitleId: vi.fn(),
    setEditingKeystrokeSegmentId: vi.fn(),
    setEditingPointerId: vi.fn(),
    setActivePanel: vi.fn(),
    setSegment: vi.fn(),
    isDeviceAudioAvailable: true,
    isMicAudioAvailable: true,
    isWebcamAvailable: true,
    currentRawVideoPath: "",
    currentRawMicAudioPath: "",
    beginBatch: vi.fn(),
    commitBatch: vi.fn(),
    selectedTextIds: [],
    selectedSubtitleIds: [],
  };
}

describe("TimelineArea track stack memoization", () => {
  it("passes identical track-stack props when only currentTime changes", () => {
    const props = baseProps();
    const { rerender } = render(<TimelineArea {...props} />);
    const before = stackProps[stackProps.length - 1];
    rerender(<TimelineArea {...props} currentTime={1} />);
    rerender(<TimelineArea {...props} currentTime={2} isPlaying />);
    rerender(<TimelineArea {...props} currentTime={3} isPlaying />);
    const after = stackProps[stackProps.length - 1];

    const changed = Object.keys({ ...before, ...after }).filter(
      (key) => key !== "isPlaying" && !Object.is(before[key], after[key]),
    );
    expect(changed).toEqual([]);
  });
});
