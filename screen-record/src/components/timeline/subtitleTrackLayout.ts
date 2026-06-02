import type { CSSProperties } from 'react';

export const DENSE_SUBTITLE_COUNT = 260;
export const MIN_INTERACTIVE_SUBTITLE_PX = 7;
export const TRANSLATION_CHUNK_COLORS = [
  '#2563eb',
  '#0f9f8d',
  '#d97706',
  '#8b5cf6',
  '#e11d48',
  '#0891b2',
  '#65a30d',
  '#f97316',
];

export function rangesOverlap(
  a: { startTime: number; endTime: number },
  b: { startTime: number; endTime: number },
) {
  return a.startTime < b.endTime && b.startTime < a.endTime;
}

export function getOverlapRange(
  segment: { startTime: number; endTime: number },
  elevated: { startTime: number; endTime: number } | null,
) {
  if (!elevated || !rangesOverlap(segment, elevated)) return null;
  const start = Math.max(segment.startTime, elevated.startTime);
  const end = Math.min(segment.endTime, elevated.endTime);
  const duration = Math.max(segment.endTime - segment.startTime, 0.0001);
  return {
    startPct: ((start - segment.startTime) / duration) * 100,
    endPct: ((end - segment.startTime) / duration) * 100,
  };
}

export function buildContentMaskStyle(
  ranges: Array<{ startPct: number; endPct: number }>,
): CSSProperties | undefined {
  if (ranges.length === 0) return undefined;
  const merged = [...ranges]
    .sort((a, b) => a.startPct - b.startPct)
    .reduce<Array<{ startPct: number; endPct: number }>>((acc, range) => {
      const startPct = Math.max(0, Math.min(100, range.startPct));
      const endPct = Math.max(startPct, Math.min(100, range.endPct));
      const last = acc[acc.length - 1];
      if (last && startPct <= last.endPct) {
        last.endPct = Math.max(last.endPct, endPct);
      } else if (endPct > startPct) {
        acc.push({ startPct, endPct });
      }
      return acc;
    }, []);
  if (merged.length === 0) return undefined;
  const stops: string[] = ['black 0%'];
  for (const range of merged) {
    stops.push(`black ${range.startPct}%`);
    stops.push(`transparent ${range.startPct}%`);
    stops.push(`transparent ${range.endPct}%`);
    stops.push(`black ${range.endPct}%`);
  }
  stops.push('black 100%');
  const maskImage = `linear-gradient(to right, ${stops.join(', ')})`;
  return { maskImage, WebkitMaskImage: maskImage };
}

export function isSegmentStackedAbove(
  index: number,
  rank: number,
  otherIndex: number,
  otherRank: number,
) {
  return otherRank > rank || (otherRank === rank && otherIndex > index);
}
