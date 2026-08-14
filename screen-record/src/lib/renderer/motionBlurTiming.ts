import { getExportSourceStep } from "@/lib/exportFrameTimes";
import { clamp } from "@/lib/mathUtils";
import type { VideoSegment } from "@/types/video";

export interface MotionBlurTiming {
  zoomEnabled: boolean;
  panEnabled: boolean;
  cursorEnabled: boolean;
  zoomShutterSec: number;
  panShutterSec: number;
  cursorShutterSec: number;
  maxShutterSec: number;
  sampleCount: number;
}

export function resolveMotionBlurTiming(
  segment: VideoSegment,
  sourceTime: number,
  outputFrameRate: number,
  zoomStrength: number,
  panStrength: number,
  cursorStrength: number,
): MotionBlurTiming {
  const zoomFraction = clamp(zoomStrength / 100, 0, 1);
  const panFraction = clamp(panStrength / 100, 0, 1);
  const cursorFraction = clamp(cursorStrength / 100, 0, 1);
  const maxFraction = Math.max(zoomFraction, panFraction, cursorFraction);
  const sourceStep = getExportSourceStep(
    segment,
    sourceTime,
    outputFrameRate,
  );
  const zoomShutterSec = zoomFraction * sourceStep;
  const panShutterSec = panFraction * sourceStep;
  const cursorShutterSec = cursorFraction * sourceStep;

  return {
    zoomEnabled: zoomFraction > 0.0001,
    panEnabled: panFraction > 0.0001,
    cursorEnabled: cursorFraction > 0.0001,
    zoomShutterSec,
    panShutterSec,
    cursorShutterSec,
    maxShutterSec: Math.max(
      zoomShutterSec,
      panShutterSec,
      cursorShutterSec,
    ),
    sampleCount:
      maxFraction > 0.0001
        ? Math.max(2, Math.min(8, Math.ceil(maxFraction * 8)))
        : 1,
  };
}
