import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@/lib/ipc";
import { stageBrowserCursorSlotTiles } from "@/lib/exporterCursorTiles";
import { videoRenderer } from "@/lib/videoRenderer";
import { buildCompositionExportDialogState, exportCompositionAndDownload } from "@/lib/compositionExport";
import { createSubtitleTrackStateFromSegments } from "@/lib/subtitleTracks";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";
import type {
  BackgroundConfig,
  ExportOptions,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/exporterCursorTiles", () => ({
  stageBrowserCursorSlotTiles: vi.fn(),
}));
vi.mock("@/lib/videoRenderer", () => ({
  videoRenderer: {
    bakeOverlayAtlasAndPaths: vi.fn(),
    iterateBakedWebcamFrames: vi.fn(() => []),
  },
}));

const invokeMock = vi.mocked(invoke);
const cursorStageMock = vi.mocked(stageBrowserCursorSlotTiles);
const overlayBakeMock = vi.mocked(videoRenderer.bakeOverlayAtlasAndPaths);

function createComposition(): ProjectComposition {
  const segment = {
    trimStart: 0,
    trimEnd: 1,
    trimSegments: [{ id: "trim", startTime: 0, endTime: 1 }],
    speedPoints: [{ time: 0, speed: 1 }, { time: 1, speed: 1 }],
    zoomKeyframes: [],
    textSegments: [{ id: "text", text: "Hello", startTime: 0, endTime: 1 }],
    subtitleSegments: [],
    deviceAudioAvailable: false,
  } as VideoSegment;
  const backgroundConfig = {
    scale: 100,
    borderRadius: 0,
    backgroundType: "solid",
    shadow: 0,
    canvasMode: "auto",
  } as BackgroundConfig;
  return {
    mode: "separate",
    selectedClipId: "clip",
    focusedClipId: "clip",
    clips: [{
      id: "clip",
      role: "root",
      name: "Clip",
      duration: 1,
      segment,
      backgroundConfig,
      mousePositions: [],
    }],
  };
}

