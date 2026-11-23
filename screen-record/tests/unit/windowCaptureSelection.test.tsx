import { act, renderHook } from "@testing-library/react";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useRecordingControls, type UseRecordingControlsParams } from "@/hooks/useRecordingControls";

type HostWindow = Window & { invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };

afterEach(() => {
  delete (window as HostWindow).invoke;
  localStorage.clear();
});

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return /\.(ts|tsx)$/.test(name) ? [full] : [];
  });
}

describe("window capture selection", () => {
  it("starts a window recording from the native selector without any frontend window polling", async () => {
    localStorage.setItem("screen-record-capture-source-v1", "window");
    const invoke = vi.fn(async (_cmd: string) => null);
    (window as HostWindow).invoke = invoke;
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const startNewRecording = vi.fn(async () => undefined);
    const params: UseRecordingControlsParams = {
      monitors: [], getMonitors: vi.fn(async () => []), isRecording: false, startNewRecording,
      setError: vi.fn(), currentProjectId: null, currentVideo: null, segment: null,
      persistCurrentProjectNow: vi.fn(async () => undefined), setCurrentRecordingMode: vi.fn(),
      setCurrentRawVideoPath: vi.fn(), setCurrentRawMicAudioPath: vi.fn(), setCurrentRawWebcamVideoPath: vi.fn(),
      setLastRawSavedPath: vi.fn(), setRawButtonSavedFlash: vi.fn(),
    };
    const { result } = renderHook(() => useRecordingControls(params));
    act(() => result.current.handleSelectWindowCapture(null));
    await act(async () => { await result.current.handleStartRecording(); });
    expect(invoke).toHaveBeenCalledWith("show_window_selector", undefined);

    await act(async () => {
      window.dispatchEvent(new CustomEvent("external-window-selected", { detail: { windowId: "hwnd-42" } }));
      await Promise.resolve();
    });
    expect(startNewRecording).toHaveBeenCalledWith("hwnd-42", expect.anything(), "window", undefined, expect.anything());
    expect(invoke.mock.calls.map(([cmd]) => cmd)).not.toContain("get_windows");
    expect(setIntervalSpy).not.toHaveBeenCalled();
  });

  it("has no frontend caller of the retired in-app window list", () => {
    const srcRoot = path.resolve(import.meta.dirname, "../../src");
    const offenders = sourceFiles(srcRoot).filter((file) => {
      const text = readFileSync(file, "utf8");
      return /["']get_windows["']|WindowSelectDialog|showWindowSelect/.test(text);
    });
    expect(offenders).toEqual([]);
  });
});
