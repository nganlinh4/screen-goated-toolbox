import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@/lib/ipc";
import { stageBrowserCursorSlotTiles } from "@/lib/exporterCursorTiles";
import { videoRenderer } from "@/lib/videoRenderer";
import { exportCompositionAndDownload } from "@/lib/compositionExport";
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
