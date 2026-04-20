import { invoke } from "@/lib/ipc";

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

  const normalizedTargetBins = Math.max(16, Math.min(4096, Math.round(targetBins)));
  const cacheKey = getWaveformCacheKey(trimmedPath, normalizedTargetBins);
  const cached = waveformCache.get(cacheKey);
  if (cached) {
    return cached;
  }

  const inflight = waveformInflight.get(cacheKey);
  if (inflight) {
    return inflight;
  }

  const request = invoke<AudioWaveformResponse>("get_audio_waveform", {
    path: trimmedPath,
    targetBins: normalizedTargetBins,
  })
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
      waveformCache.set(cacheKey, normalized);
      waveformInflight.delete(cacheKey);
      return normalized;
    })
    .catch((error) => {
      waveformInflight.delete(cacheKey);
      throw error;
    });

  waveformInflight.set(cacheKey, request);
  return request;
}
