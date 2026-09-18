import { createPersistedSetting } from "./persistedState";
import { computeGifResolutionOptions, computeResolutionOptions, MIN_VIDEO_BITRATE_KBPS, MAX_VIDEO_BITRATE_KBPS } from "./exportEstimator";

export interface ExportDefaults {
  format: "mp4" | "gif";
  mp4Height: number | null;
  gifWidth: number | null;
  bitrateKbps: number;
  outputDir: string;
}

const DEFAULTS: ExportDefaults = {
  format: "mp4", mp4Height: null, gifWidth: null, bitrateKbps: 0, outputDir: "",
};
const validSize = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= 2 && value <= 16384;

const setting = createPersistedSetting<ExportDefaults>("screen-record-export-defaults-v1", {
  parse: (raw) => {
    const value = raw ? JSON.parse(raw) : {};
    return {
      format: value.format === "gif" ? "gif" : "mp4",
      mp4Height: validSize(value.mp4Height) ? value.mp4Height : null,
      gifWidth: validSize(value.gifWidth) ? value.gifWidth : null,
      bitrateKbps: typeof value.bitrateKbps === "number" && Number.isFinite(value.bitrateKbps)
        ? value.bitrateKbps > 0 ? Math.max(MIN_VIDEO_BITRATE_KBPS, Math.min(MAX_VIDEO_BITRATE_KBPS, value.bitrateKbps)) : 0 : 0,
      outputDir: typeof value.outputDir === "string" ? value.outputDir : "",
    };
  },
  serialize: JSON.stringify,
  fallback: DEFAULTS,
});

export const getExportDefaults = () => ({ ...setting.getInitial() });
export function saveExportDefaults(patch: Partial<ExportDefaults>) {
  const next = { ...getExportDefaults(), ...patch };
  setting.persist(next);
  return next;
}

export function resolvePreferredResolution(
  preferences: ExportDefaults, format: "mp4" | "gif", width: number, height: number,
) {
  if (format === "gif") {
    const options = computeGifResolutionOptions(width, height);
    return options.find((option) => option.width === preferences.gifWidth) ?? options[0];
  }
  if (preferences.mp4Height === null) return { width: 0, height: 0 };
  return computeResolutionOptions(width, height)
    .find((option) => option.height === preferences.mp4Height) ?? { width: 0, height: 0 };
}
