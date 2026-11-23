import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/mediaServer", () => ({
  createAudioPlaceholderVideo: vi.fn(),
  importAudioPathToManagedMediaFile: vi.fn(),
  importAudioToManagedMediaFile: vi.fn(),
}));
vi.mock("@/lib/projectManager", () => ({
  projectManager: {
    deleteProject: vi.fn(),
    saveProject: vi.fn(),
  },
}));
vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn(),
  logToHost: vi.fn(),
}));

import { useAppDropActions } from "@/hooks/useAppDropActions";
import { useImportedAudioImport } from "@/hooks/useImportedAudioImport";
import { useSubtitleSrtImport } from "@/hooks/useSubtitleSrtImport";
import { captureImportStartOwner } from '@/hooks/importProjectOwnership';
import { activateNarrationProject, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { importAudioPathToManagedMediaFile } from "@/lib/mediaServer";
import { invoke } from "@/lib/ipc";
import type { VideoSegment } from "@/types/video";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

const baseSegment = {
  trimStart: 0,
  trimEnd: 5,
  trimSegments: [{ id: "trim", startTime: 0, endTime: 5 }],
  subtitleSegments: [],
} as unknown as VideoSegment;

describe("project import race guards", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  it('carries the original activation across delayed dropped-path drains', async () => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'project-a');
    const audio = deferred<Array<{ path: string }>>();
    const subtitles = deferred<Array<{ path: string }>>();
    const video = deferred<Array<{ path: string; action: string }>>();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'take_pending_audio_drop_actions') return audio.promise;
      if (command === 'take_pending_subtitle_drop_actions') return subtitles.promise;
      if (command === 'take_pending_video_drop_actions') return video.promise;
      if (command === 'read_subtitle_file_path') return Promise.resolve({ fileName: 'a.srt', content: 'text' });
      return Promise.resolve([]);
    });
    const audioImport = vi.fn(async (..._args: unknown[]) => undefined);
    const subtitleImport = vi.fn(async (..._args: unknown[]) => undefined);
    const videoImport = vi.fn(async () => null);
    const hook = renderHook(() => useAppDropActions({ getCurrentProjectId: () => owner.current,
      captureStartOwner: () => captureImportStartOwner(owner), importAudioPaths: audioImport,
      importSubtitlePayload: subtitleImport, importVideoPath: videoImport, setPendingAutoSubtitleProjectId: vi.fn() }));
    activateNarrationProject(owner, 'project-a');
    await act(async () => {
      audio.resolve([{ path: 'a.wav' }]); subtitles.resolve([{ path: 'a.srt' }]);
      video.resolve([{ path: 'a.mp4', action: 'generate-subtitles' }]);
    });
    await waitFor(() => expect(audioImport).toHaveBeenCalledOnce());
    await waitFor(() => expect(subtitleImport).toHaveBeenCalledOnce());
    expect((audioImport.mock.calls[0][2] as () => boolean)()).toBe(false);
    expect((subtitleImport.mock.calls[0][2] as () => boolean)()).toBe(false);
    expect(videoImport).not.toHaveBeenCalled();
    hook.unmount(); setNarrationProjectId(owner, null);
  });

  it("deletes imported audio instead of attaching it to a newly opened project", async () => {
    let activeProjectId: string | null = "project-a";
    const pendingImport = deferred<{ path: string; duration: number }>();
    vi.mocked(importAudioPathToManagedMediaFile).mockReturnValue(
      pendingImport.promise,
    );
    const onAttachToCurrentProject = vi.fn();
    const onError = vi.fn();
    const { result } = renderHook(() => useImportedAudioImport({
      getCurrentProjectId: () => activeProjectId,
      onAttachToCurrentProject,
      onCreateAudioProject: vi.fn(),
      onError,
    }));

    let operation!: Promise<void>;
    act(() => {
      operation = result.current.importAudioPath("C:\\media\\track.wav", "project-a");
    });
    activeProjectId = "project-b";
    pendingImport.resolve({ path: "C:\\managed\\track.wav", duration: 4 });
    await act(async () => operation);

    expect(onAttachToCurrentProject).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("delete_file", {
      path: "C:\\managed\\track.wav",
    });
    expect(onError).toHaveBeenCalledWith(expect.stringContaining("project changed"));
  });

  it("does not apply a subtitle file whose read completed after a project switch", async () => {
    let activeProjectId: string | null = "project-a";
    const fileRead = deferred<string>();
    const setSegment = vi.fn();
    const onError = vi.fn();
    const { result } = renderHook(() => useSubtitleSrtImport({
      segment: baseSegment,
      getCurrentSegment: () => baseSegment,
      duration: 5,
      getCurrentProjectId: () => activeProjectId,
      setSegment,
      setActivePanel: vi.fn(),
      setEditingSubtitleId: vi.fn(),
      onCreateSubtitleProject: vi.fn(),
      onError,
    }));
    const file = {
      name: "captions.srt",
      type: "application/x-subrip",
      arrayBuffer: () => fileRead.promise.then((text) => new TextEncoder().encode(text).buffer),
    } as unknown as File;

    let operation!: Promise<void>;
    act(() => { operation = result.current.importSubtitleFile(file); });
    activeProjectId = "project-b";
    fileRead.resolve("1\n00:00:00,000 --> 00:00:01,000\nHello\n");
    await act(async () => operation);

    expect(setSegment).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith(expect.stringContaining("project changed"));
  });

  it("preserves the project identity captured before dropped paths are drained", async () => {
    let activeProjectId: string | null = "project-a";
    const audioActions = deferred<Array<{ path: string }>>();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "take_pending_audio_drop_actions") {
        return audioActions.promise;
      }
      return [];
    });
    const importAudioPaths = vi.fn(async () => undefined);
    renderHook(() => useAppDropActions({
      getCurrentProjectId: () => activeProjectId,
      importAudioPaths,
      importSubtitlePayload: vi.fn(async () => undefined),
      importVideoPath: vi.fn(async () => null),
      setPendingAutoSubtitleProjectId: vi.fn(),
    }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("take_pending_audio_drop_actions", {});
    });

    activeProjectId = "project-b";
    audioActions.resolve([{ path: "C:\\media\\track.wav" }]);
    await waitFor(() => {
      expect(importAudioPaths).toHaveBeenCalledWith(
        ["C:\\media\\track.wav"],
        "project-a",
        undefined,
      );
    });
  });
});
