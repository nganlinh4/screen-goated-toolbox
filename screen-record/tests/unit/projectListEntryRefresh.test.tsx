import "fake-indexeddb/auto";
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useProjects } from "@/hooks/useProjects";
import { projectManager } from "@/lib/projectManager";
import type { Project } from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));

type NewProject = Omit<Project, "id" | "createdAt" | "lastModified">;

function newProject(name: string): NewProject {
  return {
    name,
    duration: 1,
    rawVideoPath: `C:/recordings/${name}.mp4`,
    segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
    backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
    mousePositions: [],
  } as unknown as NewProject;
}

function projectsStoreGetAlls(spy: { mock: { contexts: unknown[] } }): number {
  return spy.mock.contexts.filter((store) => (store as IDBObjectStore).name === "projects").length;
}

function renderProjects() {
  const noop = vi.fn();
  return renderHook(() => useProjects({
    videoControllerRef: { current: undefined },
    setCurrentVideo: noop, setCurrentAudio: noop, setCurrentMicAudio: noop,
    setCurrentWebcamVideo: noop, setSegment: noop, setBackgroundConfig: noop,
    setMousePositions: noop, setThumbnails: noop,
    currentVideo: null, currentAudio: null, currentMicAudio: null,
  }));
}

describe("project list entry refresh after a save", () => {
  afterEach(async () => {
    vi.restoreAllMocks();
    projectManager.setActiveProjectId(null);
    for (const project of await projectManager.getProjects()) {
      await projectManager.deleteProject(project.id);
    }
  });

  it("patches the saved project's card in place without reading every project", async () => {
    // Same-millisecond saves tie on createdAt; the list order must still hold.
    vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    const older = await projectManager.saveProject(newProject("older"));
    const edited = await projectManager.saveProject(newProject("edited"));
    vi.mocked(Date.now).mockRestore();
    const { result } = renderProjects();
    await waitFor(() => expect(result.current.projects).toHaveLength(2));
    const orderBefore = result.current.projects.map((project) => project.id);

    await projectManager.updateProject(edited.id, { name: "Renamed" });
    const getAll = vi.spyOn(IDBObjectStore.prototype, "getAll");
    for (const id of [edited.id, older.id]) {
      await act(async () => {
        await result.current.refreshProjectEntry(id);
      });
      expect(result.current.projects.map((project) => project.id)).toEqual(orderBefore);
    }

    expect(projectsStoreGetAlls(getAll)).toBe(0);
    expect(result.current.projects.find((project) => project.id === edited.id)?.name).toBe("Renamed");
    expect(result.current.projects.find((project) => project.id === older.id)?.name).toBe("older");
  });

  it("falls back to a full reload when the stored set no longer matches the list", async () => {
    const kept = await projectManager.saveProject(newProject("kept"));
    const removed = await projectManager.saveProject(newProject("removed"));
    const { result } = renderProjects();
    await waitFor(() => expect(result.current.projects).toHaveLength(2));

    await projectManager.deleteProject(removed.id);
    await act(async () => {
      await result.current.refreshProjectEntry(kept.id);
    });

    expect(result.current.projects.map((project) => project.id)).toEqual([kept.id]);
  });
});
