import {
  CropRect,
  AutoZoomConfig,
  DEFAULT_AUTO_ZOOM_CONFIG,
} from "@/types/video";
import { createPersistedSetting } from "@/lib/persistedState";

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

const keystrokeDelaySetting = createPersistedSetting<number>(KEYSTROKE_DELAY_KEY, {
  parse: (raw) => {
    if (raw === null) return DEFAULT_KEYSTROKE_DELAY_SEC;
    const n = Number(raw);
    if (!Number.isFinite(n)) return DEFAULT_KEYSTROKE_DELAY_SEC;
    return Math.max(-1, Math.min(1, n));
  },
  serialize: (value) => String(value),
  fallback: DEFAULT_KEYSTROKE_DELAY_SEC,
});

const keystrokeLanguageSetting = createPersistedSetting<KeystrokeLanguage>(KEYSTROKE_LANGUAGE_KEY, {
  parse: (raw) => {
    if (raw && (VALID_KEYSTROKE_LANGUAGES as readonly string[]).includes(raw)) {
      return raw as KeystrokeLanguage;
    }
    return "en";
  },
  serialize: (value) => value,
  fallback: "en",
});

const keystrokeModeSetting = createPersistedSetting<"off" | "keyboard" | "keyboardMouse">(
  KEYSTROKE_MODE_PREF_KEY,
  {
    parse: (raw) => {
      if (raw === "keyboard" || raw === "keyboardMouse" || raw === "off") return raw;
      return "off";
    },
    serialize: (value) => value,
    fallback: "off",
  },
);

const keystrokeOverlaySetting = createPersistedSetting<{ x: number; y: number; scale: number }>(
  KEYSTROKE_OVERLAY_PREF_KEY,
  {
    parse: (raw) => {
      if (raw) {
        const p = JSON.parse(raw) as Partial<{ x: number; y: number; scale: number }>;
        if (typeof p === "object" && p !== null) {
          return {
            x: typeof p.x === "number" ? p.x : 50,
            y: typeof p.y === "number" ? p.y : 100,
            scale: typeof p.scale === "number" ? p.scale : 1,
          };
        }
      }
      return { x: 50, y: 100, scale: 1 };
    },
    serialize: (value) => JSON.stringify(value),
    fallback: { x: 50, y: 100, scale: 1 },
  },
);

export function getSavedKeystrokeDelaySec(): number {
  return keystrokeDelaySetting.getInitial();
}

export function getSavedKeystrokeLanguage(): KeystrokeLanguage {
  return keystrokeLanguageSetting.getInitial();
}

export function saveKeystrokeLanguage(lang: KeystrokeLanguage): void {
  keystrokeLanguageSetting.persist(lang);
}

export function getSavedKeystrokeModePref(): "off" | "keyboard" | "keyboardMouse" {
  return keystrokeModeSetting.getInitial();
}

export function getSavedKeystrokeOverlayPref(): {
  x: number;
  y: number;
  scale: number;
} {
  return keystrokeOverlaySetting.getInitial();
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

const cropPrefSetting = createPersistedSetting<CropRect | undefined>(CROP_PREF_KEY, {
  parse: (raw) => {
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<CropRect>;
    return normalizeCropRect(parsed);
  },
  serialize: (crop) => {
    const normalized = normalizeCropRect(crop);
    return normalized ? JSON.stringify(normalized) : null;
  },
  fallback: undefined,
});

const autoZoomPrefSetting = createPersistedSetting<boolean>(AUTO_ZOOM_PREF_KEY, {
  // default ON for first-time users
  parse: (raw) => (raw !== null ? raw === "1" : true),
  serialize: (enabled) => (enabled ? "1" : "0"),
  fallback: true,
});

const autoZoomConfigSetting = createPersistedSetting<AutoZoomConfig>(AUTO_ZOOM_CONFIG_KEY, {
  parse: (raw) => {
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        followTightness: typeof parsed.followTightness === "number" ? parsed.followTightness : DEFAULT_AUTO_ZOOM_CONFIG.followTightness,
        zoomLevel: typeof parsed.zoomLevel === "number" ? parsed.zoomLevel : DEFAULT_AUTO_ZOOM_CONFIG.zoomLevel,
        speedSensitivity: typeof parsed.speedSensitivity === "number" ? parsed.speedSensitivity : DEFAULT_AUTO_ZOOM_CONFIG.speedSensitivity,
      };
    }
    return { ...DEFAULT_AUTO_ZOOM_CONFIG };
  },
  serialize: (config) => JSON.stringify(config),
  fallback: { ...DEFAULT_AUTO_ZOOM_CONFIG },
});

const smartPointerPrefSetting = createPersistedSetting<boolean>(SMART_POINTER_PREF_KEY, {
  // default ON for first-time users
  parse: (raw) => (raw !== null ? raw === "1" : true),
  serialize: (enabled) => (enabled ? "1" : "0"),
  fallback: true,
});

const exportFpsPrefSetting = createPersistedSetting<number>(EXPORT_FPS_PREF_KEY, {
  parse: (raw) => {
    if (raw === null) return DEFAULT_EXPORT_FPS;
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) return DEFAULT_EXPORT_FPS;
    const rounded = Math.round(parsed);
    if (rounded < MIN_EXPORT_FPS || rounded > MAX_EXPORT_FPS) {
      return DEFAULT_EXPORT_FPS;
    }
    return rounded;
  },
  serialize: (value) => String(value),
  fallback: DEFAULT_EXPORT_FPS,
});

export function getSavedCropPref(): CropRect | undefined {
  return cropPrefSetting.getInitial();
}

export function saveCropPref(crop: CropRect | undefined): void {
  cropPrefSetting.persist(crop);
}

export function getSavedAutoZoomPref(): boolean {
  return autoZoomPrefSetting.getInitial();
}

export function saveAutoZoomPref(enabled: boolean): void {
  autoZoomPrefSetting.persist(enabled);
}

export function getSavedAutoZoomConfig(): AutoZoomConfig {
  return autoZoomConfigSetting.getInitial();
}

export function saveAutoZoomConfig(config: AutoZoomConfig): void {
  autoZoomConfigSetting.persist(config);
}

export function getSavedSmartPointerPref(): boolean {
  return smartPointerPrefSetting.getInitial();
}

export function saveSmartPointerPref(enabled: boolean): void {
  smartPointerPrefSetting.persist(enabled);
}

export function getSavedExportFpsPref(): number {
  return exportFpsPrefSetting.getInitial();
}
