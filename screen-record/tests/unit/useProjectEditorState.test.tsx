import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useProjectEditorState } from "@/hooks/useProjectEditorState";
import type { Project, ProjectComposition, VideoSegment } from "@/types/video";

const segment = {
  trimStart: 0,
  trimEnd: 5,
  trimSegments: [{ id: "trim", startTime: 0, endTime: 5 }],
} as VideoSegment;

const composition = (selectedClipId: string): ProjectComposition => ({
  mode: "separate",
  selectedClipId,
  focusedClipId: selectedClipId,
  clips: [{
    id: selectedClipId,
    role: "root",
    name: "Root",
    duration: 5,
    segment,
    backgroundConfig: {
      scale: 100,
      borderRadius: 0,
      backgroundType: "solid",
      backgroundColor: "#000000",
    },
    mousePositions: [],
  }],
});

const project = (value: ProjectComposition): Project => ({
  id: "project-a",
  name: "Project A",
  createdAt: 1,
  lastModified: 1,
  duration: 5,
  segment,
  backgroundConfig: value.clips[0].backgroundConfig,
  mousePositions: [],
  composition: value,
});

describe("useProjectEditorState composition authority", () => {
  it("keeps React state, the active project, and the synchronous ref converged", () => {
    const { result } = renderHook(() => useProjectEditorState());
    const initial = composition("root");

    act(() => result.current.setCurrentProjectData(project(initial)));
    expect(result.current.composition).toBe(initial);
    expect(result.current.currentProjectDataRef.current?.composition).toBe(initial);

    const changed = composition("changed-root");
    act(() => result.current.rawSetComposition(changed));
    expect(result.current.composition).toBe(changed);
    expect(result.current.currentProjectData?.composition).toBe(changed);
    expect(result.current.currentProjectDataRef.current).toBe(
      result.current.currentProjectData,
    );

    act(() => result.current.setCurrentProjectData(null));
    expect(result.current.composition).toBeNull();
    expect(result.current.currentProjectDataRef.current).toBeNull();
  });
});
