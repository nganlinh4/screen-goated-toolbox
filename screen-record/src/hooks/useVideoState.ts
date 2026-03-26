// Barrel file — re-exports all sub-modules that were previously in this file.
// The main useVideoState() composition hook is not present here because the
// codebase never defined one; each sub-hook is consumed independently.

// Preferences / localStorage utilities
export {
  DEFAULT_KEYSTROKE_DELAY_SEC,
  DEFAULT_EXPORT_FPS,
  MIN_EXPORT_FPS,
  MAX_EXPORT_FPS,
  TRACK_DELAY_LIMIT_SEC,
  MIN_CROP_SIZE,
  PROJECT_LOAD_DEBUG,
  PROJECT_SWITCH_DEBUG,
  VALID_KEYSTROKE_LANGUAGES,
  normalizeTrackDelaySec,
  summarizeLoadedBackground,
  getSavedKeystrokeDelaySec,
  getSavedKeystrokeLanguage,
  saveKeystrokeLanguage,
  getSavedKeystrokeModePref,
  getSavedKeystrokeOverlayPref,
  normalizeCropRect,
  getSavedCropPref,
  saveCropPref,
  getSavedAutoZoomPref,
  saveAutoZoomPref,
  getSavedAutoZoomConfig,
  saveAutoZoomConfig,
  getSavedSmartPointerPref,
  saveSmartPointerPref,
  getSavedExportFpsPref,
} from "./videoStatePreferences";
export type { KeystrokeLanguage } from "./videoStatePreferences";

// Small helpers
export {
  hasValidCaptureDimensions,
  stabilizeMousePositionsForTimeline,
} from "./videoStateHelpers";

// Hooks
export { useVideoPlayback } from "./useVideoPlayback";
export { useRecording } from "./useRecording";
export { useProjects } from "./useProjects";
export { useExport } from "./useExport";
export { useZoomKeyframes } from "./useZoomKeyframes";
export { useTextOverlays } from "./useTextOverlays";
export { useAutoZoom } from "./useAutoZoom";
export { useCursorHiding } from "./useCursorHiding";
