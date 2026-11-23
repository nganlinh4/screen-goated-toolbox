import "fake-indexeddb/auto";
import { expect, it, vi } from "vitest";
import { projectManager } from "@/lib/projectManager";
import type { Project, VideoSegment } from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));

it("reuses the persisted recording after a lost acknowledgement without overwriting it", async () => {
  const source = {
    name: "Original recording",
    segment: { trimStart: 0, trimEnd: 2 } as VideoSegment,
    backgroundConfig: { backgroundType: "solid", backgroundColor: "#000" } as Project["backgroundConfig"],
    mousePositions: [],
    rawVideoPath: "C:/recordings/original.mp4",
  };
  const id = "b".repeat(32);
  const first = await projectManager.saveProject(source, id);
  const retried = await projectManager.saveProject({ ...source, name: "Later retry" }, id);
  expect(retried.id).toBe(first.id);
  expect(retried.name).toBe("Original recording");
  await expect(projectManager.saveProject({ ...source, rawVideoPath: "C:/different.mp4" }, id)).rejects.toThrow();
  expect((await projectManager.getProjects()).filter((project) => project.id === first.id)).toHaveLength(1);
});

it("serializes competing saves for the same recording identity", async () => {
  const source = {
    name: "Recording",
    segment: { trimStart: 0, trimEnd: 1 } as VideoSegment,
    backgroundConfig: { backgroundType: "solid" } as Project["backgroundConfig"],
    mousePositions: [],
    rawVideoPath: "C:/recordings/other.mp4",
  };
  const id = "c".repeat(32);
  const [first, second] = await Promise.all([
    projectManager.saveProject(source, id),
    projectManager.saveProject(source, id),
  ]);
  expect(first.id).toBe(second.id);
  expect((await projectManager.getProjects()).filter((project) => project.id === first.id)).toHaveLength(1);
});
