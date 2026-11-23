import { act, renderHook } from '@testing-library/react';
import { useRef, useState } from 'react';
import { expect, it, vi } from 'vitest';
import { captureSubtitleSourceOwners, subtitleSourceMatches } from '@/hooks/subtitleSourceOwnership';
import { useSubtitleResultApplication } from '@/hooks/useSubtitleResultApplication';
import { activateNarrationProject, narrationProjectEpoch } from '@/hooks/narrationProjectOwnership';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';

const a = { trimStart: 0, trimEnd: 3, trimSegments: [{ id: 'a', startTime: 0, endTime: 3 }], subtitleSegments: [] } as VideoSegment;
const b = { trimStart: 0, trimEnd: 7, trimSegments: [{ id: 'b', startTime: 0, endTime: 7 }], subtitleSegments: [] } as VideoSegment;
const composition = {
  mode: 'separate', focusedClipId: 'b', selectedClipId: 'b',
  clips: [{ id: 'a', duration: 3, segment: a, rawVideoPath: 'a.mp4' },
    { id: 'b', duration: 7, segment: b, rawVideoPath: 'b.mp4' }],
} as ProjectComposition;
const plan = {
  clips: [{ clipId: 'a', sourcePath: 'a.mp4', sourceDuration: 3, trimSegments: [] }],
  sourceTypeForNative: 'video', clipTransformsByClip: {}, replacementRangesByClip: {}, indicator: {} as never,
} as SubtitleGenerationPlan;
const owners = captureSubtitleSourceOwners(plan, a, composition);
const context = {
  projectKey: 'p',
  indicator: {} as never, sourceTypeForNative: 'video' as const, replacementRangesByClip: {},
  clipTransformsByClip: {}, sourceOwnersByClip: owners,
};
const resultA = { clipId: 'a', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'clip A' }] };

it('rejects a deferred updater after the same project is reopened', () => {
  const owner = { current: 'p' as string | null };
  const epoch = narrationProjectEpoch(owner);
  let deferred: ((prev: ProjectComposition | null) => ProjectComposition | null) | undefined;
  const setComposition = vi.fn((next) => { if (typeof next === 'function') deferred = next; });
  const { result } = renderHook(() => useSubtitleResultApplication({
    segment: b, currentProjectIdRef: owner, currentProjectDataRef: { current: { id: 'p', composition } as Project },
    ownsProjectEpoch: () => narrationProjectEpoch(owner) === epoch,
    currentRawVideoPath: 'b.mp4', currentRawMicAudioPath: '', setSegment: vi.fn(), setSegmentSilently: vi.fn(),
    loadedClipIdRef: { current: 'b' }, composition, setComposition, setCompositionSilently: setComposition,
    jobId: 'job', jobContext: context,
  }));
  act(() => result.current.queueSubtitleResults([resultA], true));
  expect(deferred).toBeDefined();
  activateNarrationProject(owner, 'p');
  expect(deferred!(composition)).toBe(composition);
});

function renderSource(initial: ProjectComposition, projectId = 'p') {
  const stale = vi.fn();
  const hook = renderHook(() => {
    const [current, setCurrent] = useState<ProjectComposition | null>(initial);
    const [segment, setSegment] = useState<VideoSegment | null>(b);
    const projectRef = useRef<Project | null>(null);
    projectRef.current = { id: projectId, composition: current ?? undefined, rawVideoPath: 'b.mp4', segment } as Project;
    const applied = useSubtitleResultApplication({
      segment, segmentRef: { current: segment }, currentProjectDataRef: projectRef,
      currentRawVideoPath: 'b.mp4', currentRawMicAudioPath: '', onStaleResults: stale,
      setSegment, setSegmentSilently: setSegment, loadedClipIdRef: { current: 'b' },
      composition: current, setComposition: setCurrent, setCompositionSilently: setCurrent,
      jobId: 'job', jobContext: context,
    });
    return { current, segment, ...applied };
  });
  return { ...hook, stale };
}

it('applies a separate-mode result to its original unloaded clip, leaving the loaded clip intact', () => {
  const { result, stale } = renderSource(composition);
  act(() => result.current.queueSubtitleResults([resultA], true));
  expect(result.current.current?.clips[0].segment.subtitleSegments).toEqual(expect.arrayContaining([
    expect.objectContaining({ text: 'clip A' }),
  ]));
  expect(result.current.current?.clips[1].segment).toEqual(b);
  expect(result.current.segment).toEqual(b);
  expect(stale).not.toHaveBeenCalled();
});

it('rejects results when the synchronous project owner changed before the view updates', () => {
  const { result, stale } = renderSource(composition, 'other');
  act(() => result.current.queueSubtitleResults([resultA], true));
  expect(result.current.current).toEqual(composition);
  expect(result.current.segment).toEqual(b);
  expect(stale).toHaveBeenCalledOnce();
});

