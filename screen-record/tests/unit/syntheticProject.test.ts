import { describe, expect, it } from "vitest";
import { createSyntheticProjectFixture } from "@/testHarness/syntheticProject";

describe("synthetic editor fixture", () => {
  it("creates a stress project with dense subtitles, narration, and audio", () => {
    const project = createSyntheticProjectFixture({ profile: "huge" });

    expect(project.composition?.clips).toHaveLength(1);
    expect(project.segment.subtitleSegments).toHaveLength(10_000);
    expect(project.composition?.narrationSegments).toHaveLength(1_000);
    expect(project.composition?.audioSegments).toHaveLength(80);
    expect(project.composition?.timelineOnly).toBe(true);
  });
});
