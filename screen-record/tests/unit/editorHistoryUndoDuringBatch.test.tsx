import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useEditorHistory, type EditorHistorySnapshot } from '@/hooks/useEditorHistory';
import type { VideoSegment } from '@/types/video';

const segmentAt = (marker: number) => ({ trimStart: 0, trimEnd: marker } as unknown as VideoSegment);

const initialSnapshot = {
  segment: segmentAt(0),
  composition: null,
  backgroundConfig: {},
  webcamConfig: {},
  duration: 10,
  currentRecordingMode: 'screen',
  currentRawVideoPath: '',
  currentRawMicAudioPath: '',
  currentRawWebcamVideoPath: '',
} as unknown as EditorHistorySnapshot;

const trimEnd = (snapshot: EditorHistorySnapshot) => snapshot.segment?.trimEnd;

it('Ctrl+Z in the middle of a drag reverts the drag and keeps earlier history', () => {
  const { result } = renderHook(() => useEditorHistory({ initialSnapshot, applySnapshot: vi.fn() }));
  const current = () => trimEnd(result.current.getSnapshot());

  act(() => result.current.setSegment(segmentAt(1)));
  // A slider or timeline drag is in progress when the user presses Ctrl+Z.
  act(() => result.current.beginBatch());
  act(() => result.current.setSegment(segmentAt(2)));
  act(() => result.current.undo());
  expect(current()).toBe(1);
  act(() => result.current.commitBatch());

  expect(result.current.canRedo).toBe(true);
  act(() => result.current.undo());
  expect(current()).toBe(0);
  act(() => result.current.redo());
  act(() => result.current.redo());
  expect(current()).toBe(2);
});

it('changes made after a mid-drag undo still form one undo step', () => {
  const { result } = renderHook(() => useEditorHistory({ initialSnapshot, applySnapshot: vi.fn() }));
  const current = () => trimEnd(result.current.getSnapshot());

  act(() => result.current.setSegment(segmentAt(1)));
  act(() => result.current.beginBatch());
  act(() => result.current.setSegment(segmentAt(2)));
  act(() => result.current.undo());
  act(() => result.current.setSegment(segmentAt(3)));
  act(() => result.current.setSegment(segmentAt(4)));
  act(() => result.current.commitBatch());

  act(() => result.current.undo());
  expect(current()).toBe(1);
  act(() => result.current.undo());
  expect(current()).toBe(0);
});
