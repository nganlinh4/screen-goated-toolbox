import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useZoomKeyframes } from '@/hooks/useZoomKeyframes';
import type { VideoSegment } from '@/types/video';

const segment = {
  trimStart: 0,
  trimEnd: 10,
  zoomBlocks: [{ id: 'z1', startTime: 2, endTime: 6, zoomFactor: 1.5, positionX: 0.5, positionY: 0.5 }],
} as unknown as VideoSegment;

function renderZoomHook() {
  let current: VideoSegment | null = segment;
  const setSegment = vi.fn((next: VideoSegment | null) => {
    current = next;
    hook.rerender();
  });
  const video = { currentTime: 4 } as HTMLVideoElement;
  const hook = renderHook(() => useZoomKeyframes({
    segment: current,
    setSegment,
    videoRef: { current: video },
    currentTime: 4,
    isVideoReady: true,
    renderFrame: vi.fn(),
    activePanel: 'zoom',
    setActivePanel: vi.fn(),
  }));
  return { hook, setSegment, current: () => current };
}

it('keeps the final value of a fast zoom slider drag', () => {
  const { hook, current } = renderZoomHook();
  expect(hook.result.current.editingKeyframeId).toBe(0);

  // Slider onChange events arriving within a few milliseconds, then the pointer is released.
  act(() => hook.result.current.updateZoom({ zoomFactor: 2 }));
  act(() => hook.result.current.updateZoom({ zoomFactor: 2.6 }));
  act(() => hook.result.current.updateZoom({ positionX: 0.8 }));

  const block = current()?.zoomBlocks?.[0];
  expect(block?.zoomFactor).toBe(2.6);
  expect(block?.positionX).toBe(0.8);
});
