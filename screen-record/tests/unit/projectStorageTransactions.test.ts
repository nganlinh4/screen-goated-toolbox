import "fake-indexeddb/auto";
import { Blob as NodeBlob } from "node:buffer";
import { describe, expect, it } from "vitest";
import {
  buildCompositionAssetKey,
  idbCreateProjectBundle,
  idbDeleteProjectBundle,
  idbGet,
  idbUpdateProjectBundle,
  idbUpdateProjectWithCompositionAssetBundle,
  idbWriteCompositionAssetBundle,
  PROJECTS_STORE,
  type StoredProjectRecord,
} from "@/lib/projectStorage";
import { removeCompositionClip } from "@/lib/projectComposition";
import type {
  Project,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";

const segment = {
  trimStart: 0,
  trimEnd: 5,
  trimSegments: [{ id: "trim", startTime: 0, endTime: 5 }],
} as VideoSegment;

function clip(id: string, role: "root" | "snapshot"): ProjectCompositionClip {
  return {
    id,
    role,
    name: id,
    duration: 5,
    segment,
    backgroundConfig: {
      scale: 100,
      borderRadius: 0,
      backgroundType: "solid",
      backgroundColor: "#000000",
    },
    mousePositions: [],
  };
}

function composition(clips: ProjectCompositionClip[]): ProjectComposition {
  return {
    mode: "separate",
    selectedClipId: clips[0].id,
    focusedClipId: clips[0].id,
    clips,
  };
}

function project(id: string, value: ProjectComposition): Project {
  return {
    id,
    name: id,
    createdAt: 1,
    lastModified: 1,
    duration: 5,
    segment,
    backgroundConfig: value.clips[0].backgroundConfig,
    mousePositions: [],
    composition: value,
    videoBlob: new NodeBlob([`root-${id}`]) as unknown as Blob,
  };
}

function readBlobText(blob: Blob): Promise<string> {
  return (blob as unknown as NodeBlob).text();
}

describe.sequential("project storage transactions", () => {
  it("rolls back a root project record when a media write cannot be cloned", async () => {
    const id = "fault-create-project";
    const invalid = project(id, composition([clip("root", "root")]));
    invalid.audioBlob = (() => undefined) as unknown as Blob;

    await expect(idbCreateProjectBundle(invalid)).rejects.toBeTruthy();
    expect(await idbGet(PROJECTS_STORE, id)).toBeNull();
    expect(await idbGet("videos", id)).toBeNull();
  });

  it("commits composition metadata and clip blobs together or not at all", async () => {
    const id = "fault-composition-project";
    const initial = composition([clip("root", "root")]);
    await idbCreateProjectBundle(project(id, initial));
    const next = composition([
      clip("root", "root"),
      clip("snapshot", "snapshot"),
    ]);

    await expect(idbUpdateProjectWithCompositionAssetBundle(
      id,
      "snapshot",
      next,
      { videoBlob: (() => undefined) as unknown as Blob },
    )).rejects.toBeTruthy();

    const stored = await idbGet<StoredProjectRecord>(PROJECTS_STORE, id);
    expect(stored?.composition?.clips.map((item) => item.id)).toEqual(["root"]);
    expect(await idbGet(
      "composition_videos",
      buildCompositionAssetKey(id, "snapshot"),
    )).toBeNull();
  });

  it("keeps retained clip media across remove, undo, and reload", async () => {
    const id = "undo-reload-project";
    const original = composition([
      clip("root", "root"),
      clip("snapshot", "snapshot"),
    ]);
    await idbCreateProjectBundle(project(id, original));
    const assetKey = buildCompositionAssetKey(id, "snapshot");
    await idbWriteCompositionAssetBundle(assetKey, {
      videoBlob: new NodeBlob(["snapshot-video"]) as unknown as Blob,
    });

    const removed = removeCompositionClip(original, "snapshot");
    await idbUpdateProjectBundle(id, { composition: removed });
    const afterRemove = await idbGet<StoredProjectRecord>(PROJECTS_STORE, id);
    expect(afterRemove?.composition?.retainedRemovedClips?.[0]?.id).toBe("snapshot");

    await idbUpdateProjectBundle(id, { composition: original });
    const reloaded = await idbGet<StoredProjectRecord>(PROJECTS_STORE, id);
    const reloadedAsset = await idbGet<Blob>("composition_videos", assetKey);
    expect(reloaded?.composition?.clips.map((item) => item.id)).toEqual([
      "root",
      "snapshot",
    ]);
    expect(reloadedAsset).not.toBeNull();
    expect(await readBlobText(reloadedAsset!)).toBe("snapshot-video");
  });

  it("rolls back database deletion when any store mutation faults", async () => {
    const id = "fault-delete-project";
    const value = composition([clip("root", "root")]);
    await idbCreateProjectBundle(project(id, value));
    const originalDelete = IDBObjectStore.prototype.delete;
    let deleteCalls = 0;
    IDBObjectStore.prototype.delete = function failingDelete(key: IDBValidKey) {
      deleteCalls += 1;
      if (deleteCalls === 3) throw new DOMException("injected failure", "DataError");
      return originalDelete.call(this, key);
    };
    try {
      await expect(idbDeleteProjectBundle(id, { composition: value })).rejects.toThrow(
        "injected failure",
      );
    } finally {
      IDBObjectStore.prototype.delete = originalDelete;
    }

    expect(await idbGet(PROJECTS_STORE, id)).not.toBeNull();
    const rootVideo = await idbGet<Blob>("videos", id);
    expect(rootVideo).not.toBeNull();
    expect(await readBlobText(rootVideo!)).toBe(`root-${id}`);
  });
});
