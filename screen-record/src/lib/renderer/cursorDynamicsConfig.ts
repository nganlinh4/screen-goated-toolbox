import type { BackgroundConfig } from '@/types/video';

// Default pointer movement delay (seconds)
export const DEFAULT_CURSOR_OFFSET_SEC = 0;
export const DEFAULT_CURSOR_WIGGLE_STRENGTH = 0.30;
export const DEFAULT_CURSOR_WIGGLE_DAMPING = 0.55;
export const DEFAULT_CURSOR_WIGGLE_RESPONSE = 6.5;

export function getCursorMovementDelaySec(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorMovementDelay;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_OFFSET_SEC;
  return Math.max(-0.5, Math.min(0.5, raw));
}

export function getCursorSmoothness(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorSmoothness;
  if (raw === undefined || Number.isNaN(raw)) return 5;
  return Math.max(0, Math.min(10, raw));
}

export function getCursorShadowStrength(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorShadow;
  if (raw === undefined || Number.isNaN(raw)) return 35;
  return Math.max(0, Math.min(200, raw));
}

export function getCursorWiggleStrength(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorWiggleStrength;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_STRENGTH;
  return Math.max(0, Math.min(1, raw));
}

export function getCursorWiggleDamping(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorWiggleDamping;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_DAMPING;
  return Math.max(0.35, Math.min(0.98, raw));
}

export function getCursorWiggleResponse(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorWiggleResponse;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_RESPONSE;
  return Math.max(2, Math.min(12, raw));
}

export function getCursorTiltAngleRad(backgroundConfig?: BackgroundConfig | null): number {
  return (backgroundConfig?.cursorTiltAngle ?? -10) * (Math.PI / 180);
}

export function getCursorProcessingSignature(backgroundConfig?: BackgroundConfig | null): string {
  return [
    getCursorSmoothness(backgroundConfig).toFixed(2),
    getCursorWiggleStrength(backgroundConfig).toFixed(2),
    getCursorWiggleDamping(backgroundConfig).toFixed(2),
    getCursorWiggleResponse(backgroundConfig).toFixed(2),
    getCursorTiltAngleRad(backgroundConfig).toFixed(4),
  ].join('|');
}