it('rejects a replaced source under the same clip id and reports the skipped result', () => {
  const replaced = { ...composition, clips: [ { ...composition.clips[0], rawVideoPath: 'replacement.mp4' }, composition.clips[1] ] };
  const { result, stale } = renderSource(replaced);
  act(() => result.current.queueSubtitleResults([resultA], true));
  expect(result.current.current).toEqual(replaced);
  expect(result.current.segment).toEqual(b);
  expect(stale).toHaveBeenCalledOnce();
});

it('rejects a result for a replaced loaded clip before the old media unloads', () => {
  const ownerPlan = {
    ...plan, clips: [{ ...plan.clips[0], clipId: 'b', sourcePath: 'b.mp4', sourceDuration: 7 }],
  } as SubtitleGenerationPlan;
  const loadedOwners = captureSubtitleSourceOwners(ownerPlan, b, composition);
  expect(subtitleSourceMatches(loadedOwners.b, composition, 'b', b, 'b.mp4', '')).toBe(true);
  const replacement = { ...composition, clips: [composition.clips[0],
    { ...composition.clips[1], rawVideoPath: 'replacement.mp4' }] };
  const stale = vi.fn();
  const { result } = renderHook(() => {
    const [current, setCurrent] = useState<ProjectComposition | null>(replacement);
    const [segment, setSegment] = useState<VideoSegment | null>(b);
    const projectRef = useRef<Project | null>(null);
    projectRef.current = { id: 'p', composition: current ?? undefined, rawVideoPath: 'b.mp4', segment } as Project;
    const applied = useSubtitleResultApplication({
      segment, segmentRef: { current: segment }, currentProjectDataRef: projectRef,
      currentRawVideoPath: 'b.mp4', currentRawMicAudioPath: '', onStaleResults: stale,
      setSegment, setSegmentSilently: setSegment, loadedClipIdRef: { current: 'b' },
      composition: current, setComposition: setCurrent, setCompositionSilently: setCurrent,
      jobId: 'job', jobContext: { ...context, sourceOwnersByClip: loadedOwners },
    });
    return { current, segment, ...applied };
  });
  act(() => result.current.queueSubtitleResults([
    { clipId: 'b', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'old source' }] },
  ], true));
  expect(result.current.current).toEqual(replacement);
  expect(result.current.segment).toEqual(b);
  expect(stale).toHaveBeenCalledOnce();
});

it('fails closed while loaded and project trim boundaries are out of sync', () => {
  const ownerPlan = { ...plan, clips: [{ ...plan.clips[0], clipId: 'b', sourcePath: 'b.mp4', sourceDuration: 7 }] } as SubtitleGenerationPlan;
  const owner = captureSubtitleSourceOwners(ownerPlan, b, composition).b;
  const changed = { ...composition, clips: [composition.clips[0],
    { ...composition.clips[1], segment: { ...b, trimEnd: 6 } }] };
  expect(subtitleSourceMatches(owner, changed, 'b', b, 'b.mp4', '')).toBe(false);
});

it('rejects changed trim boundaries and changed audio placement or rate', () => {
  const trimmed = { ...composition, clips: [ { ...composition.clips[0], segment: { ...a, trimEnd: 2 } }, composition.clips[1] ] };
  expect(subtitleSourceMatches(owners.a, trimmed, 'b', b, 'b.mp4', '')).toBe(false);
  const audio = { id: 'audio', rawAudioPath: 'audio.wav', name: 'audio', duration: 10, startTime: 4, inPoint: 1, outPoint: 9, playbackRate: 1, addedAt: 4 };
  const audioComposition = { ...composition, audioSegments: [audio] };
  const audioPlan = {
    clips: [{ clipId: 'audio:audio', sourcePath: 'audio.wav', sourceDuration: 10, trimSegments: [] }],
    sourceTypeForNative: 'audio', replacementRangesByClip: {}, indicator: {},
    clipTransformsByClip: { 'audio:audio': { kind: 'audio', audioSegmentId: 'audio', sourceName: 'audio', sourcePath: 'audio.wav', timelineOffsetSec: 3, sourceLocalOffsetSec: 1 } },
  } as SubtitleGenerationPlan;
  const owner = captureSubtitleSourceOwners(audioPlan, b, audioComposition)['audio:audio'];
  expect(subtitleSourceMatches(owner, audioComposition, 'b', b, 'b.mp4', '')).toBe(true);
  for (const patch of [{ playbackRate: 2 }, { inPoint: 2 }, { outPoint: 8 }, { startTime: 5 }, { duration: 11 }, { rawAudioPath: 'new.wav' }]) {
    expect(subtitleSourceMatches(owner, { ...audioComposition, audioSegments: [{ ...audio, ...patch }] }, 'b', b, 'b.mp4', '')).toBe(false);
  }
});
