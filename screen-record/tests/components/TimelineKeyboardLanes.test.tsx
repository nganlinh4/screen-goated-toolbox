import { useState } from "react";
import { render, renderHook, act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { PointerTrack } from "@/components/timeline/PointerTrack";
import { WebcamVisibilityTrack } from "@/components/timeline/WebcamVisibilityTrack";
import { KeystrokeTrack } from "@/components/timeline/KeystrokeTrack";
import { TrimTrack } from "@/components/timeline/TrimTrack";
import { ZoomBlockLayer } from "@/components/timeline/ZoomBlockLayer";
import { NarrationTrack } from "@/components/timeline/NarrationTrack";
import { SoftAudioSegmentBlock, type SoftAudioPoint } from "@/components/timeline/SoftAudioSegmentBlock";
import {
  useTimelineKeyboardEdits,
  type TimelineKeyboardEdits,
} from "@/components/timeline/useTimelineKeyboardEdits";
import en from "@/i18n/en";
import type { NarrationSegment, VideoSegment, ZoomBlock } from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

function edits(playhead = 2): TimelineKeyboardEdits {
  return {
    getPlayheadTime: () => playhead,
    nudge: vi.fn(),
    nudgeZoomBlock: vi.fn(),
    nudgeZoomEase: vi.fn(),
    deleteZoomBlock: vi.fn(),
  };
}

const range = { id: "r1", startTime: 1, endTime: 3 };

function videoSegment(overrides: Partial<VideoSegment> = {}): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    trimSegments: [{ id: "trim1", startTime: 0, endTime: 10 }],
    zoomKeyframes: [],
    textSegments: [],
    cursorVisibilitySegments: [range],
    webcamVisibilitySegments: [range],
    keystrokeMode: "keyboard",
    keyboardVisibilitySegments: [range],
    ...overrides,
  } as unknown as VideoSegment;
}

describe("visibility lanes by keyboard", () => {
  it("nudges, splits and deletes a pointer segment and its edges", async () => {
    const keyEdits = edits(2);
    const onPointerClick = vi.fn();
    const onDeletePointerSegments = vi.fn();
    const onAddPointerSegment = vi.fn();
    render(
      <PointerTrack
        segment={videoSegment()}
        duration={10}
        onPointerClick={onPointerClick}
        onHandleDragStart={() => {}}
        onDeletePointerSegments={onDeletePointerSegments}
        onAddPointerSegment={onAddPointerSegment}
        keyboardEdits={keyEdits}
      />,
    );
    const user = userEvent.setup();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Pointer 1, 1\.00 s - 3\.00 s$/ }));
    await user.keyboard("{ArrowRight}sa{Delete}");
    expect(keyEdits.nudge).toHaveBeenCalledWith("pointer", "r1", "body", 0.1);
    expect(onAddPointerSegment).toHaveBeenCalledWith(2);
    expect(onPointerClick).toHaveBeenCalledWith("r1", 2);
    expect(onDeletePointerSegments).toHaveBeenCalledWith(["r1"]);

    const start = screen.getByRole("slider", { name: "Pointer 1 Start" });
    expect(start.className).toContain("group-focus-within:opacity-100");
    start.focus();
    await user.keyboard("{Shift>}{ArrowLeft}{/Shift}");
    expect(keyEdits.nudge).toHaveBeenLastCalledWith("pointer", "r1", "start", -1);
  });

  it("gives webcam and keystroke segments the same keys for their own lane", async () => {
    const keyEdits = edits();
    render(
      <>
        <WebcamVisibilityTrack
          segment={videoSegment()} duration={10} isAvailable
          onWebcamClick={() => {}} onHandleDragStart={() => {}} keyboardEdits={keyEdits}
        />
        <KeystrokeTrack
          segment={videoSegment()} duration={10} editingKeystrokeSegmentId={null}
          onKeystrokeClick={() => {}} onHandleDragStart={() => {}} keyboardEdits={keyEdits}
        />
      </>,
    );
    const user = userEvent.setup();
    screen.getByRole("button", { name: /^Webcam 1,/ }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(keyEdits.nudge).toHaveBeenLastCalledWith("webcam", "r1", "body", -0.1);
    screen.getByRole("slider", { name: `${en.trackKeystrokes} 1 End` }).focus();
    await user.keyboard("{ArrowRight}");
    expect(keyEdits.nudge).toHaveBeenLastCalledWith("keystroke", "r1", "end", 0.1);
  });

  it("moves trim edges and adds a kept range from the keyboard", async () => {
    const keyEdits = edits(4);
    const onTrimAddSegment = vi.fn();
    render(
      <TrimTrack
        segment={videoSegment()} duration={10} thumbnails={[]}
        onTrimDragStart={() => {}} onTrimSplit={() => {}} onTrimAddSegment={onTrimAddSegment}
        keyboardEdits={keyEdits}
      />,
    );
    const user = userEvent.setup();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("slider", { name: "Video 1 Start" }));
    await user.keyboard("{ArrowRight}");
    expect(keyEdits.nudge).toHaveBeenLastCalledWith("trim", "trim1", "start", 0.1);
    await user.tab();
    await user.keyboard("{ArrowLeft}");
    expect(keyEdits.nudge).toHaveBeenLastCalledWith("trim", "trim1", "end", -0.1);
    await user.keyboard("a");
    expect(onTrimAddSegment).toHaveBeenCalledWith(4);
  });
});

