import { invoke } from "@/lib/ipc";
import { getLruCacheValue, setLruCacheValue } from "@/lib/boundedCache";

export interface AudioWaveformBin {
  min: number;
  max: number;
}

export interface AudioWaveformResponse {
  bins: AudioWaveformBin[];
  sourceDurationSec: number;
}

const waveformCache = new Map<string, AudioWaveformResponse>();
const waveformInflight = new Map<string, Promise<AudioWaveformResponse>>();
const TARGET_BIN_BUCKET_SIZE = 64;
export const MAX_WAVEFORM_CACHE_ENTRIES = 32;
export const MAX_WAVEFORM_CONCURRENT_REQUESTS = 4;
export const MAX_WAVEFORM_INFLIGHT_ENTRIES = 64;
let activeWaveformRequests = 0;
const waveformAdmissionQueue: Array<() => void> = [];

export function clearAudioWaveformCache(): void {
  waveformCache.clear();
}

async function runWithWaveformAdmission<T>(
  operation: () => Promise<T>,
): Promise<T> {
  if (activeWaveformRequests >= MAX_WAVEFORM_CONCURRENT_REQUESTS) {
    await new Promise<void>((resolve) => waveformAdmissionQueue.push(resolve));
  }
  activeWaveformRequests += 1;
  try {
    return await operation();
  } finally {
    activeWaveformRequests = Math.max(0, activeWaveformRequests - 1);
    waveformAdmissionQueue.shift()?.();
  }
}

function getWaveformCacheKey(path: string, targetBins: number) {
  return JSON.stringify({
    path: path.trim(),
    targetBins,
  });
}

export async function getAudioWaveform(
  path: string,
  targetBins: number,
): Promise<AudioWaveformResponse> {
  const trimmedPath = path.trim();
  if (!trimmedPath) {
    return { bins: [], sourceDurationSec: 0 };
  }

  const normalizedTargetBins = Math.max(
    16,
    Math.min(
      4096,
      Math.ceil(Math.round(targetBins) / TARGET_BIN_BUCKET_SIZE) *
        TARGET_BIN_BUCKET_SIZE,
    ),
  );
  const cacheKey = getWaveformCacheKey(trimmedPath, normalizedTargetBins);
  const cached = getLruCacheValue(waveformCache, cacheKey);
  if (cached) {
    return cached;
  }

  const inflight = waveformInflight.get(cacheKey);
  if (inflight) {
    return inflight;
  }

  if (waveformInflight.size >= MAX_WAVEFORM_INFLIGHT_ENTRIES) {
    throw new Error("Too many audio waveforms are waiting to load");
  }

  const request = runWithWaveformAdmission(() =>
    invoke<AudioWaveformResponse>("get_audio_waveform", {
      path: trimmedPath,
      targetBins: normalizedTargetBins,
    }),
  )
    .then((response) => {
      const normalized: AudioWaveformResponse = {
        bins: Array.isArray(response?.bins)
          ? response.bins.map((bin) => ({
              min: Number.isFinite(bin?.min) ? bin.min : 0,
              max: Number.isFinite(bin?.max) ? bin.max : 0,
            }))
          : [],
        sourceDurationSec:
          Number.isFinite(response?.sourceDurationSec) &&
          (response?.sourceDurationSec ?? 0) > 0
            ? response.sourceDurationSec
            : 0,
      };
      setLruCacheValue(
        waveformCache,
        cacheKey,
        normalized,
        MAX_WAVEFORM_CACHE_ENTRIES,
      );
      return normalized;
    })
    .finally(() => {
      if (waveformInflight.get(cacheKey) === request) {
        waveformInflight.delete(cacheKey);
      }
    });

  waveformInflight.set(cacheKey, request);
  return request;
}
