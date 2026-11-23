import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Header } from "@/components/Header";
import { useDownloadableBg } from "@/components/sidepanel/useDownloadableBg";
import en from "@/i18n/en";
import { DEFAULT_RECORDING_AUDIO_SELECTION } from "@/types/recordingAudio";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

type HostWindow = Window & {
  ipc?: { postMessage: (message: string) => void };
  invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
};

const host = window as HostWindow;
let postMessage: ReturnType<typeof vi.fn>;
let maximized = false;
let invokeImpl: (cmd: string) => Promise<unknown>;

beforeEach(() => {
  postMessage = vi.fn();
  maximized = false;
  invokeImpl = async (cmd) => (cmd === "is_maximized" ? maximized : null);
  host.ipc = { postMessage };
  host.invoke = vi.fn((cmd: string) => invokeImpl(cmd));
});

afterEach(() => {
  delete host.ipc;
  delete host.invoke;
  vi.useRealTimers();
});

function renderHeader() {
  const noop = vi.fn();
  return render(
    <Header
      isRecording={false}
      recordingDuration={0}
      currentVideo={null}
      isProcessing={false}
      hotkeys={[]}
      onRemoveHotkey={noop}
      onOpenHotkeyDialog={noop}
      onExport={noop}
      recordingMode="withCursor"
      onRecordingModeChange={noop}
      recordingAudioSelection={DEFAULT_RECORDING_AUDIO_SELECTION}
      isSelectingRecordingAudioApp={false}
      onToggleRecordingDeviceAudio={noop}
      onToggleRecordingMicAudio={noop}
      onSelectAllRecordingDeviceAudio={noop}
      onRequestRecordingAudioAppSelection={noop}
      rawButtonLabel="Raw"
      rawButtonPulse={false}
      rawButtonDisabled={false}
      onOpenRawVideoDialog={noop}
      onOpenProjects={noop}
      captureSource="monitor"
      captureFps={null}
      monitors={[]}
      onSelectMonitorCapture={noop}
      onSelectWindowCapture={noop}
    />,
  );
}

describe("Header window chrome", () => {
  it("starts a native drag only for the primary mouse button", () => {
    const { container } = renderHeader();
    const header = container.querySelector(".app-header")!;
    fireEvent.mouseDown(header, { button: 2, clientX: 300, clientY: 20 });
    fireEvent.mouseDown(header, { button: 1, clientX: 300, clientY: 20 });
    expect(postMessage).not.toHaveBeenCalled();
    fireEvent.mouseDown(header, { button: 0, clientX: 300, clientY: 20 });
    expect(postMessage).toHaveBeenCalledWith("drag_window");
  });

  it("follows maximize changes made outside the header buttons", async () => {
    renderHeader();
    await waitFor(() => expect(screen.getByTitle(en.maximize)).toBeInTheDocument());
    maximized = true;
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    await waitFor(() => expect(screen.getByTitle(en.restore)).toBeInTheDocument());
  });

  it("swallows a failing maximize query after the toggle instead of rejecting", async () => {
    const rejections: unknown[] = [];
    const onRejection = (event: PromiseRejectionEvent) => rejections.push(event.reason);
    window.addEventListener("unhandledrejection", onRejection);
    renderHeader();
    await waitFor(() => expect(screen.getByTitle(en.maximize)).toBeInTheDocument());
    invokeImpl = async () => {
      throw new Error("host gone");
    };
    fireEvent.click(screen.getByTitle(en.maximize));
    expect(postMessage).toHaveBeenCalledWith("toggle_maximize");
    await new Promise((resolve) => setTimeout(resolve, 120));
    window.removeEventListener("unhandledrejection", onRejection);
    expect(rejections).toEqual([]);
    expect(screen.getByTitle(en.maximize)).toBeInTheDocument();
  });
});

describe("downloadable background start", () => {
  it("turns a failed start_bg_download into the error state", async () => {
    invokeImpl = async (cmd) => {
      if (cmd === "start_bg_download") throw new Error("host refused url");
      return null;
    };
    const { result } = renderHook(() =>
      useDownloadableBg({ id: "bg1", preview: "p.jpg", downloadUrl: "https://example.invalid/a.jpg" }, vi.fn()),
    );
    await act(async () => {
      await result.current.startDownload();
    });
    expect(result.current.state).toEqual({ status: "error", message: "host refused url" });
  });
});
