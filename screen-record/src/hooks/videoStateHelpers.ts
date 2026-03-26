import { MousePosition } from "@/types/video";

const TRAILING_MOUSE_SAMPLE_EPSILON_SEC = 1 / 240;

export function hasValidCaptureDimensions(
  position: MousePosition | undefined,
): boolean {
  return (
    typeof position?.captureWidth === "number" &&
    Number.isFinite(position.captureWidth) &&
    position.captureWidth > 1 &&
    typeof position?.captureHeight === "number" &&
    Number.isFinite(position.captureHeight) &&
    position.captureHeight > 1
  );
}

export function stabilizeMousePositionsForTimeline(
  positions: MousePosition[],
  timelineDuration: number,
): MousePosition[] {
  if (positions.length === 0) return positions;

  let changed = false;
  let lastValidDims: { width: number; height: number } | null = null;

  const stabilized = positions.map((position) => {
    if (hasValidCaptureDimensions(position)) {
      lastValidDims = {
        width: position.captureWidth!,
        height: position.captureHeight!,
      };
      return position;
    }
    if (!lastValidDims) {
      return position;
    }
    changed = true;
    return {
      ...position,
      captureWidth: lastValidDims.width,
      captureHeight: lastValidDims.height,
    };
  });

  const last = stabilized[stabilized.length - 1];
  if (
    Number.isFinite(timelineDuration) &&
    timelineDuration > 0 &&
    timelineDuration - last.timestamp > TRAILING_MOUSE_SAMPLE_EPSILON_SEC
  ) {
    changed = true;
    stabilized.push({
      ...last,
      timestamp: timelineDuration,
    });
  }

  return changed ? stabilized : positions;
}
