/** Fit the narrower field of view; portrait must not crop a wide model. */
export function fitDistance(radius: number, verticalFovDegrees: number, aspect: number): number {
  const aperture = Math.tan(verticalFovDegrees * Math.PI / 360) * Math.min(1, aspect);
  return Math.max(1.7, radius / aperture * 1.12);
}

/** Preserve apparent size and orbit direction when the viewport changes. */
export function resizeDistanceRatio(previousAspect: number, nextAspect: number): number {
  return Math.min(1, previousAspect) / Math.min(1, nextAspect);
}
