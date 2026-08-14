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
      text: () => fileRead.promise,
    } as File;

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
      );
    });
  });
});
