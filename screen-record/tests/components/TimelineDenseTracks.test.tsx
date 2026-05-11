import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SubtitleTrack } from "@/components/timeline/SubtitleTrack";
import { ImportedAudioTrack } from "@/components/timeline/ImportedAudioTrack";
import { NarrationTrack } from "@/components/timeline/NarrationTrack";
import { TextTrack } from "@/components/timeline/TextTrack";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";
import type { ImportedAudioSegment, NarrationSegment, SubtitleSegment, TextSegment, VideoSegment } from "@/types/video";

function subtitles(count: number): SubtitleSegment[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `s${index}`,
    startTime: index * 0.1,
    endTime: index * 0.1 + 0.04,
    text: `Subtitle ${index}`,
    style: defaultSubtitleStyle(),
  }));
}

function segment(count: number): VideoSegment {
  const subtitleSegments = subtitles(count);
  return {
    trimStart: 0,
    trimEnd: 100,
    zoomKeyframes: [],
    textSegments: [],
    subtitleSegments,
    subtitleTracks: [{ id: "subtitle-track-original", kind: "original", segments: subtitleSegments }],
    activeSubtitleView: { kind: "track", trackId: "subtitle-track-original" },
  };
}

function textSegment(id: string, startTime: number, endTime: number): TextSegment {
  return {
    id,
    startTime,
    endTime,
    text: `Very long text label ${id}`,
    style: {
      fontVariations: {},
      color: "#ffffff",
      fontSize: 24,
      x: 50,
      y: 50,
      textAlign: "center",
      opacity: 1,
    },
  };
}

function audioSegments(count: number): ImportedAudioSegment[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `a${index}`,
    rawAudioPath: `C:/audio/${index}.wav`,
    name: `Audio ${index}`,
    duration: 1,
    startTime: index * 0.1,
    inPoint: 0,
    outPoint: 0.04,
    addedAt: index,
  }));
}

function narrationSegments(count: number): NarrationSegment[] {
  return audioSegments(count).map((entry) => ({
    ...entry,
    id: entry.id.replace("a", "n"),
    sourceSubtitleId: entry.id.replace("a", "s"),
  }));
}

describe("dense timeline tracks", () => {
  it("keeps subtitle DOM bounded while retaining the active offscreen segment", () => {
    render(
      <SubtitleTrack
        segment={segment(1_000)}
        duration={100}
        editingSubtitleId="s900"
        onSubtitleClick={() => {}}
        onHandleDragStart={() => {}}
        canvasWidthPx={1000}
        visibleTimeRange={{ startTime: 10, endTime: 20 }}
      />,
    );

    expect(document.querySelectorAll(".subtitle-segment").length).toBeLessThanOrEqual(2);
    expect(screen.getByText("Subtitle 900")).toBeInTheDocument();
  });

  it("keeps dense audio and narration DOM bounded", () => {
    const noop = vi.fn();
    render(
      <>
        <ImportedAudioTrack
          segments={audioSegments(1_000)}
          duration={100}
          selectedIds={new Set(["a900"])}
          onSegmentClick={noop}
          canvasWidthPx={1000}
          visibleTimeRange={{ startTime: 10, endTime: 20 }}
        />
        <NarrationTrack
          segments={narrationSegments(1_000)}
          duration={100}
          selectedIds={new Set(["n900"])}
          onSegmentClick={noop}
          canvasWidthPx={1000}
          visibleTimeRange={{ startTime: 10, endTime: 20 }}
        />
      </>,
    );

    expect(document.querySelectorAll(".audio-track-segment").length).toBeLessThanOrEqual(1);
    expect(document.querySelectorAll(".narration-track-segment").length).toBeLessThanOrEqual(1);
    expect(screen.getAllByText("Audio 900")).toHaveLength(2);
  });

  it("clips segment labels and brings clicked text segments to the front", () => {
    const testSegment: VideoSegment = {
      trimStart: 0,
      trimEnd: 10,
      zoomKeyframes: [],
      textSegments: [
        textSegment("t1", 1, 5),
        textSegment("t2", 3, 7),
      ],
      subtitleSegments: [],
    };
    render(
      <TextTrack
        segment={testSegment}
        duration={10}
        editingTextId={null}
        onTextClick={() => {}}
        onHandleDragStart={() => {}}
      />,
    );

    const first = screen.getAllByText("Very long text label t1")[0].closest(".text-segment") as HTMLElement;
    const second = screen.getByText("Very long text label t2").closest(".text-segment") as HTMLElement;
    expect(first).toHaveClass("overflow-hidden");
    expect(screen.getAllByText("Very long text label t1")[0]).toHaveClass("min-w-0", "max-w-full", "truncate");
    expect(second.style.background).toContain("transparent");
    const textContent = first.querySelector(".text-segment-content") as HTMLElement;
    expect(textContent.style.maskImage).toContain("transparent 50%");

    fireEvent.pointerDown(first);
    expect(first.style.zIndex).toBe("5");
    expect(second.style.zIndex).toBe("3");

    fireEvent.pointerDown(second);
    expect(second.style.zIndex).toBe("5");
    expect(first.querySelector(".text-segment-content")).not.toHaveClass("opacity-0");
  });

  it("clips subtitle segment labels and brings the clicked subtitle to the front", () => {
    const testSegment = segment(0);
    testSegment.subtitleSegments = [
      { ...subtitles(1)[0], id: "s1", startTime: 1, endTime: 5, text: "Long subtitle one" },
      { ...subtitles(1)[0], id: "s2", startTime: 3, endTime: 7, text: "Long subtitle two" },
    ];
    testSegment.subtitleTracks = [{
      id: "subtitle-track-original",
      kind: "original",
      segments: testSegment.subtitleSegments,
    }];
    render(
      <SubtitleTrack
        segment={testSegment}
        duration={10}
        editingSubtitleId={null}
        onSubtitleClick={() => {}}
        onHandleDragStart={() => {}}
      />,
    );

    const first = screen.getAllByText("Long subtitle one")[0].closest(".subtitle-segment") as HTMLElement;
    const second = screen.getByText("Long subtitle two").closest(".subtitle-segment") as HTMLElement;
    expect(first).toHaveClass("overflow-hidden");
    expect(screen.getAllByText("Long subtitle one")[0]).toHaveClass("min-w-0", "max-w-full", "truncate");
    expect(second.style.background).toContain("transparent");
    const subtitleContent = first.querySelector(".subtitle-segment-content") as HTMLElement;
    expect(subtitleContent.style.maskImage).toContain("transparent 50%");

    fireEvent.pointerDown(second);
    expect(second.style.zIndex).toBe("5");
    expect(first.style.zIndex).toBe("3");
    expect(first.querySelector(".subtitle-segment-content")).not.toHaveClass("opacity-0");
  });
});