const block = (id: string, startTime: number, endTime: number): ZoomBlock =>
  ({ id, startTime, endTime, easeIn: 0.5, easeOut: 0.5, zoomFactor: 2 }) as ZoomBlock;

describe("zoom blocks by keyboard", () => {
  function renderLayer(hoveredBlockIdx: number | null, keyEdits = edits()) {
    const onKeyframeClick = vi.fn();
    const onHoverBlock = vi.fn();
    render(
      <ZoomBlockLayer
        blocks={[block("z1", 2, 5)]} duration={10} editingKeyframeId={null}
        hoveredBlockIdx={hoveredBlockIdx} globalDragVisualMode={null} trackWidth={500}
        trackRef={{ current: null }} onKeyframeClick={onKeyframeClick} onKeyframeDragStart={() => {}}
        onHoverBlock={onHoverBlock} startResizeBlock={() => {}} startResizeTransition={() => {}}
        onToggleDirectTransition={() => {}} keyboardEdits={keyEdits}
      />,
    );
    return { keyEdits, onKeyframeClick, onHoverBlock };
  }

  it("moves, resizes, selects and deletes a block, and shows its ease handles on focus", async () => {
    const { keyEdits, onKeyframeClick, onHoverBlock } = renderLayer(null);
    const user = userEvent.setup();
    await user.tab();
    const body = screen.getByRole("button", { name: /^Zoom 1, 100%, 2\.00 s - 5\.00 s$/ });
    expect(document.activeElement).toBe(body);
    expect(onHoverBlock).toHaveBeenLastCalledWith(0);
    await user.keyboard("{ArrowRight}{Enter}{Delete}");
    expect(keyEdits.nudgeZoomBlock).toHaveBeenCalledWith(0, "body", 0.1);
    expect(onKeyframeClick).toHaveBeenCalledWith(3.5, 0);
    expect(keyEdits.deleteZoomBlock).toHaveBeenCalledWith(0);
    screen.getByRole("slider", { name: /Zoom 1, 100% End$/ }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(keyEdits.nudgeZoomBlock).toHaveBeenLastCalledWith(0, "end", -0.1);
  });

  it("nudges the ease handles once they are shown", async () => {
    const { keyEdits } = renderLayer(0);
    const user = userEvent.setup();
    screen.getByRole("slider", { name: /Ease in$/ }).focus();
    await user.keyboard("{ArrowRight}");
    expect(keyEdits.nudgeZoomEase).toHaveBeenLastCalledWith(0, "in", 0.1);
    screen.getByRole("slider", { name: /Ease out$/ }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(keyEdits.nudgeZoomEase).toHaveBeenLastCalledWith(0, "out", -0.1);
  });
});

describe("audio lanes by keyboard", () => {
  const narration = {
    id: "n1", rawAudioPath: "n1.wav", name: "Intro", duration: 4, startTime: 1,
    inPoint: 0, outPoint: 2, addedAt: 0,
  } as NarrationSegment;

  it("moves and trims a narration clip and deletes it", async () => {
    const onUpdateSegment = vi.fn();
    const onDeleteSegments = vi.fn();
    const batch = { beginBatch: vi.fn(), commitBatch: vi.fn() };
    render(
      <NarrationTrack
        segments={[narration]} duration={10} onUpdateSegment={onUpdateSegment}
        onDeleteSegments={onDeleteSegments} {...batch}
      />,
    );
    const user = userEvent.setup();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Narration "Intro", 1\.00 s - 3\.00 s$/ }));
    await user.keyboard("{ArrowRight}");
    expect(onUpdateSegment).toHaveBeenLastCalledWith("n1", { startTime: 1.1 });
    expect(batch.beginBatch).toHaveBeenCalledOnce();
    expect(batch.commitBatch).toHaveBeenCalledOnce();
    screen.getByRole("slider", { name: 'Narration "Intro" End' }).focus();
    await user.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(onUpdateSegment).toHaveBeenLastCalledWith("n1", { outPoint: 3 });
    screen.getByRole("button", { name: /^Narration "Intro"/ }).focus();
    await user.keyboard("{Delete}");
    expect(onDeleteSegments).toHaveBeenCalledWith(["n1"]);
  });

  it("moves, splits and adds device or mic audio ranges", async () => {
    const onUpdatePoints = vi.fn();
    const points: SoftAudioPoint[] = [
      { time: 0, volume: 0 }, { time: 2, volume: 0 }, { time: 2.01, volume: 1 },
      { time: 4, volume: 1 }, { time: 4.01, volume: 0 }, { time: 10, volume: 0 },
    ];
    render(
      <SoftAudioSegmentBlock
        classNamePrefix="mic-audio" laneName={en.trackMicAudio} duration={10} points={points}
        isAvailable tone="info" colorVariable="--x" onUpdatePoints={onUpdatePoints}
        beginBatch={() => {}} commitBatch={() => {}} getPlayheadTime={() => 3}
      />,
    );
    const user = userEvent.setup();
    await user.tab();
    expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toMatch(/^Mic 1, /);
    await user.keyboard("{ArrowRight}");
    expect(onUpdatePoints).toHaveBeenCalledOnce();
    await user.keyboard("s");
    expect(onUpdatePoints).toHaveBeenCalledTimes(2);
    await user.keyboard("a");
    expect(onUpdatePoints).toHaveBeenCalledTimes(3);
  });
});

