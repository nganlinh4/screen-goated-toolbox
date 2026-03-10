interface TimeRangeLike {
  startTime: number;
  endTime: number;
}

const HANDLE_PRIORITY_RADIUS_PX = 12;

export function getHandlePriorityThresholdTime(
  duration: number,
  trackWidth: number,
): number {
  if (duration <= 0 || trackWidth <= 0) return 0;
  return (HANDLE_PRIORITY_RADIUS_PX / trackWidth) * duration;
}

export function isTimeNearRangeBoundary(
  time: number,
  ranges: TimeRangeLike[],
  thresholdTime: number,
): boolean {
  if (thresholdTime <= 0) return false;
  return ranges.some(
    (range) =>
      Math.abs(time - range.startTime) <= thresholdTime ||
      Math.abs(time - range.endTime) <= thresholdTime,
  );
}
