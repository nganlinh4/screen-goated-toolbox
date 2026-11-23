import { describe, expect, it } from 'vitest';
import { handleMouseDown, type TextDragState } from '@/lib/renderer/overlayDragHandlers';
import type { SubtitleSegment, VideoSegment } from '@/types/video';

const style = { fontSize: 40, color: '#ffffff', x: 50, y: 80 };
const caption = (id: string, startTime: number, endTime: number, text: string) =>
  ({ id, startTime, endTime, text, style: { ...style } }) as unknown as SubtitleSegment;

function fakeCanvas(): HTMLCanvasElement {
  const ctx = {
    canvas: { style: {} },
    font: '',
    measureText: (text: string) => ({ width: text.length * 10, actualBoundingBoxAscent: 30, actualBoundingBoxDescent: 10 }),
    save() {},
    restore() {},
  };
  return {
    width: 1000,
    height: 500,
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 500 }),
    getContext: () => ctx,
  } as unknown as HTMLCanvasElement;
}

const dragState = (): TextDragState => ({
  isDraggingText: false,
  draggedTextId: null,
  draggedOverlayKind: null,
  dragStartPointer: { x: 0, y: 0 },
  dragTargets: [],
});

// Both captions sit at the default position; only one is on screen at a time.
const segment = {
  trimStart: 0,
  trimEnd: 10,
  textSegments: [],
  subtitleSegments: [caption('shown', 0, 2, 'Hello'), caption('later', 5, 7, 'Hello world')],
} as unknown as VideoSegment;
const atCaption = { clientX: 500, clientY: 400 } as MouseEvent;

describe('overlay hit-testing', () => {
  it('grabs the subtitle that is on screen, not a later one at the same spot', () => {
    expect(handleMouseDown(atCaption, segment, fakeCanvas(), dragState(), {}, 1)).toEqual({ kind: 'subtitle', id: 'shown' });
  });

  it('grabs nothing where no subtitle is on screen', () => {
    expect(handleMouseDown(atCaption, segment, fakeCanvas(), dragState(), {}, 3.5)).toBeNull();
  });
});
