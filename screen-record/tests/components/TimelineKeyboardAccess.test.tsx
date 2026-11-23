import { useState } from "react";
import { fireEvent, render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SpeedTrack } from "@/components/timeline/SpeedTrack";
import { SubtitleTrack } from "@/components/timeline/SubtitleTrack";
import { TextTrack } from "@/components/timeline/TextTrack";
import type { TimelineKeyboardEdits } from "@/components/timeline/useTimelineKeyboardEdits";
import { useAppShortcuts, type UseAppShortcutsParams } from "@/hooks/useAppShortcuts";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";
import en from "@/i18n/en";
import type { SpeedPoint, SubtitleSegment, TextSegment, VideoSegment } from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

afterEach(() => { document.body.innerHTML = ""; });

const subtitle: SubtitleSegment = { id: "s1", startTime: 1, endTime: 3, text: "Hello", style: defaultSubtitleStyle() };

function videoSegment(overrides: Partial<VideoSegment> = {}): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    zoomKeyframes: [],
    textSegments: [],
    subtitleSegments: [subtitle],
    subtitleTracks: [{ id: "subtitle-track-original", kind: "original", segments: [subtitle] }],
    activeSubtitleView: { kind: "track", trackId: "subtitle-track-original" },
    ...overrides,
  } as unknown as VideoSegment;
}

function keyboardEdits(playhead = 2): TimelineKeyboardEdits {
  return {
    getPlayheadTime: () => playhead, nudge: vi.fn(), nudgeZoomBlock: vi.fn(), nudgeZoomEase: vi.fn(), deleteZoomBlock: vi.fn(),
  };
}

function SpeedHarness({ initial, onChange }: { initial: SpeedPoint[]; onChange: (points: SpeedPoint[]) => void }) {
  const [points, setPoints] = useState(initial);
  return (
    <SpeedTrack
      segment={videoSegment({ speedPoints: points })}
      duration={10}
      onUpdateSpeedPoints={(next) => { setPoints(next); onChange(next); }}
      beginBatch={() => {}}
      commitBatch={() => {}}
    />
  );
}

describe("timeline curve points by keyboard", () => {
  const initial: SpeedPoint[] = [{ time: 0, speed: 1 }, { time: 5, speed: 2 }, { time: 10, speed: 1 }];

  it("tabs to each point as a labelled slider", async () => {
    render(<SpeedHarness initial={initial} onChange={() => {}} />);
    const user = userEvent.setup();
    await user.tab();
    const first = screen.getByRole("slider", { name: `${en.timelineSpeedPoint} 1` });
    expect(document.activeElement).toBe(first);
    expect(first.getAttribute("aria-valuetext")).toBe("1.00x");
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("slider", { name: `${en.timelineSpeedPoint} 2` }));
  });

  it("nudges value and time with arrows, deletes with Delete and adds with Enter", async () => {
    const onChange = vi.fn();
    render(<SpeedHarness initial={initial} onChange={onChange} />);
    const user = userEvent.setup();
    screen.getByRole("slider", { name: `${en.timelineSpeedPoint} 2` }).focus();

    await user.keyboard("{ArrowUp}");
    expect(onChange).toHaveBeenLastCalledWith([initial[0], { time: 5, speed: 2.1 }, initial[2]]);
    await user.keyboard("{Shift>}{ArrowDown}{/Shift}");
    expect(onChange.mock.lastCall![0][1].speed).toBeCloseTo(1.1);
    await user.keyboard("{ArrowRight}");
    expect(onChange.mock.lastCall![0][1].time).toBeCloseTo(5.1);

    await user.keyboard("{Delete}");
    expect(onChange).toHaveBeenLastCalledWith([initial[0], initial[2]]);

    screen.getByRole("slider", { name: `${en.timelineSpeedPoint} 1` }).focus();
    await user.keyboard("{Enter}");
    const added = onChange.mock.lastCall![0] as SpeedPoint[];
    expect(added).toHaveLength(3);
    expect(added[1].time).toBe(5);
  });

  it("keeps the endpoints pinned in time and undeletable", async () => {
    const onChange = vi.fn();
    render(<SpeedHarness initial={initial} onChange={onChange} />);
    const user = userEvent.setup();
    screen.getByRole("slider", { name: `${en.timelineSpeedPoint} 1` }).focus();
    await user.keyboard("{ArrowRight}{Delete}");
    expect(onChange).not.toHaveBeenCalled();
  });
});

function renderSubtitleTrack(extra: Partial<React.ComponentProps<typeof SubtitleTrack>> = {}) {
  const props = {
    onSubtitleClick: vi.fn(),
    onSubtitleSplit: vi.fn(),
    onDeleteSubtitleSegments: vi.fn(),
    keyboardEdits: keyboardEdits(),
    ...extra,
  };
  render(
    <SubtitleTrack
      segment={videoSegment()}
      duration={10}
      editingSubtitleId={null}
      onHandleDragStart={() => {}}
      isDeviceAudioAvailable
      isMicAudioAvailable
      {...props}
    />,
  );
  return props;
}