describe("useTimelineKeyboardEdits", () => {
  function setup(initial: VideoSegment, editingKeyframeId: number | null = null) {
    const setEditingKeyframeId = vi.fn();
    const { result } = renderHook(() => {
      const [segment, setSegment] = useState<VideoSegment | null>(initial);
      const keyEdits = useTimelineKeyboardEdits({
        currentTime: 0, duration: 10, editingKeyframeId, setSegment, setEditingKeyframeId,
      });
      return { segment, keyEdits };
    });
    return { result, setEditingKeyframeId };
  }

  it("writes visibility and trim nudges back to the segment", () => {
    const { result } = setup(videoSegment());
    act(() => result.current.keyEdits.nudge("pointer", "r1", "body", 1));
    expect(result.current.segment?.cursorVisibilitySegments?.[0]).toMatchObject({ startTime: 2, endTime: 4 });
    act(() => result.current.keyEdits.nudge("trim", "trim1", "start", 0.5));
    expect(result.current.segment?.trimStart).toBe(0.5);
    act(() => result.current.keyEdits.nudge("pointer", "missing", "body", 1));
    expect(result.current.segment?.cursorVisibilitySegments?.[0]).toMatchObject({ startTime: 2, endTime: 4 });
  });

  it("keeps a moved zoom block clear of its neighbours and fixes the editing index after a delete", () => {
    const { result, setEditingKeyframeId } = setup(
      videoSegment({ zoomBlocks: [block("a", 0, 2), block("b", 2.5, 4), block("c", 6, 8)] }),
      2,
    );
    act(() => result.current.keyEdits.nudgeZoomBlock(1, "body", -1));
    expect(result.current.segment?.zoomBlocks?.[1]).toMatchObject({ startTime: 2.2, endTime: 3.7 });
    act(() => result.current.keyEdits.nudgeZoomEase(1, "in", 0.25));
    expect(result.current.segment?.zoomBlocks?.[1].easeIn).toBe(0.75);
    act(() => result.current.keyEdits.deleteZoomBlock(0));
    expect(result.current.segment?.zoomBlocks?.map((b) => b.id)).toEqual(["b", "c"]);
    expect(setEditingKeyframeId).toHaveBeenLastCalledWith(1);
  });
});
