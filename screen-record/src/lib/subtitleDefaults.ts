import type { SubtitleSegment } from '@/types/video';
import {
  DEFAULT_TEXT_ANIMATION,
  DEFAULT_TEXT_LINE_HEIGHT,
  DEFAULT_TEXT_SHADOW,
  DEFAULT_TEXT_STROKE,
  DEFAULT_TEXT_WRAP,
  defaultTextBackground,
} from '@/lib/textStyleDefaults';

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
    lineHeight: DEFAULT_TEXT_LINE_HEIGHT,
    wrap: { ...DEFAULT_TEXT_WRAP },
    stroke: { ...DEFAULT_TEXT_STROKE },
    shadow: { ...DEFAULT_TEXT_SHADOW },
    animation: { ...DEFAULT_TEXT_ANIMATION },
    background: defaultTextBackground({ opacity: 0.65 }),
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
