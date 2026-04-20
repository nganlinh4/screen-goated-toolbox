import type { SubtitleSegment } from '@/types/video';

export function defaultSubtitleStyle() {
  return {
    fontSize: 54,
    color: '#ffffff',
    x: 50,
    y: 90,
    fontVariations: { wght: 600, wdth: 100, slnt: 0, ROND: 0 },
    textAlign: 'center' as const,
    opacity: 1,
    letterSpacing: 0,
    background: {
      enabled: true,
      color: '#000000',
      opacity: 0.65,
      paddingX: 16,
      paddingY: 8,
      borderRadius: 32,
    },
  };
}

export function createManualSubtitleSegment(
  atTime: number,
  duration: number,
): SubtitleSegment {
  const segDur = 3;
  const startTime = Math.max(0, atTime - segDur / 2);
  return {
    id: crypto.randomUUID(),
    startTime,
    endTime: Math.min(startTime + segDur, duration),
    text: 'New Subtitle',
    style: defaultSubtitleStyle(),
  };
}
