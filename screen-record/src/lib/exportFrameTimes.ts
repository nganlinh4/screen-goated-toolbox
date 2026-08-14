import type { TrimSegment, VideoSegment } from "@/types/video";
import { clamp } from "@/lib/mathUtils";
import { getSpeedAtTime } from "@/lib/speedCurve";

const FRAME_TIME_EPSILON_SEC = 1e-9;
// Mirrors the native staging stream limit so oversized jobs fail before IPC.
const MAX_FRONTEND_EXPORT_FRAMES = 2_000_000;

function getExportTrimSegments(segment: VideoSegment): readonly TrimSegment[] {
  if (segment.trimSegments?.length) return segment.trimSegments;
  return [
    {
      id: "export-range",
      startTime: segment.trimStart,
      endTime: segment.trimEnd,
    },
  ];
}

export function getExportSourceStep(
  segment: VideoSegment,
  sourceTime: number,
  fps: number,
): number {
  return (
    clamp(getSpeedAtTime(sourceTime, segment.speedPoints), 0.1, 16) /
    Math.max(1, fps)
  );
}

export function visitExportSourceTimes(
  segment: VideoSegment,
  fps: number,
  visitor: (sourceTime: number, frameIndex: number) => void,
): number {
  let frameCount = 0;
  for (const { sourceTime, frameIndex } of iterateExportSourceTimes(segment, fps)) {
    visitor(sourceTime, frameIndex);
    frameCount = frameIndex + 1;
  }
  return frameCount;
}

export function* iterateExportSourceTimes(
  segment: VideoSegment,
  fps: number,
): Generator<{ sourceTime: number; frameIndex: number }> {
  const trimSegments = getExportTrimSegments(segment);
  if (trimSegments.length === 0) return;

  let trimIndex = 0;
  let sourceTime = trimSegments[0].startTime;
  const endTime = trimSegments[trimSegments.length - 1].endTime;
  let frameIndex = 0;

  while (sourceTime < endTime - FRAME_TIME_EPSILON_SEC) {
    while (
      trimIndex < trimSegments.length &&
      sourceTime >= trimSegments[trimIndex].endTime
    ) {
      trimIndex += 1;
      if (trimIndex < trimSegments.length) {
        sourceTime = trimSegments[trimIndex].startTime;
      }
    }
    if (trimIndex >= trimSegments.length) break;
    if (frameIndex >= MAX_FRONTEND_EXPORT_FRAMES) {
      throw new Error(
        "This export contains too many frames to prepare safely. Shorten the timeline or increase its playback speed.",
      );
    }

    yield { sourceTime, frameIndex };
    frameIndex += 1;
    sourceTime += getExportSourceStep(segment, sourceTime, fps);
  }
}

export function buildExportSourceTimes(
  segment: VideoSegment,
  fps: number,
): number[] {
  const sourceTimes: number[] = [];
  visitExportSourceTimes(segment, fps, (sourceTime) => {
    sourceTimes.push(sourceTime);
  });
  return sourceTimes;
}
