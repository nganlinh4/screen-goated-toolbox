import "fake-indexeddb/auto";
import { Blob as NodeBlob } from "node:buffer";
import { afterEach, describe, expect, it, vi } from "vitest";
import { projectManager } from "@/lib/projectManager";
import { omitUnchangedProjectMedia, rememberStoredProjectMedia } from "@/lib/projectStoredMedia";
import type { Project } from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));

function storeCalls(spy: { mock: { contexts: unknown[] } }, storeName: string): number {
  return spy.mock.contexts.filter((store) => (store as IDBObjectStore).name === storeName).length;
}

describe("project media writes", () => {
  afterEach(() => vi.restoreAllMocks());

  it("drops only blob updates identical to the stored blob", () => {
    const stored = new NodeBlob(["video"]) as unknown as Blob;
    const replacement = new NodeBlob(["new"]) as unknown as Blob;
    rememberStoredProjectMedia("p", { videoBlob: stored, micAudioBlob: null });
    expect(omitUnchangedProjectMedia("p", { videoBlob: stored, name: "x" })).toEqual({ name: "x" });
    expect(omitUnchangedProjectMedia("p", { videoBlob: replacement })).toEqual({ videoBlob: replacement });
    expect(omitUnchangedProjectMedia("p", { micAudioBlob: undefined })).toEqual({ micAudioBlob: undefined });
    expect(omitUnchangedProjectMedia("other", { videoBlob: stored })).toEqual({ videoBlob: stored });
  });

  it("autosave with unchanged media rewrites no blob and reads no full project list", async () => {
    projectManager.setActiveProjectId(null);
    const created = await projectManager.saveProject({
      name: "Media", duration: 1,
      segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
      backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
      mousePositions: [],
      videoBlob: new NodeBlob(["recorded video"]) as unknown as Blob,
    } as unknown as Omit<Project, "id" | "createdAt" | "lastModified">);
    projectManager.setActiveProjectId(created.id);
    const loaded = await projectManager.loadProject(created.id);
    expect(loaded?.videoBlob).toBeTruthy();

    const put = vi.spyOn(IDBObjectStore.prototype, "put");
    const getAll = vi.spyOn(IDBObjectStore.prototype, "getAll");
    const applied = await projectManager.updateProject(created.id, {
      name: "Edited",
      videoBlob: loaded?.videoBlob,
      micAudioBlob: undefined,
    });

    expect(applied).toBe(true);
    expect(storeCalls(put, "videos")).toBe(0);
    expect(storeCalls(getAll, "projects")).toBe(0);
    const reloaded = await projectManager.loadProject(created.id);
    expect(reloaded?.name).toBe("Edited");
    expect(reloaded?.videoBlob).toBeTruthy();
    await projectManager.deleteProject(created.id);
  });
});
