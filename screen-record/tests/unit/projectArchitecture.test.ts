import { describe, expect, it } from "vitest";
import type {
  BackgroundConfig,
  Project,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";
import {
  ensureProjectComposition,
  removeCompositionClip,
} from "@/lib/projectComposition";
import {
  collectProjectCustomBackgroundUrls,
  MAX_PROJECT_NAME_LENGTH,
  normalizeProjectName,
} from "@/lib/projectMetadata";
import { ProjectWriteCoordinator } from "@/lib/projectWriteCoordinator";

const background = (customBackground?: string): BackgroundConfig => ({
  scale: 1,
  borderRadius: 0,
  backgroundType: "solid",
  backgroundColor: "#000000",
  customBackground,
});

const segment = (): VideoSegment =>
  ({
    trimStart: 0,
    trimEnd: 5,
    trimSegments: [{ id: "trim", startTime: 0, endTime: 5 }],
  }) as VideoSegment;

const clip = (id: string, customBackground?: string): ProjectCompositionClip => ({
  id,
  role: id === "root" ? "root" : "snapshot",
  name: id,
  duration: 5,
  segment: segment(),
  backgroundConfig: background(customBackground),
  mousePositions: [],
});

const project = (): Project => ({
  id: "project",
  name: "Project",
  createdAt: 1,
  lastModified: 1,
  duration: 5,
  segment: segment(),
  backgroundConfig: background("root-background"),
  mousePositions: [],
  composition: {
    mode: "separate",
    selectedClipId: "root",
    focusedClipId: "root",
    clips: [clip("root"), clip("snapshot", "clip-background")],
    retainedRemovedClips: [clip("removed", "removed-background")],
    audioSegments: [
      {
        id: "audio",
        rawAudioPath: "audio.wav",
        name: "Audio",
        duration: 5,
        startTime: 0,
        inPoint: 0,
        outPoint: 5,
        addedAt: 1,
      },
    ],
    audioTrackVolumePoints: [{ time: 0, volume: 0.75 }],
    narrationSegments: [
      {
        id: "narration",
        rawAudioPath: "narration.wav",
        name: "Narration",
        duration: 5,
        startTime: 0,
        inPoint: 0,
        outPoint: 5,
        addedAt: 1,
      },
    ],
    narrationTrackVolumePoints: [{ time: 0, volume: 0.5 }],
  },
});

describe("project state architecture", () => {
  it("skips an obsolete queued editor save and applies the latest save", async () => {
    const coordinator = new ProjectWriteCoordinator();
    let release!: () => void;
    const barrier = new Promise<void>((resolve) => {
      release = resolve;
    });
    const writes: string[] = [];
    const blocker = coordinator.enqueue("project", async () => barrier);
    const oldIntent = coordinator.createEditorIntent("project");
    const oldSave = coordinator.enqueue(
      "project",
      async () => writes.push("old"),
      oldIntent,
    );
    const latestIntent = coordinator.createEditorIntent("project");
    const latestSave = coordinator.enqueue(
      "project",
      async () => writes.push("latest"),
      latestIntent,
    );

    release();
    await blocker;
    expect(await oldSave).toEqual({ applied: false });
    expect((await latestSave).applied).toBe(true);
    expect(writes).toEqual(["latest"]);
  });

  it("keeps the write queue usable after a failed mutation", async () => {
    const coordinator = new ProjectWriteCoordinator();
    await expect(
      coordinator.enqueue("project", async () => {
        throw new Error("write failed");
      }),
    ).rejects.toThrow("write failed");
    const next = await coordinator.enqueue("project", async () => "saved");
    expect(next).toEqual({ applied: true, value: "saved" });
  });

  it("normalizes names without allowing empty or control-only values", () => {
    expect(normalizeProjectName("  My\n Project  ")).toBe("My Project");
    expect(normalizeProjectName("Client/Launch:Final?.mp4")).toBe(
      "Client Launch Final .mp4",
    );
    expect(normalizeProjectName("CON")).toBe("Project CON");
    expect(normalizeProjectName("lpt1.txt")).toBe("Project lpt1.txt");
    expect(normalizeProjectName("Project...   ")).toBe("Project");
    expect(() => normalizeProjectName("\u0000\n")).toThrow();
    expect(() => normalizeProjectName("... ")).toThrow();
    expect(normalizeProjectName("🙂".repeat(200))).toHaveLength(
      MAX_PROJECT_NAME_LENGTH * 2,
    );
  });

  it("preserves narration and both volume envelopes during normalization", () => {
    const normalized = ensureProjectComposition(project());
    expect(normalized.audioSegments).toHaveLength(1);
    expect(normalized.audioTrackVolumePoints).toEqual([
      { time: 0, volume: 0.75 },
    ]);
    expect(normalized.narrationSegments).toHaveLength(1);
    expect(normalized.narrationTrackVolumePoints).toEqual([
      { time: 0, volume: 0.5 },
    ]);
    expect(normalized.retainedRemovedClips?.[0]?.id).toBe("removed");
  });

  it("retains removed clip metadata so undo still has durable media references", () => {
    const source = project().composition!;
    const removed = removeCompositionClip(source, "snapshot");
    expect(removed.clips.map((item) => item.id)).toEqual(["root"]);
    expect(removed.retainedRemovedClips?.map((item) => item.id)).toEqual([
      "removed",
      "snapshot",
    ]);
  });

  it("retains custom backgrounds used by active and removed composition clips", () => {
    expect(collectProjectCustomBackgroundUrls([project()])).toEqual(
      expect.arrayContaining([
        "root-background",
        "clip-background",
        "removed-background",
      ]),
    );
  });
});
