import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useTimelineSegmentActions } from '@/components/timeline/useTimelineSegmentActions';
import { duplicateSubtitleAcrossTracks } from '@/lib/subtitleTrackMutations';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import type { SubtitleSegment, TextSegment, VideoSegment } from '@/types/video';

const cue = (id: string, startTime: number, endTime: number) =>
  ({ id, startTime, endTime, text: id, style: {} }) as unknown as SubtitleSegment;
const text = (id: string, startTime: number, endTime: number) =>
  ({ id, startTime, endTime, text: id, style: {} }) as unknown as TextSegment;

const spans = (items: readonly { startTime: number; endTime: number }[]) =>
  items.map(({ startTime, endTime }) => [startTime, endTime]).sort((a, b) => a[0] - b[0]);

describe('duplicate placement', () => {
  it('does not stack a subtitle copy on a cue that starts where the source ends', () => {
    const segment = {
      trimStart: 0, trimEnd: 20, textSegments: [],
      subtitleSegments: [cue('a', 0, 2), cue('b', 2, 4), cue('c', 10, 12)],
    } as unknown as VideoSegment;
    const result = duplicateSubtitleAcrossTracks(segment, 'a', 20);
    expect(result.newSubtitleId).toBeNull();
    expect(spans(getVisibleSubtitleSegments(result.segment))).toEqual([[0, 2], [2, 4], [10, 12]]);
  });

  it('still places a subtitle copy in free space after the source', () => {
    const segment = {
      trimStart: 0, trimEnd: 20, textSegments: [],
      subtitleSegments: [cue('a', 0, 2), cue('c', 10, 12)],
    } as unknown as VideoSegment;
    const result = duplicateSubtitleAcrossTracks(segment, 'a', 20);
    expect(spans(getVisibleSubtitleSegments(result.segment))).toEqual([[0, 2], [2, 4], [10, 12]]);
  });

  it('does not stack a text copy on a text segment that starts where the source ends', () => {
    const segment = {
      trimStart: 0, trimEnd: 20, subtitleSegments: [],
      textSegments: [text('a', 0, 2), text('b', 2, 4)],
    } as unknown as VideoSegment;
    const setSegment = vi.fn();
    const { result } = renderHook(() => useTimelineSegmentActions({
      duration: 20, segment, setSegment, setCurrentTime: vi.fn(), videoRef: { current: null },
      beginBatch: vi.fn(), commitBatch: vi.fn(),
    }));
    result.current.handleDuplicateText('a');
    expect(setSegment).not.toHaveBeenCalled();
  });
});
