import "fake-indexeddb/auto";
import { Blob as NodeBlob } from "node:buffer";
import { describe, expect, it, vi } from "vitest";
import { projectManager } from "@/lib/projectManager";
import { invoke } from "@/lib/ipc";
import { idbCreateProjectBundle, idbGet, PROJECTS_STORE } from "@/lib/projectStorage";
import type { Project } from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));

describe("project retention safety", () => {
  it.each(['audioSegments', 'narrationSegments'] as const)('retains an inactive project whose only media is in %s', async (track) => {
    projectManager.setActiveProjectId(null);
    for (let index = 0; index < 11; index++) {
      await idbCreateProjectBundle({
        id: `owned-${track}-${index}`, name: `Project ${index}`, createdAt: index, lastModified: index,
        duration: 1, segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
        backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: 'solid' }, mousePositions: [],
        composition: { mode: 'separate', clips: [], [track]: [{ id: `take-${index}`,
          rawAudioPath: `owned-${index}.wav`, name: 'Audio', duration: 1, startTime: 0,
          inPoint: 0, outPoint: 1, addedAt: index }] },
      } as Project);
    }
    await projectManager.setLimit(10);
    expect(await idbGet(PROJECTS_STORE, `owned-${track}-0`)).not.toBeNull();
    expect((await projectManager.getProjects()).some((project) => project.id === `owned-${track}-0`)).toBe(true);
  });
  it("does not change retention or prune projects when the host has not confirmed the limit", async () => {
    const calls = vi.mocked(invoke);
    const before = projectManager.getLimit();
    calls.mockRejectedValueOnce("SGT_SETTINGS_OUTCOME_UNKNOWN");
    await expect(projectManager.setLimit(10)).rejects.toBe("SGT_SETTINGS_OUTCOME_UNKNOWN");
    expect(projectManager.getLimit()).toBe(before);
  });

  it("preserves raw recordings and retains projects whose only media is stored in IndexedDB", async () => {
    const calls = vi.mocked(invoke);
    calls.mockClear();
    projectManager.setActiveProjectId(null);
    for (let index = 0; index < 11; index++) {
      await idbCreateProjectBundle({
        id: `retention-${index}`,
        name: `Project ${index}`,
        createdAt: index,
        lastModified: index,
        duration: 1,
        segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
        backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
        mousePositions: [],
        rawVideoPath: index <= 1 ? undefined : `C:\\recordings\\${index}.mp4`,
        videoBlob: index === 0 ? new NodeBlob(["recording"]) as unknown as Blob : undefined,
      } as Project);
    }
    await projectManager.setLimit(10);
    expect(await idbGet(PROJECTS_STORE, "retention-0")).not.toBeNull();
    expect(await idbGet("videos", "retention-0")).not.toBeNull();
    expect(await idbGet(PROJECTS_STORE, "retention-1")).toBeNull();
    expect(calls.mock.calls.some(([command]) => command === "delete_file")).toBe(false);
  });

  it("keeps a newly saved recording discoverable when every older project is protected", async () => {
    projectManager.setActiveProjectId(null);
    for (let index = 0; index < 10; index++) {
      await idbCreateProjectBundle({
        id: `protected-${index}`, name: `Protected ${index}`, createdAt: index, lastModified: index,
        duration: 1, segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
        backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
        mousePositions: [], rawVideoPath: `C:\\recordings\\protected-${index}.mp4`,
      } as Project);
    }
    const recordingId = "d".repeat(32);
    const [saved] = await Promise.all([
      projectManager.saveProject({
        name: "New recording", duration: 1,
        segment: { trimStart: 0, trimEnd: 1, trimSegments: [] } as Project["segment"],
        backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" } as Project["backgroundConfig"],
        mousePositions: [], rawVideoPath: "C:\\recordings\\new.mp4",
      }, recordingId),
      projectManager.setLimit(10),
    ]);
    expect(await idbGet(PROJECTS_STORE, saved.id)).toMatchObject({ rawVideoPath: "C:\\recordings\\new.mp4" });
    expect((await projectManager.getProjects()).some((project) => project.id === saved.id)).toBe(true);
  });
});
