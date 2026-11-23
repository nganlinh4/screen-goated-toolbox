import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useTimelineDrag } from '@/components/timeline/useTimelineDrag';
import type { VideoSegment } from '@/types/video';

const segment = {
  trimStart: 0,
  trimEnd: 10,
  zoomBlocks: [],
  textSegments: [],
  subtitleSegments: [{ id: 'a', startTime: 1, endTime: 2, text: 'Hello', style: {} }],
} as unknown as VideoSegment;

beforeEach(() => {
  // Animation frames never fire, as when the pointer is released right after a move.
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
});

afterEach(() => vi.restoreAllMocks());

it('applies the last subtitle drag frame inside the drag undo batch', () => {
  const events: string[] = [];
  const timeline = { getBoundingClientRect: () => ({ left: 0, width: 1000 }) } as HTMLDivElement;
  const noop = vi.fn();
  const { result } = renderHook(() => useTimelineDrag({
    duration: 10,
    segment,
    timelineRef: { current: timeline },
    videoRef: { current: null },
    setCurrentTime: noop,
    setSegment: () => events.push('setSegment'),
    setEditingKeyframeId: noop,
    setEditingTextId: noop,
    setActivePanel: noop,
    selectedTextIds: [],
    selectedSubtitleIds: [],
    beginBatch: () => events.push('beginBatch'),
    commitBatch: () => events.push('commitBatch'),
  }));

  act(() => result.current.handleSubtitleDragStart('a', 'end'));
  act(() => result.current.handleMouseMove({ clientX: 500 } as React.PointerEvent<HTMLDivElement>));
  act(() => result.current.handleMouseUp());

  expect(events).toEqual(['beginBatch', 'setSegment', 'commitBatch']);
});
