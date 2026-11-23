import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAppEffects, type UseAppEffectsParams } from "@/hooks/useAppEffects";

// The real modules paint background swatches at import, which jsdom cannot run.
vi.mock("@/lib/appUtils", () => ({ LAST_BG_CONFIG_KEY: "test-last-bg" }));
vi.mock("@/hooks/useVideoState", () => ({ saveCropPref: vi.fn() }));
import type { BackgroundConfig, VideoSegment } from "@/types/video";

function params(video: { paused: boolean }, persist: ReturnType<typeof vi.fn>, overrides: Partial<UseAppEffectsParams> = {}): UseAppEffectsParams {
  return {
    segment: { trimStart: 0, trimEnd: 5 } as VideoSegment,
    segmentRef: { current: null },
    backgroundConfig: {} as BackgroundConfig,
    currentProjectId: "p1",
    currentVideo: "video.mp4",
    persistRef: { current: persist },
    isRecording: false,
    showHotkeyDialog: false,
    onStopRecording: vi.fn(),
    handleStartRecording: vi.fn(),
    mousePositions: [],
    composition: null,
    currentRecordingMode: "withoutCursor",
    currentRawVideoPath: "raw.mp4",
    duration: 5,
    videoRef: { current: video as HTMLVideoElement },
    isProcessing: false,
    ...overrides,
  };
}

// The canvas-mode effect also persists (metadata only) shortly after mount.
const autosaves = (persist: ReturnType<typeof vi.fn>) =>
  persist.mock.calls.filter(([options]) => options?.includeMedia === true);

describe("editor autosave", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("saves an edit once playback pauses instead of dropping it", () => {
    const video = { paused: false };
    const persist = vi.fn(async () => {});
    renderHook(() => useAppEffects(params(video, persist)));
    vi.advanceTimersByTime(3000);
    expect(autosaves(persist)).toHaveLength(0);

    video.paused = true;
    vi.advanceTimersByTime(10_000);
    expect(autosaves(persist)).toEqual([[{ refreshList: true, includeMedia: true }]]);
  });

  it("waits for an export that started after the edit to finish", () => {
    const video = { paused: true };
    const persist = vi.fn(async () => {});
    // Same editor state throughout: only the export flag changes.
    const base = params(video, persist);
    const { rerender } = renderHook((p: UseAppEffectsParams) => useAppEffects(p), { initialProps: base });
    vi.advanceTimersByTime(1000);
    rerender({ ...base, isProcessing: true });
    vi.advanceTimersByTime(5000);
    expect(autosaves(persist)).toHaveLength(0);
    rerender({ ...base, isProcessing: false });
    vi.advanceTimersByTime(5000);
    expect(autosaves(persist)).toHaveLength(1);
  });
});
