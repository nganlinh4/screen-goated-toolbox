import { describe, expect, it, vi } from "vitest";
import {
  getPreviewExportFrameRate,
  setPreviewExportFrameRate,
  subscribePreviewExportFrameRate,
} from "@/lib/exportFrameRateStore";

describe("preview export frame-rate store", () => {
  it("publishes normalized live frame-rate changes", () => {
    const listener = vi.fn();
    const unsubscribe = subscribePreviewExportFrameRate(listener);

    setPreviewExportFrameRate(29.7);
    expect(getPreviewExportFrameRate()).toBe(30);
    expect(listener).toHaveBeenCalledTimes(1);

    setPreviewExportFrameRate(30);
    expect(listener).toHaveBeenCalledTimes(1);

    setPreviewExportFrameRate(10_000);
    expect(getPreviewExportFrameRate()).toBe(240);
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    setPreviewExportFrameRate(60);
    expect(listener).toHaveBeenCalledTimes(2);
  });
});
