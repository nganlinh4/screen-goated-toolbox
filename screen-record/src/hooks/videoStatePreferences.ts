import {
  CropRect,
  AutoZoomConfig,
  DEFAULT_AUTO_ZOOM_CONFIG,
} from "@/types/video";

export const DEFAULT_KEYSTROKE_DELAY_SEC = 0;
const KEYSTROKE_DELAY_KEY = "screen-record-keystroke-delay-v1";
const KEYSTROKE_LANGUAGE_KEY = "screen-record-keystroke-language-v1";
const KEYSTROKE_MODE_PREF_KEY = "screen-record-keystroke-mode-pref-v1";
const KEYSTROKE_OVERLAY_PREF_KEY = "screen-record-keystroke-overlay-pref-v1";
const AUTO_ZOOM_PREF_KEY = "screen-record-auto-zoom-pref-v1";
const AUTO_ZOOM_CONFIG_KEY = "screen-record-auto-zoom-config-v1";
const SMART_POINTER_PREF_KEY = "screen-record-smart-pointer-pref-v1";
const EXPORT_FPS_PREF_KEY = "screen-record-export-fps-pref-v1";
const CROP_PREF_KEY = "screen-record-crop-pref-v1";
export const DEFAULT_EXPORT_FPS = 60;
export const MIN_EXPORT_FPS = 1;
export const MAX_EXPORT_FPS = 240;
export const TRACK_DELAY_LIMIT_SEC = 2;
export const MIN_CROP_SIZE = 0.05;
export const PROJECT_LOAD_DEBUG = false;
export const PROJECT_SWITCH_DEBUG = false;

export const VALID_KEYSTROKE_LANGUAGES = ["en", "ko", "vi", "es", "ja", "zh"] as const;
export type KeystrokeLanguage = (typeof VALID_KEYSTROKE_LANGUAGES)[number];

export function normalizeTrackDelaySec(value: number | null | undefined) {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.max(-TRACK_DELAY_LIMIT_SEC, Math.min(TRACK_DELAY_LIMIT_SEC, value));
}

import type { BackgroundConfig } from "@/types/video";

export function summarizeLoadedBackground(backgroundConfig: BackgroundConfig | null | undefined) {
  return backgroundConfig
    ? {
        backgroundType: backgroundConfig.backgroundType,
        canvasMode: backgroundConfig.canvasMode ?? "auto",
        canvasWidth: backgroundConfig.canvasWidth ?? null,
        canvasHeight: backgroundConfig.canvasHeight ?? null,
        autoCanvasSourceId: backgroundConfig.autoCanvasSourceId ?? null,
        scale: backgroundConfig.scale,
      }
    : null;
}

export function getSavedKeystrokeDelaySec(): number {
  try {
    const raw = localStorage.getItem(KEYSTROKE_DELAY_KEY);
    if (raw === null) return DEFAULT_KEYSTROKE_DELAY_SEC;
    const n = Number(raw);
    if (!Number.isFinite(n)) return DEFAULT_KEYSTROKE_DELAY_SEC;
    return Math.max(-1, Math.min(1, n));
  } catch {
    return DEFAULT_KEYSTROKE_DELAY_SEC;
  }
}

export function getSavedKeystrokeLanguage(): KeystrokeLanguage {
  try {
    const raw = localStorage.getItem(KEYSTROKE_LANGUAGE_KEY);
    if (raw && (VALID_KEYSTROKE_LANGUAGES as readonly string[]).includes(raw)) {
      return raw as KeystrokeLanguage;
    }
  } catch {
    /* ignore */
  }
  return "en";
}

export function saveKeystrokeLanguage(lang: KeystrokeLanguage): void {
  try {
    localStorage.setItem(KEYSTROKE_LANGUAGE_KEY, lang);
  } catch {
    /* ignore */
  }
}

export function getSavedKeystrokeModePref(): "off" | "keyboard" | "keyboardMouse" {
  try {
    const raw = localStorage.getItem(KEYSTROKE_MODE_PREF_KEY);
    if (raw === "keyboard" || raw === "keyboardMouse" || raw === "off")
      return raw;
  } catch {
    /* ignore */
  }
  return "off";
}

export function getSavedKeystrokeOverlayPref(): {
  x: number;
  y: number;
  scale: number;
} {
  try {
    const raw = localStorage.getItem(KEYSTROKE_OVERLAY_PREF_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<{
        x: number;
        y: number;
        scale: number;
      }>;
      if (typeof p === "object" && p !== null) {
        return {
          x: typeof p.x === "number" ? p.x : 50,
          y: typeof p.y === "number" ? p.y : 100,
          scale: typeof p.scale === "number" ? p.scale : 1,
        };
      }
    }
  } catch {
    /* ignore */
  }
  return { x: 50, y: 100, scale: 1 };
}

