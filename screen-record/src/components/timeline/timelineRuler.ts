import type { SpeedPoint } from "@/types/video";
import { videoTimeToWallClock } from "@/lib/exportEstimator";

const MAJOR_TICK_TARGET_PX = 80;
const RULER_STEP_OPTIONS_SEC = [
  0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600,
];
const TICK_EPSILON = 0.0001;

export interface TimelineRulerTick {
  time: number;
  leftPct: number;
  label: string;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function getMajorStep(duration: number, widthPx: number): number {
  if (duration <= 0 || widthPx <= 0) return 1;
  const rawStep = (MAJOR_TICK_TARGET_PX / widthPx) * duration;
  const configuredStep = RULER_STEP_OPTIONS_SEC.find((step) => step >= rawStep);
  if (configuredStep) return configuredStep;

  const largestStep =
    RULER_STEP_OPTIONS_SEC[RULER_STEP_OPTIONS_SEC.length - 1] ?? 3600;
  return Math.ceil(rawStep / largestStep) * largestStep;
}

function formatTimelineLabel(seconds: number, majorStep: number): string {
  const safeSeconds = Math.max(0, seconds);
  const decimals = majorStep < 1 ? 2 : majorStep < 10 ? 1 : 0;
  const precision = 10 ** decimals;
  const roundedUnits = Math.round(safeSeconds * precision);
  const minutes = Math.floor(roundedUnits / (60 * precision));
  const secondUnits = roundedUnits - minutes * 60 * precision;
  const wholeSeconds = Math.floor(secondUnits / precision);
  const wholePart = wholeSeconds.toString().padStart(2, "0");

  if (decimals === 0) return `${minutes}:${wholePart}`;

  const fractionUnits = secondUnits - wholeSeconds * precision;
  return `${minutes}:${wholePart}.${fractionUnits
    .toString()
    .padStart(decimals, "0")}`;
}

export function buildTimelineRulerTicks({
  duration,
  widthPx,
  speedPoints,
}: {
  duration: number;
  widthPx: number;
  speedPoints?: SpeedPoint[];
}): TimelineRulerTick[] {
  if (duration <= 0 || widthPx <= 0) return [];

  const majorStep = getMajorStep(duration, widthPx);
  const ticks: TimelineRulerTick[] = [];

  for (let time = 0; time <= duration + TICK_EPSILON; time += majorStep) {
    const clampedTime = clamp(time, 0, duration);
    const displayTime =
      speedPoints?.length && duration > 0
        ? videoTimeToWallClock(clampedTime, speedPoints)
        : clampedTime;

    ticks.push({
      time: clampedTime,
      leftPct: duration > 0 ? (clampedTime / duration) * 100 : 0,
      label: formatTimelineLabel(displayTime, majorStep),
    });
  }

  const lastTick = ticks[ticks.length - 1];
  if (!lastTick || Math.abs(lastTick.time - duration) > TICK_EPSILON) {
    const displayTime =
      speedPoints?.length && duration > 0
        ? videoTimeToWallClock(duration, speedPoints)
        : duration;
    ticks.push({
      time: duration,
      leftPct: 100,
      label: formatTimelineLabel(displayTime, majorStep),
    });
  }

  return ticks;
}
