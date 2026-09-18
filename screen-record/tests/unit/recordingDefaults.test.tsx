import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getDefaultAudioVolume, getNewRecordingCameraConfig, saveCameraDefaults, saveUniformAudioDefault } from "@/lib/recordingDefaults";
import { getDefaultStyle, saveStyleDefault } from "@/lib/stylePreferences";
import { defaultSubtitleStyle, createManualSubtitleSegment } from "@/lib/subtitleDefaults";
import { getExportDefaults, resolvePreferredResolution, saveExportDefaults } from "@/lib/exportPreferences";
import { createInitialExportOptions } from "@/hooks/exportHookUtils";
import { useSegmentInitializer, type UseSegmentInitializerParams } from "@/hooks/useSegmentInitializer";
import { useTextOverlays } from "@/hooks/useTextOverlays";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import type { VideoSegment } from "@/types/video";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: {} }));

beforeEach(() => localStorage.clear());

describe("new-media defaults", () => {
  it("remembers camera visibility and appearance without unavailable media overwriting them", () => {
    const config = { ...getNewRecordingCameraConfig(true), mirror: true, maxSizePercent: 30 };
    saveCameraDefaults(config);
    expect(getNewRecordingCameraConfig(false)).toEqual({ ...config, visible: false });
    expect(getNewRecordingCameraConfig(true)).toEqual(config);
    saveCameraDefaults({ ...config, visible: false });
    expect(getNewRecordingCameraConfig(true).visible).toBe(false);
  });

  it("remembers a uniform level including mute, but ignores automation", () => {
    saveUniformAudioDefault("device", [{ time: 0, volume: 0.4 }, { time: 10, volume: 0.4 }]);
    saveUniformAudioDefault("device", [{ time: 0, volume: 0.1 }, { time: 10, volume: 0.8 }]);
    saveUniformAudioDefault("mic", [{ time: 0, volume: 0 }]);
    expect(getDefaultAudioVolume("device", 1)).toBe(0.4);
    expect(getDefaultAudioVolume("mic", 1)).toBe(0);
  });

  it("uses saved audio and keystroke delay only when initializing new media", () => {
    localStorage.setItem("screen-record-keystroke-delay-v1", "0.35");
    saveUniformAudioDefault("device", [{ time: 0, volume: 0.4 }]);
    saveUniformAudioDefault("mic", [{ time: 0, volume: 0.7 }]);
    const setSegment = vi.fn();
    const props: UseSegmentInitializerParams = {
      duration: 12, segment: null, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
      mousePositions: [], currentMicAudio: null, currentWebcamVideo: null,
      setSegment, videoRef: { current: null }, canvasRef: { current: null }, tempCanvasRef: { current: null },
    };
    const { rerender } = renderHook((value) => useSegmentInitializer(value), { initialProps: props });
    expect(setSegment).toHaveBeenCalledWith(expect.objectContaining({
      keystrokeDelaySec: 0.35,
      deviceAudioPoints: [{ time: 0, volume: 0.4 }, { time: 12, volume: 0.4 }],
      micAudioPoints: [{ time: 0, volume: 0.7 }, { time: 12, volume: 0.7 }],
      deviceAudioOffsetSec: 0, micAudioOffsetSec: 0, webcamOffsetSec: 0,
    }));
    setSegment.mockClear();
    rerender({ ...props, segment: { trimStart: 0, trimEnd: 12, zoomKeyframes: [], textSegments: [], keystrokeDelaySec: -0.1 } });
    expect(setSegment).not.toHaveBeenCalled();
  });

  it("keeps text and subtitle styles separate and copies no content or timing", () => {
    const base = defaultSubtitleStyle();
    saveStyleDefault("subtitle", { ...base, fontSize: 70, y: 85 });
    saveStyleDefault("text", { ...base, fontSize: 120, y: 25 });
    expect(createManualSubtitleSegment(10, 30)).toMatchObject({
      text: "New Subtitle", startTime: 8.5, endTime: 11.5, style: { fontSize: 70, y: 85 },
    });
    const segment: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [] };
    const setSegment = vi.fn();
    const { result } = renderHook(() => useTextOverlays({ segment, setSegment, currentTime: 5, duration: 30, setActivePanel: vi.fn() }));
    act(() => result.current.handleAddText());
    expect(setSegment).toHaveBeenCalledWith(expect.objectContaining({
      textSegments: [expect.objectContaining({ text: "New Text", startTime: 3.5, style: expect.objectContaining({ fontSize: 120, y: 25 }) })],
    }));
    const first = getDefaultStyle("text", base);
    first.background!.opacity = 0;
    expect(getDefaultStyle("text", base).background?.opacity).not.toBe(0);
  });

  it("restores export intent across aspect ratios and keeps format resolutions separate", () => {
    saveExportDefaults({ mp4Height: 720, gifWidth: 480, format: "gif", outputDir: "D:\\Exports", bitrateKbps: 12000 });
    const saved = getExportDefaults();
    expect(resolvePreferredResolution(saved, "mp4", 1920, 1080)).toMatchObject({ width: 1280, height: 720 });
    expect(resolvePreferredResolution(saved, "mp4", 1080, 1920)).toMatchObject({ width: 404, height: 720 });
    expect(resolvePreferredResolution(saved, "gif", 1920, 1080)).toMatchObject({ width: 480, height: 270 });
    expect(createInitialExportOptions()).toMatchObject({ format: "gif", outputDir: "D:\\Exports", targetVideoBitrateKbps: 12000 });
    saveExportDefaults({ mp4Height: null, bitrateKbps: 0 });
    expect(resolvePreferredResolution(getExportDefaults(), "mp4", 1080, 1920)).toEqual({ width: 0, height: 0 });
    expect(createInitialExportOptions().targetVideoBitrateKbps).toBe(0);
  });

  it("falls back safely for corrupt storage", () => {
    for (const key of ["camera-default", "text-style-default", "export-defaults"]) {
      localStorage.setItem(`screen-record-${key}-v1`, "null");
    }
    expect(getNewRecordingCameraConfig(true).visible).toBe(true);
    expect(getExportDefaults().format).toBe("mp4");
    expect(getDefaultStyle("text", defaultSubtitleStyle()).fontSize).toBe(54);
  });
});
