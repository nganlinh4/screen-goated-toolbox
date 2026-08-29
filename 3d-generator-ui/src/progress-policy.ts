export const MAX_IN_FLIGHT_PROGRESS = 0.94;
export const AUTOMATIC_SEGMENTATION_START = 0.72;

export type ProgressRange = {
  start: number;
  end: number;
};

export const DEFAULT_PROGRESS_RANGE: ProgressRange = {
  start: 0,
  end: MAX_IN_FLIGHT_PROGRESS,
};

export const GENERATION_WITH_SEGMENTATION_RANGE: ProgressRange = {
  start: 0,
  end: AUTOMATIC_SEGMENTATION_START,
};

export function automaticSegmentationRange(start: number): ProgressRange {
  return {
    start: Math.max(AUTOMATIC_SEGMENTATION_START, bounded(start)),
    end: MAX_IN_FLIGHT_PROGRESS,
  };
}

export function nextDisplayedProgress(
  previous: number,
  elapsedMs: number,
  estimatedTotalMs: number,
  reportedRatio: number,
  range: ProgressRange,
) {
  const estimate = Math.max(10_000, estimatedTotalMs);
  const curved = Math.min(
    MAX_IN_FLIGHT_PROGRESS,
    0.9 * (1 - Math.exp((-3 * Math.max(0, elapsedMs)) / estimate)),
  );
  const local = Math.max(curved, bounded(reportedRatio, MAX_IN_FLIGHT_PROGRESS));
  const start = bounded(range.start, MAX_IN_FLIGHT_PROGRESS);
  const end = Math.max(start, bounded(range.end, MAX_IN_FLIGHT_PROGRESS));
  const mapped = start + (end - start) * (local / MAX_IN_FLIGHT_PROGRESS);
  return Math.max(bounded(previous, MAX_IN_FLIGHT_PROGRESS), mapped);
}

function bounded(value: number, maximum = 1) {
  return Number.isFinite(value) ? Math.max(0, Math.min(maximum, value)) : 0;
}