describe("timeline segments by keyboard", () => {
  it("focuses a subtitle segment and its handles with names and roles", async () => {
    renderSubtitleTrack();
    const user = userEvent.setup();
    await user.tab();
    const body = screen.getByRole("button", { name: /^Subtitle "Hello", 1\.00 s - 3\.00 s$/ });
    expect(document.activeElement).toBe(body);
    const start = screen.getByRole("slider", { name: `Subtitle "Hello" ${en.timelineSegmentStart}` });
    expect(start.getAttribute("aria-valuenow")).toBe("1");
    expect(screen.getByRole("slider", { name: `Subtitle "Hello" ${en.timelineSegmentEnd}` })).toBeTruthy();
    // Handles are hover-only for pointers; they must also show while the segment holds focus.
    expect(start.className).toContain("group-focus-within:opacity-100");
  });

  it("nudges, splits, selects and deletes a subtitle from the keyboard", async () => {
    const props = renderSubtitleTrack();
    const user = userEvent.setup();
    screen.getByRole("button", { name: /^Subtitle "Hello"/ }).focus();

    await user.keyboard("{ArrowRight}{Shift>}{ArrowLeft}{/Shift}");
    expect(props.keyboardEdits.nudge).toHaveBeenNthCalledWith(1, "subtitle", "s1", "body", 0.1);
    expect(props.keyboardEdits.nudge).toHaveBeenNthCalledWith(2, "subtitle", "s1", "body", -1);

    await user.keyboard("s");
    expect(props.onSubtitleSplit).toHaveBeenCalledWith("s1", 2);
    await user.keyboard("{Enter}");
    expect(props.onSubtitleClick).toHaveBeenCalledWith("s1");
    await user.keyboard("{Delete}");
    expect(props.onDeleteSubtitleSegments).toHaveBeenCalledWith(["s1"]);

    screen.getByRole("slider", { name: /Start$/ }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(props.keyboardEdits.nudge).toHaveBeenLastCalledWith("subtitle", "s1", "start", -0.1);
  });

  it("does not split when the playhead is outside the segment", async () => {
    const props = renderSubtitleTrack({ keyboardEdits: keyboardEdits(8) });
    const user = userEvent.setup();
    screen.getByRole("button", { name: /^Subtitle "Hello"/ }).focus();
    await user.keyboard("s");
    expect(props.onSubtitleSplit).not.toHaveBeenCalled();
  });

  it("gives text segments the same keys", async () => {
    const text = { id: "t1", startTime: 2, endTime: 6, text: "Title", style: {} } as unknown as TextSegment;
    const edits = keyboardEdits(4);
    const onTextSplit = vi.fn();
    const onDeleteTextSegments = vi.fn();
    render(
      <TextTrack
        segment={videoSegment({ textSegments: [text] })}
        duration={10}
        editingTextId={null}
        onTextClick={() => {}}
        onTextSplit={onTextSplit}
        onDeleteTextSegments={onDeleteTextSegments}
        onHandleDragStart={() => {}}
        keyboardEdits={edits}
      />,
    );
    const user = userEvent.setup();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Text segment "Title"/ }));
    await user.keyboard("{ArrowLeft}s{Delete}");
    expect(edits.nudge).toHaveBeenCalledWith("text", "t1", "body", -0.1);
    expect(onTextSplit).toHaveBeenCalledWith("t1", 4);
    expect(onDeleteTextSegments).toHaveBeenCalledWith(["t1"]);
    screen.getByRole("slider", { name: /End$/ }).focus();
    await user.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(edits.nudge).toHaveBeenLastCalledWith("text", "t1", "end", 1);
  });

  it("does not take focus on a mouse press, so pointer users keep arrow seeking", () => {
    renderSubtitleTrack();
    const body = screen.getByRole("button", { name: /^Subtitle "Hello"/ });
    const press = fireEvent.mouseDown(body);
    expect(press).toBe(false);
    expect(document.activeElement).not.toBe(body);
  });
});

describe("editor shortcuts around a focused timeline item", () => {
  function shortcutParams(): UseAppShortcutsParams {
    return {
      togglePlayPause: vi.fn(), currentTime: 2, duration: 10, seek: vi.fn(), isCropping: false,
      editingKeyframeId: null, editingTextId: null, editingSubtitleId: "s1",
      editingKeystrokeSegmentId: null, editingPointerId: null, segment: videoSegment(),
      setSegment: vi.fn(), setEditingKeyframeId: vi.fn(), handleDeleteText: vi.fn(),
      handleDeleteSubtitle: vi.fn(), handleDeleteKeystrokeSegment: vi.fn(),
      handleDeletePointerSegment: vi.fn(), canUndo: true, canRedo: true, undo: vi.fn(), redo: vi.fn(),
      setSeekIndicatorKey: vi.fn(), setSeekIndicatorDir: vi.fn(),
    };
  }

  it("leaves arrows and Delete to the focused segment instead of seeking or deleting the editing item", async () => {
    const shortcuts = shortcutParams();
    renderHook(() => useAppShortcuts(shortcuts));
    const props = renderSubtitleTrack();
    const user = userEvent.setup();
    screen.getByRole("button", { name: /^Subtitle "Hello"/ }).focus();

    await user.keyboard("{ArrowRight}{Delete}");
    expect(shortcuts.seek).not.toHaveBeenCalled();
    expect(shortcuts.handleDeleteSubtitle).not.toHaveBeenCalled();
    expect(props.keyboardEdits.nudge).toHaveBeenCalledOnce();
    expect(props.onDeleteSubtitleSegments).toHaveBeenCalledOnce();

    (document.activeElement as HTMLElement).blur();
    await user.keyboard("{ArrowRight}");
    expect(shortcuts.seek).toHaveBeenCalledOnce();
  });
});
