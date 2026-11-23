import "fake-indexeddb/auto";
import { describe, expect, it, vi } from "vitest";
import { projectManager } from "@/lib/projectManager";
import { invoke } from "@/lib/ipc";
import { idbCreateProjectBundle } from "@/lib/projectStorage";
import type { Project } from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));

const SNAPSHOT_DIR = "C:/Users/u/AppData/Roaming/screen-goated-toolbox/composition-snapshots";

function deletedPaths(): string[] {
  return vi.mocked(invoke).mock.calls
    .filter(([command]) => command === "delete_file")
    .map(([, args]) => (args as { path: string }).path);
}

describe("project delete removes snapshot clip media", () => {
  it("deletes every managed snapshot copy, including mic audio, but never a root path", async () => {
    vi.mocked(invoke).mockClear();
    projectManager.setActiveProjectId(null);
    const rootVideo = `${SNAPSHOT_DIR}/root-video.mp4`;
    await idbCreateProjectBundle({
      id: "snapshot-cleanup", name: "Snapshots", createdAt: 1, lastModified: 1, duration: 1,
      segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
      backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
      mousePositions: [], rawVideoPath: rootVideo,
      composition: {
        mode: "separate",
        clips: [
          { id: "root", role: "root", rawVideoPath: rootVideo },
          {
            id: "snap", role: "snapshot",
            rawVideoPath: `${SNAPSHOT_DIR}/snap-video.mp4`,
            rawMicAudioPath: `${SNAPSHOT_DIR}/snap-mic.wav`,
            rawWebcamVideoPath: `${SNAPSHOT_DIR}/snap-webcam.mp4`,
          },
        ],
        retainedRemovedClips: [
          { id: "old", role: "snapshot", rawVideoPath: rootVideo, rawMicAudioPath: `${SNAPSHOT_DIR}/old-mic.wav` },
        ],
      },
    } as unknown as Project);

    await projectManager.deleteProject("snapshot-cleanup");

    const deleted = deletedPaths();
    expect(deleted).toEqual(expect.arrayContaining([
      `${SNAPSHOT_DIR}/snap-video.mp4`,
      `${SNAPSHOT_DIR}/snap-mic.wav`,
      `${SNAPSHOT_DIR}/snap-webcam.mp4`,
      `${SNAPSHOT_DIR}/old-mic.wav`,
    ]));
    // The root video is deleted once by the root-media pass, never twice via a snapshot clip.
    expect(deleted.filter((path) => path === rootVideo)).toHaveLength(1);
  });
});