describe("composition export overlay staging", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    cursorStageMock.mockReset().mockResolvedValue(undefined);
    overlayBakeMock.mockReset().mockResolvedValue({
      atlasBase64: "data:image/png;base64,AA==",
      atlasWidth: 32,
      atlasHeight: 16,
      frames: [],
      atlasMetadata: { textEntries: [{ id: "text" }] },
    });
    invokeMock.mockImplementation(async (command) => {
      if (command === "probe_video_metadata") {
        return { width: 1920, height: 1080, fps: 60, duration: 1 };
      }
      if (command === "start_composition_export_server") {
        return { status: "success", path: "output.mp4" };
      }
      return undefined;
    });
  });

  it.each(["separate", "unified"] as const)("projects sequence-owned audio captions into each %s clip export", async (mode) => {
    const composition = createComposition();
    composition.mode = mode;
    const root = composition.clips[0];
    composition.clips.push({ ...root, id: "second", duration: 10, segment: {
      ...root.segment, trimStart: 5, trimEnd: 7,
      trimSegments: [{ id: "second-range", startTime: 5, endTime: 7 }],
    } });
    composition.globalSegment = { ...root.segment, trimEnd: 3,
      ...createSubtitleTrackStateFromSegments([
        { id: "audio-cue", startTime: 1.25, endTime: 1.75, text: "Audio caption", style: defaultSubtitleStyle(),
          sourceGroup: { kind: "audio", audioSegmentId: "audio-owner" } },
      ]),
    };
    invokeMock.mockImplementation(async (command) => command === "probe_video_metadata"
      ? { width: 1920, height: 1080, fps: 60, duration: 10 }
      : command === "start_composition_export_server" ? { status: "success" } : undefined);
    await exportCompositionAndDownload({ composition,
      exportOptions: { width: 1920, height: 1080, fps: 60, targetVideoBitrateKbps: 8_000, format: "mp4" } as ExportOptions,
      resolveClipSourcePath: async () => "source.mp4", resolveClipMicAudioPath: async () => "", resolveClipWebcamPath: async () => "",
    });
    expect(overlayBakeMock.mock.calls[0][0].subtitleSegments).toEqual([]);
    expect(overlayBakeMock.mock.calls[1][0].subtitleSegments).toEqual([
      expect.objectContaining({ id: "audio-cue", startTime: 5.25, endTime: 5.75, text: "Audio caption" }),
    ]);
    expect(composition.clips.every((clip) => clip.segment.subtitleSegments?.length === 0)).toBe(true);
    const dialog = await buildCompositionExportDialogState(composition, async () => "source.mp4");
    expect(dialog.segment?.subtitleSegments).toEqual([
      expect.objectContaining({ id: "audio-cue", startTime: 1.25, endTime: 1.75 }),
    ]);
  });

  it("retains atlas metadata in the session-scoped clip staging path", async () => {
    const exportOptions = {
      width: 0,
      height: 0,
      fps: 60,
      targetVideoBitrateKbps: 8_000,
      format: "mp4",
    } as ExportOptions;

    await exportCompositionAndDownload({
      composition: createComposition(),
      exportOptions,
      resolveClipSourcePath: async () => "source.mp4",
      resolveClipMicAudioPath: async () => "",
      resolveClipWebcamPath: async () => "",
    });

    const stageCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "stage_export_data",
    );
    expect(stageCalls).toHaveLength(2);
    expect(stageCalls[0][1]).toMatchObject({
      jobId: "clip",
      dataType: "atlas",
    });
    expect(stageCalls[1][1]).toMatchObject({
      jobId: "clip",
      dataType: "overlay_atlas_metadata",
      data: { textEntries: [{ id: "text" }] },
    });
    expect(stageCalls[0][1]).toHaveProperty("sessionId");
  });

  it("scales preview-authored overlay and frame styling to the chosen output size", async () => {
    const composition = createComposition();
    composition.clips[0].backgroundConfig = {
      ...composition.clips[0].backgroundConfig, borderRadius: 24, shadow: 40,
    };
    await exportCompositionAndDownload({
      composition,
      exportOptions: { width: 960, height: 540, fps: 60, targetVideoBitrateKbps: 8_000, format: "mp4" } as ExportOptions,
      resolveClipSourcePath: async () => "source.mp4",
      resolveClipMicAudioPath: async () => "",
      resolveClipWebcamPath: async () => "",
    });
    // The clip previews on its 1920x1080 canvas; the overlay bake gets that as its reference.
    expect(overlayBakeMock.mock.calls[0].slice(1, 4)).toEqual([960, 540, { width: 1920, height: 1080 }]);
    const request = invokeMock.mock.calls.find(([command]) => command === "start_composition_export_server")![1] as {
      clips: Array<{ backgroundConfig: BackgroundConfig }>;
    };
    expect(request.clips[0].backgroundConfig).toMatchObject({ borderRadius: 12, shadow: 20 });
  });

  it.each(["resolve", "reject"] as const)(
    "keeps staged data until the native export %ss",
    async (outcome) => {
      let settle!: (value: unknown) => void;
      let fail!: (reason: Error) => void;
      const nativeExport = new Promise((resolve, reject) => {
        settle = resolve;
        fail = reject;
      });
      invokeMock.mockImplementation(async (command) => {
        if (command === "probe_video_metadata") {
          return { width: 1920, height: 1080, fps: 60, duration: 1 };
        }
        if (command === "start_composition_export_server") return nativeExport;
        return undefined;
      });
      const operation = exportCompositionAndDownload({
        composition: createComposition(),
        exportOptions: {
          width: 0, height: 0, fps: 60, targetVideoBitrateKbps: 8_000, format: "mp4",
        } as ExportOptions,
        resolveClipSourcePath: async () => "source.mp4",
        resolveClipMicAudioPath: async () => "",
        resolveClipWebcamPath: async () => "",
      });
      // Attach the rejection handler before rejecting the deferred IPC response.
      const result = operation.then(value => ({ value }), error => ({ error }));
      await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
        "start_composition_export_server", expect.any(Object),
      ));
      expect(invokeMock.mock.calls.filter(([command]) => command === "clear_export_staging"))
        .toHaveLength(1);
      if (outcome === "resolve") settle({ status: "success", path: "output.mp4" });
      else fail(new Error("native export failed"));
      const settled = await result;
      if (outcome === "resolve") expect(settled).toHaveProperty("value.status", "success");
      else expect(settled).toHaveProperty("error.message", "native export failed");
      expect(invokeMock.mock.calls.filter(([command]) => command === "clear_export_staging"))
        .toHaveLength(2);
    },
  );

  it("never starts a native job after cancellation during deferred preparation", async () => {
    let cancelled = false;
    let release!: () => void;
    const sourcePath = new Promise<string>((resolve) => {
      release = () => resolve("source.mp4");
    });
    const operation = exportCompositionAndDownload({
      composition: createComposition(),
      exportOptions: {
        width: 0,
        height: 0,
        fps: 60,
        targetVideoBitrateKbps: 8_000,
        format: "mp4",
      } as ExportOptions,
      resolveClipSourcePath: () => sourcePath,
      resolveClipMicAudioPath: async () => "",
      resolveClipWebcamPath: async () => "",
      isCancelled: () => cancelled,
    });

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "clear_export_staging",
        expect.any(Object),
      );
    });
    cancelled = true;
    release();

    await expect(operation).rejects.toMatchObject({ name: "ExportCancelledError" });
    expect(invokeMock).not.toHaveBeenCalledWith(
      "start_composition_export_server",
      expect.any(Object),
    );
  });
});
