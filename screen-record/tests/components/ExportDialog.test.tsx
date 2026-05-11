import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ExportDialog } from "@/components/dialogs/ExportDialog";
import { invoke } from "@/lib/ipc";
import { videoExporter } from "@/lib/videoExporter";
import type { BackgroundConfig, ExportOptions, VideoSegment } from "@/types/video";

vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn(async (command: string) => {
    if (command === "get_default_export_dir") return "C:\\Users\\user\\Downloads";
    if (command === "get_export_capabilities") {
      return {
        pipeline: "zero_copy_gpu",
        nvenc_available: true,
        native_gif_available: true,
      };
    }
    return null;
  }),
}));

vi.mock("@/lib/videoExporter", async () => {
  const estimator = await import("@/lib/exportEstimator");
  return {
    computeResolutionOptions: estimator.computeResolutionOptions,
    computeGifResolutionOptions: estimator.computeGifResolutionOptions,
    GIF_MAX_WIDTH: estimator.GIF_MAX_WIDTH,
    computeBitrateSliderBounds: estimator.computeBitrateSliderBounds,
    getCanvasBaseDimensions: estimator.getCanvasBaseDimensions,
    resolveExportDimensions: estimator.resolveExportDimensions,
    estimateExportSize: estimator.estimateExportSize,
    videoExporter: {
      getExportCapabilities: vi.fn(async () => ({
        pipeline: "zero_copy_gpu",
        nvencAvailable: true,
        nativeGifAvailable: true,
      })),
    },
  };
});

const invokeMock = vi.mocked(invoke);
const getExportCapabilitiesMock = vi.mocked(videoExporter.getExportCapabilities);

const baseSegment: VideoSegment = {
  trimStart: 0,
  trimEnd: 8,
  trimSegments: [{ id: "full", startTime: 0, endTime: 8 }],
  zoomKeyframes: [],
  textSegments: [],
};

const background: BackgroundConfig = {
  scale: 1,
  borderRadius: 0,
  backgroundType: "solid",
};

const baseOptions: ExportOptions = {
  width: 0,
  height: 0,
  fps: 60,
  targetVideoBitrateKbps: 4500,
  outputDir: "C:\\Users\\user\\Downloads",
  format: "mp4",
};

function Harness({
  initial,
  sourceVideoFps = 50,
}: {
  initial: ExportOptions;
  sourceVideoFps?: number;
}) {
  const [exportOptions, setExportOptions] = useState(initial);
  return (
    <>
      <output data-testid="export-state">{JSON.stringify(exportOptions)}</output>
      <ExportDialog
        show
        onClose={() => {}}
        onExport={() => {}}
        exportOptions={exportOptions}
        setExportOptions={setExportOptions}
        segment={baseSegment}
        videoRef={{ current: null }}
        backgroundConfig={background}
        hasAudio
        sourceVideoFps={sourceVideoFps}
        trimmedDurationSec={8}
        autoCopyEnabled={false}
        onToggleAutoCopy={() => {}}
      />
    </>
  );
}

function readExportState(): ExportOptions {
  return JSON.parse(screen.getByTestId("export-state").textContent ?? "{}") as ExportOptions;
}

describe("ExportDialog format-safe defaults", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    getExportCapabilitiesMock.mockClear();
    localStorage.removeItem("screen-record-export-fps-pref-v1");
  });

  it("converts stale GIF state back to visible MP4 FPS and original resolution", async () => {
    render(<Harness initial={{ ...baseOptions, format: "mp4", fps: 10, width: 960, height: 540 }} />);

    await waitFor(() => {
      expect(readExportState()).toMatchObject({
        format: "mp4",
        fps: 50,
        width: 0,
        height: 0,
      });
    });
    expect(screen.getByRole("button", { name: /50 fps/i })).toHaveClass("ui-chip-button-active");
  });

  it("resets legacy combined export format to MP4 on dialog open", async () => {
    render(<Harness initial={{ ...baseOptions, format: "both", fps: 15 }} />);

    await waitFor(() => {
      expect(readExportState().format).toBe("mp4");
    });
  });

  it("keeps GIF inside GIF FPS and width caps, then restores MP4 source FPS on switch back", async () => {
    render(<Harness initial={{ ...baseOptions, format: "mp4", fps: 120 }} sourceVideoFps={48} />);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /gif/i }));
    });
    await waitFor(() => {
      expect(readExportState()).toMatchObject({
        format: "gif",
        fps: 24,
        width: 960,
        height: 540,
      });
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /mp4/i }));
    });
    await waitFor(() => {
      expect(readExportState()).toMatchObject({
        format: "mp4",
        fps: 24,
        width: 0,
        height: 0,
      });
    });
  });

  it("loads export capabilities and default directory through IPC when needed", async () => {
    render(<Harness initial={{ ...baseOptions, outputDir: "", fps: 30 }} />);

    await waitFor(() => {
      expect(readExportState().outputDir).toBe("C:\\Users\\user\\Downloads");
    });
    expect(invokeMock).toHaveBeenCalledWith("get_default_export_dir");
    expect(getExportCapabilitiesMock).toHaveBeenCalled();
  });
});
