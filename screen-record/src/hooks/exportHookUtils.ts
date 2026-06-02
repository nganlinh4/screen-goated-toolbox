import { ExportArtifact, ExportOptions } from "@/types/video";
import { getSavedExportFpsPref } from "./videoStatePreferences";

export interface NativeVideoMetadataProbe {
  width: number;
  height: number;
  fps: number;
  fpsNum: number;
  fpsDen: number;
}

export function createInitialExportOptions(): ExportOptions {
  return {
    width: 0,
    height: 0,
    fps: getSavedExportFpsPref(),
    targetVideoBitrateKbps: 0,
    speed: 1,
    exportProfile: "turbo_nv",
    preferNvTurbo: true,
    qualityGatePercent: 3,
    turboCodec: "hevc",
    preRenderPolicy: "aggressive",
    outputDir: "",
    format: "mp4",
  };
}

export function getExportFailureMessage(error: unknown): string {
  const raw =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : String(error ?? "");
  if (raw.includes("0x80070070") || /not enough space on the disk/i.test(raw)) {
    return "Export failed because the output drive is full. Free up disk space or choose another export folder, then export again.";
  }
  if (/Export already in progress/i.test(raw)) {
    return "An export is still finishing or cleaning up. Wait a moment, or restart the app if it stays stuck.";
  }
  return raw || "Export failed for an unknown reason.";
}

export function normalizeExportArtifacts(
  result:
    | {
        status?: string;
        path?: string;
        artifacts?: ExportArtifact[];
      }
    | undefined,
): ExportArtifact[] {
  if (Array.isArray(result?.artifacts) && result.artifacts.length > 0) {
    return result.artifacts;
  }
  if (typeof result?.path === "string" && result.path) {
    return [
      {
        format: result.path.toLowerCase().endsWith(".gif") ? "gif" : "mp4",
        path: result.path,
        primary: true,
      },
    ];
  }
  return [];
}