export function normalizeCropRect(
  crop: Partial<CropRect> | null | undefined,
): CropRect | undefined {
  if (!crop) return undefined;
  const rawX =
    typeof crop.x === "number" && Number.isFinite(crop.x) ? crop.x : 0;
  const rawY =
    typeof crop.y === "number" && Number.isFinite(crop.y) ? crop.y : 0;
  const rawWidth =
    typeof crop.width === "number" && Number.isFinite(crop.width)
      ? crop.width
      : 1;
  const rawHeight =
    typeof crop.height === "number" && Number.isFinite(crop.height)
      ? crop.height
      : 1;

  const width = Math.max(MIN_CROP_SIZE, Math.min(1, rawWidth));
  const height = Math.max(MIN_CROP_SIZE, Math.min(1, rawHeight));
  const x = Math.max(0, Math.min(1 - width, rawX));
  const y = Math.max(0, Math.min(1 - height, rawY));

  if (width >= 0.999 && height >= 0.999 && x <= 0.001 && y <= 0.001) {
    return undefined;
  }
  return { x, y, width, height };
}

export function getSavedCropPref(): CropRect | undefined {
  try {
    const raw = localStorage.getItem(CROP_PREF_KEY);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<CropRect>;
    return normalizeCropRect(parsed);
  } catch {
    return undefined;
  }
}

export function saveCropPref(crop: CropRect | undefined): void {
  try {
    const normalized = normalizeCropRect(crop);
    if (!normalized) {
      localStorage.removeItem(CROP_PREF_KEY);
      return;
    }
    localStorage.setItem(CROP_PREF_KEY, JSON.stringify(normalized));
  } catch {
    // ignore persistence failures
  }
}

export function getSavedAutoZoomPref(): boolean {
  try {
    const raw = localStorage.getItem(AUTO_ZOOM_PREF_KEY);
    if (raw !== null) return raw === "1";
  } catch {
    /* ignore */
  }
  return true; // default ON for first-time users
}

export function saveAutoZoomPref(enabled: boolean): void {
  try {
    localStorage.setItem(AUTO_ZOOM_PREF_KEY, enabled ? "1" : "0");
  } catch {
    /* ignore */
  }
}

export function getSavedAutoZoomConfig(): AutoZoomConfig {
  try {
    const raw = localStorage.getItem(AUTO_ZOOM_CONFIG_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        followTightness: typeof parsed.followTightness === "number" ? parsed.followTightness : DEFAULT_AUTO_ZOOM_CONFIG.followTightness,
        zoomLevel: typeof parsed.zoomLevel === "number" ? parsed.zoomLevel : DEFAULT_AUTO_ZOOM_CONFIG.zoomLevel,
        speedSensitivity: typeof parsed.speedSensitivity === "number" ? parsed.speedSensitivity : DEFAULT_AUTO_ZOOM_CONFIG.speedSensitivity,
      };
    }
  } catch { /* ignore */ }
  return { ...DEFAULT_AUTO_ZOOM_CONFIG };
}

export function saveAutoZoomConfig(config: AutoZoomConfig): void {
  try {
    localStorage.setItem(AUTO_ZOOM_CONFIG_KEY, JSON.stringify(config));
  } catch { /* ignore */ }
}

export function getSavedSmartPointerPref(): boolean {
  try {
    const raw = localStorage.getItem(SMART_POINTER_PREF_KEY);
    if (raw !== null) return raw === "1";
  } catch {
    /* ignore */
  }
  return true; // default ON for first-time users
}

export function saveSmartPointerPref(enabled: boolean): void {
  try {
    localStorage.setItem(SMART_POINTER_PREF_KEY, enabled ? "1" : "0");
  } catch {
    /* ignore */
  }
}

export function getSavedExportFpsPref(): number {
  try {
    const raw = localStorage.getItem(EXPORT_FPS_PREF_KEY);
    if (raw === null) return DEFAULT_EXPORT_FPS;
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) return DEFAULT_EXPORT_FPS;
    const rounded = Math.round(parsed);
    if (rounded < MIN_EXPORT_FPS || rounded > MAX_EXPORT_FPS) {
      return DEFAULT_EXPORT_FPS;
    }
    return rounded;
  } catch {
    return DEFAULT_EXPORT_FPS;
  }
}
