import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { VideoController } from "@/lib/videoController";
import type { Project } from "@/types/video";
import { loadProjectVideo } from "@/hooks/useProjects/loadProjectMedia";
import { useMediaPipelineRecovery } from "@/hooks/useMediaPipelineRecovery";
import { getMediaServerUrl } from "@/lib/mediaServer";

vi.mock("@/lib/mediaServer", () => ({ getMediaServerUrl: vi.fn() }));

describe("media load ownership", () => {
  it("does not start a project load after its URL resolves for a superseded request", async () => {
    let resolveUrl!: (url: string) => void;
    vi.mocked(getMediaServerUrl).mockReturnValueOnce(new Promise((resolve) => { resolveUrl = resolve; }));
    const loadVideo = vi.fn();
    let current = true;
    const pending = loadProjectVideo({
      controller: { loadVideo } as unknown as VideoController,
      project: { segment: { trimStart: 0 } } as Project,
      rawVideoPath: "old.mp4", isTimelineOnlyProject: false,
      isLatestRequest: () => current,
    });
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    current = false;
    resolveUrl("blob:old");
    await rejected;
    expect(loadVideo).not.toHaveBeenCalled();
  });

  it("does not let delayed recovery replace a newer video load", async () => {
    let resolveUrl!: (url: string) => void;
    vi.mocked(getMediaServerUrl).mockReturnValueOnce(new Promise((resolve) => { resolveUrl = resolve; }));
    const controller = { videoLoadVersion: 1, loadVideo: vi.fn() };
    const setVideo = vi.fn();
    const { unmount } = renderHook(() => useMediaPipelineRecovery({
      currentProjectDataRef: { current: { id: "p" } as Project },
      currentRawVideoPath: "p.mp4", currentRawMicAudioPath: "", currentRawWebcamVideoPath: "",
      currentTime: 0, isPlaying: false, projectId: "p", seek: vi.fn(),
      segmentDeviceAudioAvailable: false, setCurrentAudio: vi.fn(), setCurrentMicAudio: vi.fn(),
      setCurrentVideo: setVideo, setCurrentWebcamVideo: vi.fn(), setPreviewAudioResetKey: vi.fn(),
      videoControllerRef: { current: controller as unknown as VideoController },
    }));
    act(() => window.dispatchEvent(new CustomEvent("sr-reset-media-pipeline")));
    controller.videoLoadVersion = 2;
    await act(async () => { resolveUrl("blob:old"); await Promise.resolve(); });
    expect(controller.loadVideo).not.toHaveBeenCalled();
    expect(setVideo).not.toHaveBeenCalled();
    unmount();
  });
});
