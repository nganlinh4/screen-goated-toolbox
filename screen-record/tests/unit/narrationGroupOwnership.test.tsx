import { act, fireEvent, render, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AudioPanel } from '@/components/sidepanel/AudioPanel';
import { NarrationTrack } from '@/components/timeline/NarrationTrack';
import { useEditorMainTimelineState } from '@/components/useEditorMainTimelineState';
import * as editorNarrationAlignment from '@/components/editorNarrationAlignment';
import { useTimelineTrackCallbacks } from '@/hooks/useTimelineTrackCallbacks';
import { useTimelineWorkspaceComposition } from '@/hooks/useTimelineWorkspaceComposition';
import { useSubtitleNarrationTargets } from '@/hooks/useSubtitleNarrationTargets';
import { buildNarrationSegment } from '@/hooks/subtitleNarrationHelpers';
import { narrationProjectEpoch, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { appendAudioSegments } from '@/lib/audioSegmentTime';
import { materializeNarrationGroupTakes, preserveNarrationGroupEdits, selectedNarrationSubtitleTimings, updateNarrationTake } from '@/lib/narrationGroupTakes';
import { excludeDeletedNarrationTakes, markNarrationTakesDeleted, registerNarrationBatch, retireNarrationBatch } from '@/lib/narrationBatchRetirements';
import { applyLiveNarrationSegments, clearLiveNarrationSegments } from '@/lib/liveNarrationStreamStore';
import { createSubtitleTrackStateFromSegments } from '@/lib/subtitleTracks';
import en from '@/i18n/en';
import type { ImportedAudioSegment, NarrationSegment, Project, ProjectComposition, VideoSegment } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));
vi.mock('@/hooks/useSubtitleTranslation', () => ({ useSubtitleTranslation: () => ({}) }));
vi.mock('@/lib/projectManager', () => ({ projectManager: {
  createEditorWriteIntent: vi.fn(() => ({})), updateProject: vi.fn().mockResolvedValue(undefined),
} }));

vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: new Proxy({}, {
  get: (_target, key) => key === 'audioPanelSelectedCount' ? '{count} selected' : String(key),
}) }) }));

const take = (id: string, startTime: number, inPoint: number, outPoint: number): NarrationSegment => ({
  id, rawAudioPath: 'shared.wav', name: 'Voice', duration: 8, startTime, inPoint, outPoint,
  playbackRate: 2, addedAt: 0, sourceSubtitleId: `sub-${id}`,
  narrationGroupTakeId: 'one', narrationGroupSourceStartTime: 10, narrationBatchId: 'batch-1',
});

