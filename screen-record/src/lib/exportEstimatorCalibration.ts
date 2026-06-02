export const ESTIMATE_CALIBRATION_STORAGE_KEY = 'sr-export-estimate-calibration-v1';
export const MAX_CALIBRATION_SAMPLES = 24;
export const MAX_CALIBRATION_BUCKETS = 48;

export interface ExportEstimateCalibration {
  ratio: number;
  samples: number;
  updatedAt: number;
}

export interface ExportEstimateCalibrationStore {
  version: 2;
  global: ExportEstimateCalibration;
  buckets: Record<string, ExportEstimateCalibration>;
}

export interface ExportEstimateCalibrationSnapshot {
  ratio: number;
  samples: number;
  profileKey?: string;
  globalRatio?: number;
  globalSamples?: number;
  bucketRatio?: number;
  bucketSamples?: number;
}

function clampCalibrationValue(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function createDefaultCalibration(): ExportEstimateCalibration {
  return {
    ratio: 1,
    samples: 0,
    updatedAt: Date.now()
  };
}

export function normalizeCalibrationEntry(entry: Partial<ExportEstimateCalibration> | undefined): ExportEstimateCalibration {
  const fallback = createDefaultCalibration();
  if (!entry) return fallback;
  return {
    ratio: clampCalibrationValue(Number(entry.ratio) || 1, 0.5, 1.5),
    samples: clampCalibrationValue(Math.round(Number(entry.samples) || 0), 0, MAX_CALIBRATION_SAMPLES),
    updatedAt: Number(entry.updatedAt) || fallback.updatedAt
  };
}

export function readEstimateCalibrationStore(): ExportEstimateCalibrationStore {
  const fallbackGlobal = createDefaultCalibration();
  const fallbackStore: ExportEstimateCalibrationStore = {
    version: 2,
    global: fallbackGlobal,
    buckets: {}
  };
  try {
    if (typeof window === 'undefined' || !window.localStorage) return fallbackStore;
    const raw = window.localStorage.getItem(ESTIMATE_CALIBRATION_STORAGE_KEY);
    if (!raw) return fallbackStore;
    const parsed = JSON.parse(raw) as
      | (Partial<ExportEstimateCalibrationStore> & { version?: number })
      | Partial<ExportEstimateCalibration>;

    if (!('version' in parsed)) {
      return {
        version: 2,
        global: normalizeCalibrationEntry(parsed as Partial<ExportEstimateCalibration>),
        buckets: {}
      };
    }

    const global = normalizeCalibrationEntry(parsed.global);
    const rawBuckets = parsed.buckets && typeof parsed.buckets === 'object'
      ? (parsed.buckets as Record<string, Partial<ExportEstimateCalibration>>)
      : {};
    const bucketEntries = Object.entries(rawBuckets)
      .map(([key, value]) => [key, normalizeCalibrationEntry(value)] as const)
      .sort((a, b) => b[1].updatedAt - a[1].updatedAt)
      .slice(0, MAX_CALIBRATION_BUCKETS);

    const buckets: Record<string, ExportEstimateCalibration> = {};
    for (const [key, value] of bucketEntries) {
      buckets[key] = value;
    }

    return { version: 2, global, buckets };
  } catch {
    return fallbackStore;
  }
}

export function writeEstimateCalibrationStore(store: ExportEstimateCalibrationStore) {
  try {
    if (typeof window === 'undefined' || !window.localStorage) return;
    window.localStorage.setItem(ESTIMATE_CALIBRATION_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // Ignore localStorage failures (private mode, quota, etc.).
  }
}

export function blendCalibration(previous: ExportEstimateCalibration, observedRatio: number): ExportEstimateCalibration {
  const weight = previous.samples < 5 ? 0.35 : 0.2;
  return {
    ratio: clampCalibrationValue((previous.ratio * (1 - weight)) + (observedRatio * weight), 0.5, 1.5),
    samples: Math.min(MAX_CALIBRATION_SAMPLES, previous.samples + 1),
    updatedAt: Date.now()
  };
}

export function getExportEstimateCalibration(profileKey?: string): ExportEstimateCalibrationSnapshot {
  const store = readEstimateCalibrationStore();
  const global = store.global;
  const bucket = profileKey ? store.buckets[profileKey] : undefined;

  if (!bucket || bucket.samples <= 0) {
    const bootstrappedRatio = global.samples > 0
      ? clampCalibrationValue(1 + ((global.ratio - 1) * 0.15), 0.75, 1.25)
      : 1;
    return {
      ratio: bootstrappedRatio,
      samples: 0,
      profileKey,
      globalRatio: global.ratio,
      globalSamples: global.samples
    };
  }

  const bucketWeight = bucket.samples >= 2
    ? 1
    : 0.5;
  return {
    ratio: clampCalibrationValue((global.ratio * (1 - bucketWeight)) + (bucket.ratio * bucketWeight), 0.5, 1.5),
    samples: Math.min(MAX_CALIBRATION_SAMPLES, global.samples + bucket.samples),
    profileKey,
    globalRatio: global.ratio,
    globalSamples: global.samples,
    bucketRatio: bucket.ratio,
    bucketSamples: bucket.samples
  };
}

export function recordExportEstimateResult(expectedBytes: number, actualBytes: number, profileKey?: string) {
  if (expectedBytes <= 0 || actualBytes <= 0) return;
  const observedRatio = clampCalibrationValue(actualBytes / expectedBytes, 0.35, 2.5);
  const store = readEstimateCalibrationStore();
  store.global = blendCalibration(store.global, observedRatio);

  if (profileKey) {
    const previousBucket = store.buckets[profileKey] ?? createDefaultCalibration();
    store.buckets[profileKey] = blendCalibration(previousBucket, observedRatio);

    const trimmedEntries = Object.entries(store.buckets)
      .sort((a, b) => b[1].updatedAt - a[1].updatedAt)
      .slice(0, MAX_CALIBRATION_BUCKETS);
    store.buckets = {};
    for (const [key, value] of trimmedEntries) {
      store.buckets[key] = value;
    }
  }

  writeEstimateCalibrationStore(store);
}
