import { act, renderHook } from '@testing-library/react';
import { useRef, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { useAppHistoryState } from '@/hooks/useAppHistoryState';
import { useAppOverlaySelection } from '@/hooks/useAppOverlaySelection';
import { createSubtitleTrackStateFromSegments, getSubtitleTracks, normalizeSubtitleTrackState } from '@/lib/subtitleTracks';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import { DEFAULT_WEBCAM_CONFIG } from '@/lib/webcam';
import { notifyUserError } from '@/lib/userNotifications';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';

vi.mock('@/lib/userNotifications', () => ({ notifyUserError: vi.fn() }));

const local = { trimStart: 8, trimEnd: 10, textSegments: [],
  trimSegments: [{ id: 'clip-range', startTime: 8, endTime: 10 }],
  ...createSubtitleTrackStateFromSegments([]),
} as VideoSegment;
const root = { ...local, trimStart: 0, trimEnd: 1, trimSegments: [{ id: 'root-range', startTime: 0, endTime: 1 }] };
const clip = { id: 'root', role: 'root' as const, name: 'Root', duration: 1, segment: root,
  backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
};
const initial: ProjectComposition = { mode: 'unified', focusedClipId: 'clip', selectedClipId: 'clip',
  clips: [clip, { ...clip, id: 'clip', duration: 10, segment: local }],
  globalSegment: { ...root, ...createSubtitleTrackStateFromSegments([
    { id: 'shared', text: 'Before', startTime: 0.5, endTime: 2.5, style: defaultSubtitleStyle(),
      sourceGroup: { kind: 'audio', audioSegmentId: 'audio-owner' } },
  ]) },
};

function changeText(segment: VideoSegment | null, text: string) {
  return segment && normalizeSubtitleTrackState({ ...segment,
    subtitleTracks: getSubtitleTracks(segment).map((track) => ({ ...track,
      segments: track.segments.map((cue) => cue.id === 'shared' ? { ...cue, text } : cue),
    })),
  });
}

function useHarness() {
  const [segment, rawSetSegment] = useState<VideoSegment | null>(local);
  const [composition, rawSetComposition] = useState<ProjectComposition | null>(initial);
  const segmentRef = useRef(segment);
  const projectRef = useRef<Project | null>({ id: 'p', name: 'P', createdAt: 1, lastModified: 1,
    duration: 10, segment: local, composition: initial, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
    webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [],
  });
  const projectIdRef = useRef<string | null>('p');
  const clipRef = useRef<string | null>('clip');
  const transition = useRef(false);
  const history = useAppHistoryState({
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, backgroundConfigRef: { current: DEFAULT_BACKGROUND_CONFIG },
    webcamConfig: DEFAULT_WEBCAM_CONFIG, webcamConfigRef: { current: DEFAULT_WEBCAM_CONFIG },
    composition, currentProjectDataRef: projectRef, currentRawMicAudioPath: '', currentRawVideoPath: '',
    currentRawWebcamVideoPath: '', currentRecordingMode: 'withoutCursor', duration: 10,
    handleProjectRawVideoPathChange: vi.fn(), isPlaying: false, isPlayingRef: { current: false },
    isProjectTransitionRef: transition, pendingSilentSegmentRef: { current: null }, pendingSilentSegmentTimerRef: { current: null },
    rawSetBackgroundConfig: vi.fn(), rawSetComposition, rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
    rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(), rawSetSegment, rawSetWebcamConfig: vi.fn(),
    segment, segmentRef, setBackgroundConfigState: vi.fn(), setCurrentProjectData: vi.fn(), setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
  });
  const overlay = useAppOverlaySelection({ segment, segmentRef, composition,
    setSegment: history.setSegment, setSegmentSilently: history.setSegmentSilently,
    setComposition: history.setComposition, setCompositionSilently: history.setCompositionSilently,
    currentProjectDataRef: projectRef, currentProjectIdRef: projectIdRef, loadedClipIdRef: clipRef,
    isProjectTransitionRef: transition, beginBatch: history.beginBatch, commitBatch: history.commitBatch,
  });
  return { segment, segmentRef, composition, history, overlay, projectRef, projectIdRef, clipRef, transition };
}

describe('composition subtitle editor ownership', () => {
  it('keeps view identity stable across unrelated React renders and repeated reads', () => {
    const { result, rerender } = renderHook(useHarness);
    const before = result.current.overlay.subtitleEditor.segment;
    expect(result.current.overlay.subtitleEditor.segmentRef.current).toBe(before);
    rerender();
    expect(result.current.overlay.subtitleEditor.segment).toBe(before);
    expect(result.current.overlay.subtitleEditor.segmentRef.current).toBe(before);
    expect(result.current.history.canUndo).toBe(false);
  });
  it('runs the real drag callback through canonical storage and one undo/redo boundary', () => {
    const { result } = renderHook(useHarness);
    const originalX = result.current.composition!.globalSegment!.subtitleSegments![0].style.x;
    expect(result.current.overlay.subtitleEditor.segment?.subtitleSegments?.[0]).toMatchObject({ startTime: 8, endTime: 9.5 });
    act(() => result.current.overlay.handleOverlayDragMove([{ kind: 'subtitle', id: 'shared', x: 22, y: 33 }]));
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0]).toMatchObject({ startTime: 0.5, endTime: 2.5,
      style: expect.objectContaining({ x: 22, y: 33 }) });
    expect(result.current.segment?.subtitleSegments).toEqual([]);
    expect(result.current.composition?.clips[1].segment.subtitleSegments).toEqual([]);
    expect(result.current.history.canUndo).toBe(true);
    act(() => result.current.history.undo());
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].style.x).toBe(originalX);
    expect(result.current.history.canUndo).toBe(false);
    act(() => result.current.history.redo());
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].style.x).toBe(22);
  });

  it('batches local and canonical changes together and strips view ownership from persisted segments', () => {
    const { result } = renderHook(useHarness);
    act(() => result.current.overlay.subtitleEditor.setSegment((previous) => {
      const edited = changeText(previous, 'After')!;
      return { ...edited, textSegments: [{ id: 'local-text', text: 'Local', startTime: 8, endTime: 9, style: defaultSubtitleStyle() }] };
    }));
    expect(result.current.segment?.textSegments).toHaveLength(1);
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].text).toBe('After');
    expect(Object.getOwnPropertySymbols(result.current.segment!)).toHaveLength(0);
    expect(Object.getOwnPropertySymbols(result.current.projectRef.current!.segment)).toHaveLength(0);
    act(() => result.current.history.undo());
    expect(result.current.segment?.textSegments).toEqual([]);
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].text).toBe('Before');
    expect(result.current.history.canUndo).toBe(false);
  });

  it('accepts fresh ref reads twice in one event and silently updates without adding undo', () => {
    const { result } = renderHook(useHarness);
    act(() => {
      const editor = result.current.overlay.subtitleEditor;
      editor.setSegmentSilently(changeText(editor.segmentRef.current, 'First'));
      editor.setSegmentSilently(changeText(editor.segmentRef.current, 'Second'));
    });
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].text).toBe('Second');
    expect(result.current.history.canUndo).toBe(false);
    expect(result.current.segment?.subtitleSegments).toEqual([]);
  });

  it('rejects a stale object after a provider result and rebases a functional edit onto the latest view', () => {
    const { result } = renderHook(useHarness);
    const stale = changeText(result.current.overlay.subtitleEditor.segment, 'Stale');
    act(() => result.current.history.setComposition((current) => current && { ...current,
      globalSegment: { ...current.globalSegment!, ...createSubtitleTrackStateFromSegments([
        ...current.globalSegment!.subtitleSegments!,
        { ...current.globalSegment!.subtitleSegments![0], id: 'later', startTime: 2.75, endTime: 2.9, text: 'Later' },
      ]) },
    }));
    act(() => result.current.overlay.subtitleEditor.setSegment(stale));
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].text).toBe('Before');
    expect(notifyUserError).toHaveBeenCalledWith('subtitleEditSourceChanged');
    act(() => result.current.overlay.subtitleEditor.setSegment((previous) => changeText(previous, 'Fresh')));
    expect(result.current.composition?.globalSegment?.subtitleSegments?.map((cue) => cue.text)).toEqual(['Fresh', 'Later']);
  });

  it('retires callbacks on clip handoff or project change before evaluating an updater', () => {
    const { result } = renderHook(useHarness);
    const update = vi.fn((previous: VideoSegment | null) => changeText(previous, 'Wrong owner'));
    const setter = result.current.overlay.subtitleEditor.setSegment;
    act(() => { result.current.clipRef.current = 'root'; setter(update); });
    expect(update).not.toHaveBeenCalled();
    act(() => { result.current.clipRef.current = 'clip'; result.current.projectIdRef.current = 'other'; setter(update); });
    expect(update).not.toHaveBeenCalled();
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].text).toBe('Before');
  });

  it('commits S2S sequence results with their full bounds and one history boundary', () => {
    const { result } = renderHook(useHarness);
    const cue = { ...initial.globalSegment!.subtitleSegments![0], id: 'new-result', startTime: 0.25, endTime: 2.75 };
    let written: VideoSegment | undefined;
    act(() => { written = result.current.overlay.subtitleEditor.populateS2sSubtitles([cue], [], 'en', { coordinateSpace: 'sequence' }); });
    expect(written).toBe(result.current.composition?.globalSegment);
    expect(written?.subtitleSegments?.[0]).toMatchObject({ id: 'new-result', startTime: 0.25, endTime: 2.75 });
    expect(result.current.segment?.subtitleSegments).toEqual([]);
    act(() => result.current.history.undo());
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].id).toBe('shared');
    expect(result.current.history.canUndo).toBe(false);
    act(() => result.current.history.redo());
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].id).toBe('new-result');
  });

  it('throws before a producer can acknowledge an unresolved or retired write', () => {
    const { result } = renderHook(useHarness);
    const writer = result.current.overlay.subtitleEditor.populateS2sSubtitles;
    expect(() => writer([], [], 'en')).toThrow('destination');
    act(() => { result.current.clipRef.current = 'root'; });
    expect(() => writer([], [], 'en', { coordinateSpace: 'sequence' })).toThrow('destination');
    expect(result.current.composition?.globalSegment?.subtitleSegments?.[0].id).toBe('shared');
  });
});