describe('narration group ownership', () => {
  it('reuses the rendered owner across playback renders and rejects in-place topology changes', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    const segment: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments([{ id: 'sub-a', startTime: 1, endTime: 2, text: 'a', style }]) };
    let composition = { mode: 'separate', clips: [
      { id: 'a', rawVideoPath: 'a.mp4', segment, duration: 30 },
      { id: 'b', rawVideoPath: 'b.mp4', segment: { ...segment }, duration: 30 },
    ], narrationSegments: [take('a', 10, 0, 2)] } as ProjectComposition;
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'memo-owner');
    const epoch = narrationProjectEpoch(owner);
    const setSegment = vi.fn();
    const snapshot = () => ({ composition, segment, loadedClipId: 'a', videoPath: 'a.mp4', micPath: '' });
    const ownerSpy = vi.spyOn(editorNarrationAlignment, 'narrationEditorOwner');
    const hook = renderHook(({ time }) => useEditorMainTimelineState({
      t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment, setSegment, composition,
      setComposition: vi.fn(), currentTime: time, duration: 30, editingSubtitleId: null,
      setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(), setEditingSubtitleId: vi.fn(),
      setEditingKeystrokeSegmentId: vi.fn(), setEditingPointerId: vi.fn(), projectResetKey: 'memo-owner',
      currentProjectIdRef: owner, renderedProjectEpoch: epoch, loadedClipId: 'a', currentRawVideoPath: 'a.mp4',
      getNarrationSourceSnapshot: () => snapshot(), narrationSegments: composition.narrationSegments,
      beginBatch: vi.fn(), commitBatch: vi.fn(),
    }), { initialProps: { time: 0 } });
    act(() => hook.result.current.handleNarrationSelectionChange(['group-take-one']));
    for (let time = 1; time <= 20; time += 1) hook.rerender({ time });
    expect(ownerSpy).toHaveBeenCalledTimes(1);

    // A caller that mutates a nested clip in place must not silently adopt the
    // new owner merely because an unrelated playback render happened.
    composition.clips[1].segment.trimStart = 3;
    hook.rerender({ time: 21 });
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(setSegment).not.toHaveBeenCalled();

    // A new rendered immutable composition establishes the new owner.
    composition = { ...composition, clips: [...composition.clips] };
    hook.rerender({ time: 22 });
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(setSegment).toHaveBeenCalledOnce();
    ownerSpy.mockRestore(); hook.unmount(); setNarrationProjectId(owner, null);
  });

  it('aligns the current moved take and ignores deletion, replacement and project reopening before rerender', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    let segment: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments([{ id: 'sub-a', startTime: 1, endTime: 2, text: 'a', style }]) };
    const original = [take('a', 10, 0, 2)];
    let composition = { mode: 'separate', clips: [
      { id: 'a', rawVideoPath: 'a.mp4', segment, duration: 30 },
      { id: 'b', rawVideoPath: 'b.mp4', segment: { ...segment }, duration: 30 },
    ],
      narrationSegments: original } as ProjectComposition;
    const renderedComposition = composition;
    const renderedSegment = segment;
    const project = { current: { id: 'align-owner', composition } as Project | null };
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'align-owner');
    let renderedEpoch = narrationProjectEpoch(owner);
    let videoPath = 'a.mp4';
    const snapshot = () => ({ composition, segment, loadedClipId: 'a', videoPath, micPath: '' });
    const setSegment = vi.fn((next: React.SetStateAction<VideoSegment | null>) => {
      segment = (typeof next === 'function' ? next(segment) : next) as VideoSegment;
    });
    const hook = renderHook(() => ({
      editor: useEditorMainTimelineState({ t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment: renderedSegment,
        setSegment, composition: renderedComposition, currentRawVideoPath: 'a.mp4', setComposition: vi.fn(), currentTime: 0, duration: 30, editingSubtitleId: null,
        setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(), setEditingSubtitleId: vi.fn(),
        setEditingKeystrokeSegmentId: vi.fn(), setEditingPointerId: vi.fn(), projectResetKey: 'align-owner',
        currentProjectIdRef: owner, renderedProjectEpoch: renderedEpoch, loadedClipId: 'a', getNarrationSourceSnapshot: snapshot,
        narrationSegments: original, beginBatch: vi.fn(), commitBatch: vi.fn() }),
      track: useTimelineTrackCallbacks({ applyCurrentComposition: vi.fn(), composition, currentProjectDataRef: project,
        duration: 30, isPlaceholderBackedProject: false, persistCurrentComposition: vi.fn(), segmentRef: { current: segment },
        setComposition: vi.fn(), setCurrentProjectData: vi.fn(), setEditorPreviewDuration: vi.fn(), setSegment,
        updateCurrentMusicSegments: vi.fn(), updatePlaceholderProjectDuration: vi.fn(),
        updateCurrentNarrationSegments: (update) => {
          composition = { ...composition, narrationSegments: update(composition.narrationSegments ?? []) };
          project.current = { ...project.current!, composition };
        } }),
    }));
    act(() => hook.result.current.editor.handleNarrationSelectionChange(['group-take-one']));
    act(() => {
      hook.result.current.track.handleUpdateNarrationSegment('group-take-one', { startTime: 20 });
      hook.result.current.editor.handleAlignSubtitlesToNarration();
    });
    expect(segment.subtitleSegments?.[0]).toMatchObject({ startTime: 20, endTime: 21 });
    setSegment.mockClear();
    act(() => {
      hook.result.current.track.handleDeleteNarrationSegments(['group-take-one']);
      hook.result.current.editor.handleAlignSubtitlesToNarration();
    });
    expect(setSegment).not.toHaveBeenCalled();
    composition = { ...composition, narrationSegments: original };
    const originalClips = composition.clips;
    composition = { ...composition, clips: [...originalClips].reverse() };
    act(() => hook.result.current.editor.handleAlignSubtitlesToNarration());
    expect(setSegment).not.toHaveBeenCalled();
    composition = { ...composition, clips: [originalClips[0], { ...originalClips[1],
      segment: { ...originalClips[1].segment, trimStart: 3 } }] };
    act(() => hook.result.current.editor.handleNarrationSelectionChange(['group-take-one']));
    act(() => hook.result.current.editor.handleAlignSubtitlesToNarration());
    expect(setSegment).not.toHaveBeenCalled();
    composition = { ...composition, clips: originalClips };
    videoPath = 'replacement.mp4';
    act(() => hook.result.current.editor.handleNarrationSelectionChange(['group-take-one']));
    act(() => hook.result.current.editor.handleAlignSubtitlesToNarration());
    expect(setSegment).not.toHaveBeenCalled();
    videoPath = 'a.mp4';
    setNarrationProjectId(owner, 'another-project');
    setNarrationProjectId(owner, 'align-owner');
    act(() => hook.result.current.editor.handleNarrationSelectionChange(['group-take-one']));
    act(() => hook.result.current.editor.handleAlignSubtitlesToNarration());
    expect(setSegment).not.toHaveBeenCalled();
    renderedEpoch = narrationProjectEpoch(owner);
    hook.rerender();
    act(() => hook.result.current.editor.handleAlignSubtitlesToNarration());
    expect(setSegment).toHaveBeenCalledOnce();
    hook.unmount(); setNarrationProjectId(owner, null);
  });
  it('preserves legacy split cues on repeated alignment when original boundaries are unavailable', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    const segment: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments([
        { id: 'sub-a', startTime: 1, endTime: 2, text: 'a', style },
        { id: 'sub-b', startTime: 2, endTime: 3, text: 'b', style },
      ]) };
    const legacy = updateNarrationTake([{ ...take('a', 0, 0, 4), sourceSubtitleIds: ['sub-a', 'sub-b'] }],
      'group-take-one', { startTime: 10, inPoint: 1, outPoint: 3, playbackRate: 2 });
    const setSegment = vi.fn();
    const hook = renderHook(() => useEditorMainTimelineState({
      t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment, setSegment,
      composition: null, setComposition: vi.fn(), currentTime: 0, duration: 30,
      editingSubtitleId: null, setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(),
      setEditingSubtitleId: vi.fn(), setEditingKeystrokeSegmentId: vi.fn(), setEditingPointerId: vi.fn(),
      projectResetKey: 'legacy-split-alignment', narrationSegments: legacy, beginBatch: vi.fn(), commitBatch: vi.fn(),
    }));
    act(() => hook.result.current.handleNarrationSelectionChange(['group-take-one']));
    act(() => { hook.result.current.handleAlignSubtitlesToNarration(); hook.result.current.handleAlignSubtitlesToNarration(); });
    expect(setSegment).not.toHaveBeenCalled();
    hook.unmount();
  });
  it('aligns each split cue to a moved, trimmed and sped-up selected take', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    const cues = [
      { id: 'sub-a', startTime: 0, endTime: 2, text: 'a', style, splitGroupId: 'split', splitGroupText: 'a b', splitGroupStartTime: 0, splitGroupEndTime: 4 },
      { id: 'sub-b', startTime: 2, endTime: 4, text: 'b', style, splitGroupId: 'split', splitGroupText: 'a b', splitGroupStartTime: 0, splitGroupEndTime: 4 },
    ];
    const targets = renderHook(() => useSubtitleNarrationTargets(cues, [], null, true));
    const target = targets.result.current.requestItems[0];
    expect(target.sourceSubtitleTimings).toEqual([{ id: 'sub-a', start: 0, end: 0.5 }, { id: 'sub-b', start: 0.5, end: 1 }]);
    const generated = buildNarrationSegment({ subtitleId: target.id, text: target.text, path: 'shared.wav',
      duration: 4, sourceInPoint: 0, sourceOutPoint: 4, startTime: 0, endTime: 4,
      narrationGroupTakeId: 'one', sourceSubtitleId: target.sourceSubtitleId,
      replaceSubtitleIds: target.replaceSubtitleIds, sourceSubtitleTimings: target.sourceSubtitleTimings },
    'batch', { method: 'GeminiLive' } as Parameters<typeof buildNarrationSegment>[2]);
    const raw = JSON.parse(JSON.stringify([generated])) as NarrationSegment[];
    const edited = updateNarrationTake(raw, 'group-take-one', { startTime: 10, inPoint: 1, outPoint: 3, playbackRate: 2 });
    let segment: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments(cues) };
    const setSegment = vi.fn((next: React.SetStateAction<VideoSegment | null>) => {
      segment = (typeof next === 'function' ? next(segment) : next) as VideoSegment;
    });
    const hook = renderHook(() => useEditorMainTimelineState({
      t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment, setSegment,
      composition: null, setComposition: vi.fn(), currentTime: 0, duration: 30,
      editingSubtitleId: null, setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(),
      setEditingSubtitleId: vi.fn(), setEditingKeystrokeSegmentId: vi.fn(), setEditingPointerId: vi.fn(),
      projectResetKey: 'split-alignment', narrationSegments: edited, beginBatch: vi.fn(), commitBatch: vi.fn(),
    }));
    act(() => hook.result.current.handleNarrationSelectionChange(['group-take-one']));
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(segment.subtitleSegments?.map(({ startTime, endTime }) => [startTime, endTime])).toEqual([[10, 10.5], [10.5, 11]]);
    const regenerated = renderHook(() => useSubtitleNarrationTargets(segment.subtitleSegments ?? [], [], null, true));
    expect(regenerated.result.current.requestItems[0]).toMatchObject({
      startTime: 10, endTime: 11,
      sourceSubtitleTimings: [{ id: 'sub-a', start: 0, end: 0.5 }, { id: 'sub-b', start: 0.5, end: 1 }],
    });
    hook.rerender();
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(segment.subtitleSegments?.map(({ startTime, endTime }) => [startTime, endTime])).toEqual([[10, 10.5], [10.5, 11]]);
    hook.unmount(); targets.unmount(); regenerated.unmount();
  });
  it('aligns the selected live group before its deferred editor flush', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    const segment: VideoSegment = {
      trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments([
        { id: 'sub-a', startTime: 1, endTime: 2, text: 'a', style },
        { id: 'sub-b', startTime: 2, endTime: 3, text: 'b', style },
      ]),
    };
    // The project state has not yet flushed the live result that the track and
    // panel render during playback.
    let updated: VideoSegment = segment;
    const setSegment = vi.fn((next: React.SetStateAction<VideoSegment | null>) => {
      updated = (typeof next === 'function' ? next(updated) : next) as VideoSegment;
    });
    applyLiveNarrationSegments('live-alignment', [take('a', 10, 0, 2), take('b', 11, 2, 4)], []);
    const hook = renderHook(() => useEditorMainTimelineState({
      t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment, setSegment,
      composition: null, setComposition: vi.fn(), currentTime: 0, duration: 30,
      editingSubtitleId: null, setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(),
      setEditingSubtitleId: vi.fn(), setEditingKeystrokeSegmentId: vi.fn(),
      setEditingPointerId: vi.fn(), projectResetKey: 'live-alignment', narrationSegments: [],
      beginBatch: vi.fn(), commitBatch: vi.fn(),
    }));
    act(() => hook.result.current.handleNarrationSelectionChange(['group-take-one']));
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(updated.subtitleSegments?.map(({ id, startTime, endTime }) => ({ id, startTime, endTime }))).toEqual([
      { id: 'sub-a', startTime: 10, endTime: 11 },
      { id: 'sub-b', startTime: 11, endTime: 12 },
    ]);
    hook.unmount();
    clearLiveNarrationSegments('live-alignment');
  });

  it('aligns the current segment without erasing a newer deferred subtitle update', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    const cue = { id: 'sub-a', startTime: 1, endTime: 2, text: 'a', style };
    const rendered: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments([cue]) };
    let current: VideoSegment = rendered;
    const setSegment = vi.fn((next: React.SetStateAction<VideoSegment | null>) => {
      current = (typeof next === 'function' ? next(current) : next) as VideoSegment;
    });
    const owner = { current: 'p' as string | null };
    let loadedClipId = 'a';
    const source = () => ({ composition: null, segment: current, loadedClipId, videoPath: 'a.mp4', micPath: '' });
    const hook = renderHook(() => useEditorMainTimelineState({
      t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment: rendered, setSegment,
      composition: null, setComposition: vi.fn(), currentTime: 0, duration: 30,
      editingSubtitleId: null, setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(),
      setEditingSubtitleId: vi.fn(), setEditingKeystrokeSegmentId: vi.fn(), setEditingPointerId: vi.fn(),
      projectResetKey: 'p', currentProjectIdRef: owner, loadedClipId: 'a', currentRawVideoPath: 'a.mp4', getNarrationSourceSnapshot: source,
      narrationSegments: [take('a', 10, 0, 2)], beginBatch: vi.fn(), commitBatch: vi.fn(),
    }));
    act(() => hook.result.current.handleNarrationSelectionChange(['group-take-one']));
    current = { ...rendered, ...createSubtitleTrackStateFromSegments([cue,
      { id: 'late', startTime: 3, endTime: 4, text: 'late result', style }]) };
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(current.subtitleSegments?.map(({ id, startTime, endTime }) => ({ id, startTime, endTime }))).toEqual([
      { id: 'sub-a', startTime: 10, endTime: 11 },
      { id: 'late', startTime: 3, endTime: 4 },
    ]);
    setSegment.mockClear();
    loadedClipId = 'b';
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(setSegment).not.toHaveBeenCalled();
    hook.unmount();
  });

  it('keeps cues outside a trimmed take unchanged and recalculates group bounds from all surviving cues', () => {
    const style = { fontSize: 30, color: '#fff', x: 50, y: 50 };
    const cues = [
      { id: 'sub-a', startTime: 0, endTime: 2, text: 'a', style, splitGroupId: 'split', splitGroupText: 'a b', splitGroupStartTime: 0, splitGroupEndTime: 4 },
      { id: 'sub-b', startTime: 2, endTime: 4, text: 'b', style, splitGroupId: 'split', splitGroupText: 'a b', splitGroupStartTime: 0, splitGroupEndTime: 4 },
    ];
    const raw = [{ ...take('a', 0, 0, 4), sourceSubtitleIds: ['sub-a', 'sub-b'],
      sourceSubtitleTimings: [{ id: 'sub-a', start: 0, end: 0.5 }, { id: 'sub-b', start: 0.5, end: 1 }] }];
    const edited = updateNarrationTake(raw, 'group-take-one', { startTime: 10, inPoint: 2.5, outPoint: 4, playbackRate: 2 });
    let segment: VideoSegment = { trimStart: 0, trimEnd: 30, zoomKeyframes: [], textSegments: [],
      ...createSubtitleTrackStateFromSegments(cues) };
    const hook = renderHook(() => useEditorMainTimelineState({
      t: en, activePanel: 'audio', setActivePanel: vi.fn(), segment,
      setSegment: (next) => { segment = (typeof next === 'function' ? next(segment) : next) as VideoSegment; }, composition: null, setComposition: vi.fn(),
      currentTime: 0, duration: 30, editingSubtitleId: null, setEditingKeyframeId: vi.fn(), setEditingTextId: vi.fn(),
      setEditingSubtitleId: vi.fn(), setEditingKeystrokeSegmentId: vi.fn(), setEditingPointerId: vi.fn(),
      projectResetKey: 'clipped-alignment', narrationSegments: edited, beginBatch: vi.fn(), commitBatch: vi.fn(),
    }));
    act(() => hook.result.current.handleNarrationSelectionChange(['group-take-one']));
    act(() => hook.result.current.handleAlignSubtitlesToNarration());
    expect(segment.subtitleSegments?.map(({ startTime, endTime, splitGroupStartTime, splitGroupEndTime }) =>
      ({ startTime, endTime, splitGroupStartTime, splitGroupEndTime }))).toEqual([
      { startTime: 0, endTime: 2, splitGroupStartTime: 0, splitGroupEndTime: 10.75 },
      { startTime: 10, endTime: 10.75, splitGroupStartTime: 0, splitGroupEndTime: 10.75 },
    ]);
    const regenerated = renderHook(() => useSubtitleNarrationTargets(segment.subtitleSegments ?? [], [], null, true));
    expect(regenerated.result.current.requestItems[0]).toMatchObject({ startTime: 0, endTime: 10.75 });
    hook.unmount(); regenerated.unmount();
  });

  it('routes actual panel and timeline callbacks to the group without changing source takes', () => {
    let stored = [take('a', 10, 0, 2), take('b', 11, 2, 4)];
    const projectRef = { current: { id: 'p' } as Project | null };
    const update = (fn: (items: NarrationSegment[]) => NarrationSegment[]) => { stored = fn(stored); };
    const options = {
      applyCurrentComposition: vi.fn(), composition: null, currentProjectDataRef: projectRef,
      duration: 30, isPlaceholderBackedProject: false, persistCurrentComposition: vi.fn(),
      segmentRef: { current: null }, setComposition: vi.fn(), setCurrentProjectData: vi.fn(),
      setEditorPreviewDuration: vi.fn(), setSegment: vi.fn(), updateCurrentMusicSegments: vi.fn(),
      updateCurrentNarrationSegments: update, updatePlaceholderProjectDuration: vi.fn(),
    };
    const { result } = renderHook(() => useTimelineTrackCallbacks(options));
    const groupId = materializeNarrationGroupTakes(stored)[0].id;
    expect(groupId).toBe('group-take-one');
    const panel = render(<AudioPanel importedSegments={[]} narrationSegments={stored} subtitleSegments={[]}
      selectedImportedIds={new Set()} selectedNarrationIds={new Set([groupId])}
      onUpdateImportedSegment={vi.fn()} onUpdateNarrationSegment={result.current.handleUpdateNarrationSegment} />);
    expect(panel.getByText('1 selected')).toBeTruthy();
    fireEvent.change(panel.getByLabelText('audioPanelSpeed'), { target: { value: '1' } });
    expect(materializeNarrationGroupTakes(stored)[0].playbackRate).toBe(1);
    vi.stubGlobal('PointerEvent', MouseEvent);
    vi.stubGlobal('requestAnimationFrame', () => 1);
    vi.stubGlobal('cancelAnimationFrame', () => {});
    const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0, y: 0, left: 0, top: 0, right: 300, bottom: 30, width: 300, height: 30, toJSON: () => ({}),
    });
    Object.defineProperty(HTMLElement.prototype, 'setPointerCapture', { configurable: true, value: vi.fn() });
    const timeline = render(<NarrationTrack segments={stored} duration={30}
      onUpdateSegment={result.current.handleUpdateNarrationSegment} />);
    const block = timeline.container.querySelector<HTMLElement>('.narration-track-segment')!;
    fireEvent.pointerDown(block, { button: 0, clientX: 100 });
    fireEvent.pointerMove(block, { clientX: 160 });
    fireEvent.pointerUp(block, { clientX: 160 });
    expect(materializeNarrationGroupTakes(stored)[0].startTime).toBe(16);
    timeline.unmount();
    Reflect.deleteProperty(HTMLElement.prototype, 'setPointerCapture');
    rect.mockRestore();
    vi.unstubAllGlobals();
    act(() => result.current.handleUpdateNarrationSegment(groupId, { startTime: 20, inPoint: 1, outPoint: 3 }));
    expect(stored.map(({ id, inPoint, outPoint, sourceSubtitleId }) => ({ id, inPoint, outPoint, sourceSubtitleId }))).toEqual([
      { id: 'a', inPoint: 0, outPoint: 2, sourceSubtitleId: 'sub-a' },
      { id: 'b', inPoint: 2, outPoint: 4, sourceSubtitleId: 'sub-b' },
    ]);
    const restored = JSON.parse(JSON.stringify(stored)) as NarrationSegment[];
    expect(materializeNarrationGroupTakes(restored)[0]).toMatchObject({ id: groupId, startTime: 20, inPoint: 1, outPoint: 3, playbackRate: 1 });
    expect([...selectedNarrationSubtitleTimings(restored, new Set([groupId])).entries()]).toEqual([
      ['sub-a', { startTime: 20, endTime: 21 }], ['sub-b', { startTime: 21, endTime: 22 }],
    ]);
    const appended = preserveNarrationGroupEdits(restored, [take('c', 12, 4, 6)]);
    expect(appended[0].narrationGroupEdit).toEqual(restored[0].narrationGroupEdit);
    expect(materializeNarrationGroupTakes([...restored, ...appended])[0]).toMatchObject({ startTime: 20, inPoint: 1, outPoint: 3 });
    const replacement = preserveNarrationGroupEdits(restored, [{ ...take('c', 12, 4, 6), rawAudioPath: 'new.wav' }]);
    expect(replacement[0].narrationGroupEdit).toBeUndefined();
    stored.push({ ...take('other', 25, 0, 1), narrationGroupTakeId: undefined });
    registerNarrationBatch('p', 'batch-1', vi.fn());
    act(() => result.current.handleDeleteNarrationSegments([groupId]));
    expect(stored.map((item) => item.id)).toEqual(['other']);
    expect(excludeDeletedNarrationTakes('p', [take('later-partial', 12, 4, 6)])).toEqual([]);
    expect(excludeDeletedNarrationTakes('p', [take('later-final', 12, 4, 6)])).toEqual([]);
    expect(excludeDeletedNarrationTakes('p', [{ ...take('new', 12, 4, 6), narrationBatchId: 'batch-2' }])).toHaveLength(1);
    retireNarrationBatch('batch-1');
    expect(excludeDeletedNarrationTakes('p', [take('later-final', 12, 4, 6)])).toHaveLength(1);
    act(() => result.current.handleUpdateNarrationSegment('other', { startTime: 7 }));
    expect(stored[0].startTime).toBe(7);
  });

  it('appends after playback-adjusted durations for both existing and incoming audio', () => {
    const audio = (id: string, startTime: number, playbackRate: number): ImportedAudioSegment => ({
      id, rawAudioPath: `${id}.wav`, name: id, duration: 8, startTime, inPoint: 0, outPoint: 8, playbackRate, addedAt: 0,
    });
    expect(appendAudioSegments([audio('old', 2, 2)], [audio('new', 0, 0.5), audio('last', 0, 1)]).map((item) => item.startTime)).toEqual([2, 6, 22]);
  });

  it('keeps an edit made before the second group member arrives', () => {
    const edited = updateNarrationTake([take('a', 10, 0, 2)], 'a', { startTime: 17, playbackRate: 1 });
    expect(materializeNarrationGroupTakes(edited)[0]).toMatchObject({ id: 'group-take-one', startTime: 17, playbackRate: 1 });
    const incoming = preserveNarrationGroupEdits(edited, [take('b', 11, 2, 4)]);
    expect(materializeNarrationGroupTakes([...edited, ...incoming])[0]).toMatchObject({
      id: 'group-take-one', startTime: 17, outPoint: 4, playbackRate: 1,
    });
  });

  it('does not extend a group endpoint explicitly trimmed while more audio arrives', () => {
    const moved = updateNarrationTake([take('a', 10, 0, 2)], 'group-take-one', { startTime: 17 });
    const trimmed = updateNarrationTake(moved, 'group-take-one', { outPoint: 1.5 });
    const incoming = preserveNarrationGroupEdits(trimmed, [take('b', 11, 2, 4)]);
    expect(materializeNarrationGroupTakes([...trimmed, ...incoming])[0]).toMatchObject({
      startTime: 17, outPoint: 1.5,
    });
  });

  it('keeps the selected group identity while members arrive and routes legacy member actions to the group', () => {
    const first = [take('a', 10, 0, 2)];
    const selected = new Set([materializeNarrationGroupTakes(first)[0].id]);
    expect([...selected]).toEqual(['group-take-one']);
    const timeline = render(<NarrationTrack segments={first} duration={30} selectedIds={selected} />);
    const panel = render(<AudioPanel importedSegments={[]} narrationSegments={first} subtitleSegments={[]}
      selectedImportedIds={new Set()} selectedNarrationIds={selected}
      onUpdateImportedSegment={vi.fn()} onUpdateNarrationSegment={vi.fn()} />);
    expect(timeline.container.querySelector('[data-narration-segment-id="group-take-one"][data-selected="true"]')).toBeTruthy();
    expect(panel.getByText('1 selected')).toBeTruthy();
    const both = [...first, take('b', 11, 2, 4)];
    timeline.rerender(<NarrationTrack segments={both} duration={30} selectedIds={selected} />);
    panel.rerender(<AudioPanel importedSegments={[]} narrationSegments={both} subtitleSegments={[]}
      selectedImportedIds={new Set()} selectedNarrationIds={selected}
      onUpdateImportedSegment={vi.fn()} onUpdateNarrationSegment={vi.fn()} />);
    expect(timeline.container.querySelector('[data-narration-segment-id="group-take-one"][data-selected="true"]')).toBeTruthy();
    expect(panel.getByText('1 selected')).toBeTruthy();
    const edited = updateNarrationTake(both, 'a', { startTime: 16 });
    expect(materializeNarrationGroupTakes(edited)[0].startTime).toBe(16);
    expect([...selectedNarrationSubtitleTimings(edited, new Set(['a'])).keys()]).toEqual(['sub-a', 'sub-b']);
    const update = renderHook(() => useTimelineTrackCallbacks({
      applyCurrentComposition: vi.fn(), composition: null,
      currentProjectDataRef: { current: { id: 'p' } as Project | null },
      duration: 30, isPlaceholderBackedProject: false, persistCurrentComposition: vi.fn(),
      segmentRef: { current: null }, setComposition: vi.fn(), setCurrentProjectData: vi.fn(),
      setEditorPreviewDuration: vi.fn(), setSegment: vi.fn(), updateCurrentMusicSegments: vi.fn(),
      updateCurrentNarrationSegments: (fn: (items: NarrationSegment[]) => NarrationSegment[]) => expect(fn(edited)).toEqual([]),
      updatePlaceholderProjectDuration: vi.fn(),
    }));
    act(() => update.result.current.handleDeleteNarrationSegments(['a']));
    update.unmount(); timeline.unmount(); panel.unmount();
  });

  it('retires the exact producing batch when its deletion ledger reaches capacity', () => {
    const cancel = vi.fn();
    registerNarrationBatch('p', 'batch-1', cancel);
    const many = Array.from({ length: 8193 }, (_, index) => ({
      ...take(`take-${index}`, 0, 0, 1), narrationGroupTakeId: `group-${index}`,
    }));
    markNarrationTakesDeleted('p', many);
    expect(cancel).toHaveBeenCalledOnce();
    expect(excludeDeletedNarrationTakes('p', [take('later', 0, 0, 1)])).toEqual([]);
    expect(excludeDeletedNarrationTakes('p', [{ ...take('new', 0, 0, 1), narrationBatchId: 'new-batch' }])).toHaveLength(1);
    retireNarrationBatch('batch-1');
  });

  it('keeps deletion after a live flush and remount while allowing a new batch', () => {
    const owner = { current: 'p' as string | null };
    const project = { current: { id: 'p', composition: { narrationSegments: [], audioSegments: [], clips: [] } } as unknown as Project };
    const setComposition = (value: unknown) => { project.current.composition = value as Project['composition']; };
    const workspaceOptions = {
      composition: project.current.composition, currentProjectDataRef: project, currentProjectId: 'p', currentProjectIdRef: owner,
      currentTime: 0, duration: 30, editorHistory: { replaceSnapshot: vi.fn() }, handleEditorRawVideoPathChange: vi.fn(),
      isPlaceholderBackedProject: false, isPlaying: true, loadProjects: vi.fn().mockResolvedValue(undefined),
      rawSetComposition: vi.fn(), rawSetSegment: vi.fn(), segmentRef: { current: null }, setComposition,
      setCompositionSilently: vi.fn(), setCurrentProjectData: vi.fn(), setCurrentVideo: vi.fn(),
      setEditorPreviewDuration: vi.fn(), setSegment: vi.fn(), videoControllerRef: { current: undefined },
      videoRef: { current: { paused: false } as HTMLVideoElement },
    };
    const first = renderHook(() => useTimelineWorkspaceComposition(workspaceOptions));
    registerNarrationBatch('p', 'batch-1', vi.fn());
    act(() => first.result.current.applyNarrationAudioSegments([take('a', 10, 0, 2), take('b', 11, 2, 4)], []));
    const callbacks = renderHook(() => useTimelineTrackCallbacks({
      ...workspaceOptions, applyCurrentComposition: first.result.current.applyCurrentComposition,
      persistCurrentComposition: first.result.current.persistCurrentComposition, updateCurrentMusicSegments: vi.fn(),
      updateCurrentNarrationSegments: first.result.current.updateCurrentNarrationSegments,
      updatePlaceholderProjectDuration: vi.fn(),
    }));
    act(() => callbacks.result.current.handleDeleteNarrationSegments(['group-take-one']));
    expect(project.current.composition?.narrationSegments).toEqual([]);
    clearLiveNarrationSegments('p');
    first.unmount();
    const second = renderHook(() => useTimelineWorkspaceComposition(workspaceOptions));
    act(() => second.result.current.applyNarrationAudioSegments([take('late', 12, 4, 6)], ['sub-late']));
    expect(project.current.composition?.narrationSegments).toEqual([]);
    act(() => second.result.current.applyNarrationAudioSegments([{ ...take('new', 13, 0, 2),
      narrationBatchId: 'batch-2', narrationGroupTakeId: 'two' }], []));
    expect(project.current.composition?.narrationSegments?.map((item) => item.id)).toEqual(['new']);
    retireNarrationBatch('batch-1');
    clearLiveNarrationSegments('p');
    callbacks.unmount(); second.unmount();
  });
});
