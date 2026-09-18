import { createPersistedSetting } from "./persistedState";
import { DEFAULT_WEBCAM_CONFIG } from "./webcam";
import type { AudioGainPoint, WebcamConfig } from "@/types/video";

const cameraSetting = createPersistedSetting<WebcamConfig | null>("screen-record-camera-default-v1", {
  parse: (raw) => {
    if (!raw) return null;
    const value = JSON.parse(raw);
    if (!value || typeof value !== "object") return null;
    const defaults = DEFAULT_WEBCAM_CONFIG;
    for (const key of Object.keys(defaults) as (keyof WebcamConfig)[]) {
      if (typeof value[key] !== typeof defaults[key]) return null;
      if (typeof value[key] === "number" && !Number.isFinite(value[key])) return null;
    }
    if (!["topLeft", "topRight", "bottomLeft", "bottomRight"].includes(value.position)) return null;
    return value as WebcamConfig;
  },
  serialize: JSON.stringify,
  fallback: null,
});

export function getNewRecordingCameraConfig(available: boolean): WebcamConfig {
  const saved = cameraSetting.getInitial();
  return { ...DEFAULT_WEBCAM_CONFIG, ...saved, visible: available && (saved?.visible ?? true) };
}

export function saveCameraDefaults(config: WebcamConfig) {
  cameraSetting.persist(config);
}

type AudioSource = "device" | "mic";
const audioSetting = (source: AudioSource) => createPersistedSetting<number | null>(
  `screen-record-${source}-volume-default-v1`,
  {
    parse: (raw) => {
      if (raw === null) return null;
      const value = Number(raw);
      return Number.isFinite(value) && value >= 0 && value <= 1 ? value : null;
    },
    serialize: String,
    fallback: null,
  },
);

export function getDefaultAudioVolume(source: AudioSource, fallback: number): number {
  return audioSetting(source).getInitial() ?? fallback;
}

// Only a uniform level is a reusable default; never carry automation into new media.
export function saveUniformAudioDefault(source: AudioSource, points: AudioGainPoint[]) {
  const volume = points[0]?.volume;
  if (!Number.isFinite(volume) || volume < 0 || volume > 1) return;
  if (!points.every((point) => point.volume === volume)) return;
  audioSetting(source).persist(volume);
}

export const volumeViewSetting = createPersistedSetting<boolean>("screen-record-volume-view-v1", {
  parse: (raw) => raw === "1",
  serialize: (value) => value ? "1" : "0",
  fallback: false,
});
