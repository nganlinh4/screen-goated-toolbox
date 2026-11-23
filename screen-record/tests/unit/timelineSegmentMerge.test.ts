import { describe, expect, it } from 'vitest';
import { mergeTextSegmentsInRange } from '@/lib/timelineSegmentSelection';
import type { TextSegment } from '@/types/video';

const segment = (id: string, startTime: number, endTime: number, text: string) =>
  ({ id, startTime, endTime, text }) as TextSegment;

describe('mergeTextSegmentsInRange', () => {
  it('keeps the latest end time when a merged segment nests inside another', () => {
    const { merged } = mergeTextSegmentsInRange(
      [segment('a', 0, 10, 'long'), segment('b', 2, 4, 'short')],
      { startTime: 0, endTime: 10 },
      ' ',
    );
    expect(merged).toMatchObject({ id: 'a', startTime: 0, endTime: 10, text: 'long short' });
  });

  it('spans from the first start to the last end for back-to-back segments', () => {
    const { merged, segments } = mergeTextSegmentsInRange(
      [segment('a', 0, 2, 'one'), segment('b', 2, 5, 'two'), segment('c', 8, 9, 'out')],
      { startTime: 0, endTime: 5 },
      ' ',
    );
    expect(merged).toMatchObject({ startTime: 0, endTime: 5, text: 'one two' });
    expect(segments.map((item) => item.id)).toEqual(['a', 'c']);
  });
});
