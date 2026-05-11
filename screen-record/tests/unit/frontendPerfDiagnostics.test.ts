import { describe, expect, it, vi } from "vitest";
import {
  getFrontendPerfSnapshot,
  markFrontendPerfEvent,
  resetFrontendPerfDiagnostics,
  startFrontendFrameProbe,
  stopFrontendFrameProbe,
} from "@/lib/frontendPerfDiagnostics";

describe("frontend perf diagnostics", () => {
  it("records named performance events", () => {
    resetFrontendPerfDiagnostics();
    markFrontendPerfEvent("fixture-loaded");

    expect(getFrontendPerfSnapshot().events).toEqual([
      expect.objectContaining({ label: "fixture-loaded" }),
    ]);
  });

  it("summarizes requestAnimationFrame deltas", () => {
    resetFrontendPerfDiagnostics();
    let raf: FrameRequestCallback | null = null;
    const rafSpy = vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => {
      raf = cb;
      return 1;
    });
    const cancelSpy = vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});

    startFrontendFrameProbe();
    raf?.(0);
    raf?.(16);
    raf?.(50);
    const summary = stopFrontendFrameProbe();

    expect(summary.sampleCount).toBeGreaterThanOrEqual(2);
    expect(summary.maxFrameDeltaMs).toBeGreaterThanOrEqual(34);

    rafSpy.mockRestore();
    cancelSpy.mockRestore();
  });
});
