import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "@/lib/ipc";
import { useAppDropActions } from "@/hooks/useAppDropActions";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => []) }));

function takeCalls(command: string) {
  return vi.mocked(invoke).mock.calls.filter(([name]) => name === command).length;
}

function render(initialBusy: boolean, importAudioPaths = vi.fn(async () => undefined)) {
  return renderHook(({ busy }: { busy: boolean }) => useAppDropActions({
    // Fresh closures every render, as App passes them.
    getCurrentProjectId: () => "project-1",
    importAudioPaths: (...args) => importAudioPaths(...args),
    importSubtitlePayload: async () => undefined,
    importVideoPath: async () => null,
    setPendingAutoSubtitleProjectId: () => undefined,
    isAudioImportBusy: busy,
  }), { initialProps: { busy: initialBusy } });
}

describe("useAppDropActions", () => {
  it("drains each host queue once on mount, not on every render", async () => {
    vi.mocked(invoke).mockClear();
    const hook = render(false);
    await act(async () => { await Promise.resolve(); });
    for (let index = 0; index < 5; index += 1) hook.rerender({ busy: false });
    await act(async () => { await Promise.resolve(); });
    expect(takeCalls("take_pending_audio_drop_actions")).toBe(1);
    expect(takeCalls("take_pending_subtitle_drop_actions")).toBe(1);
    expect(takeCalls("take_pending_video_drop_actions")).toBe(1);
  });

  it("leaves a drop queued while an audio import runs and imports it once the importer is idle", async () => {
    vi.mocked(invoke).mockClear();
    const importAudioPaths = vi.fn(async () => undefined);
    const hook = render(true, importAudioPaths);
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === "take_pending_audio_drop_actions" ? [{ path: "C:/music/second.mp3" }] : []);
    await act(async () => { window.dispatchEvent(new Event("sgt-audio-drop-pending")); await Promise.resolve(); });
    expect(takeCalls("take_pending_audio_drop_actions")).toBe(0);
    hook.rerender({ busy: false });
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    expect(importAudioPaths).toHaveBeenCalledWith(["C:/music/second.mp3"], "project-1", undefined);
  });
});
