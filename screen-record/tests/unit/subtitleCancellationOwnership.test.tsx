import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useRef, useState } from 'react';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { useAppHistoryState } from '@/hooks/useAppHistoryState';
import { clearResumableRun, readResumableRun, saveResumableRun } from '@/hooks/resumableJobRegistry';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import { DEFAULT_WEBCAM_CONFIG } from '@/lib/webcam';
import type { SubtitleJobStatus } from '@/hooks/subtitleGenerationTypes';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';
import { getRetiringSubtitleGenerations, releaseRetiringSubtitleGeneration } from '@/hooks/retiredSubtitleGeneration';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@/lib/ipc', () => ipc);
vi.mock('@/hooks/subtitleSourceOwnership', () => ({
  subtitleSourceMatches: () => true,
  captureSubtitleSourceOwners: () => ({}),
}));

afterEach(() => {
  clearResumableRun('subtitle-generation');
  getRetiringSubtitleGenerations().forEach(releaseRetiringSubtitleGeneration);
  vi.clearAllMocks();
  vi.useRealTimers();
});

it('retires a generation before undo so a delayed result cannot restore it', async () => {
  vi.useFakeTimers();
  let finishPoll!: (status: SubtitleJobStatus) => void;
  const latePoll = new Promise<SubtitleJobStatus>((resolve) => { finishPoll = resolve; });
  let polls = 0;
  ipc.invoke.mockImplementation((command: string) => {
    if (command === 'get_subtitle_generation_status') {
      polls += 1;
      if (polls > 1) return latePoll;
      return Promise.resolve({ state: 'running', message: '', progress: 0.5, activeClipId: 'clip',
        totalClips: 1, completedClips: 0, resultsRevision: 1, skipped: [],
        results: [{ clipId: 'clip', isPartial: true, segments: [{ startTime: 1, endTime: 2, text: 'partial' }] }],
      });
    }
    if (command === 'cancel_subtitle_generation') return Promise.resolve();
    return Promise.resolve({ methods: [] });
  });
  saveResumableRun('subtitle-generation', {
    projectEpoch: 0, projectKey: 'p', jobId: 'job', jobContext: {
      indicator: {}, sourceTypeForNative: 'video', replacementRangesByClip: {}, clipTransformsByClip: {},
    }, status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
  });
  const original = { trimStart: 0, trimEnd: 10, subtitleSegments: [] } as VideoSegment;
  const initialComposition = { mode: 'unified', selectedClipId: 'clip', focusedClipId: 'clip',
    clips: [{ id: 'clip', duration: 10, segment: original }] } as ProjectComposition;
  const { result } = renderHook(() => {
    const [segment, rawSetSegment] = useState<VideoSegment | null>(original);
    const [composition, rawSetComposition] = useState<ProjectComposition | null>(initialComposition);
    const segmentRef = useRef<VideoSegment | null>(original);
    const retire = useRef<() => void>(() => {});
    const projectRef = useRef<Project | null>({
      id: 'p', name: 'Project', createdAt: 1, lastModified: 1, duration: 10,
      segment: original, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
      webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [], composition: initialComposition,
    });
    const history = useAppHistoryState({
      onHistoryRestore: () => retire.current(),
      backgroundConfig: DEFAULT_BACKGROUND_CONFIG, backgroundConfigRef: { current: DEFAULT_BACKGROUND_CONFIG },
      webcamConfig: DEFAULT_WEBCAM_CONFIG, webcamConfigRef: { current: DEFAULT_WEBCAM_CONFIG },
      composition, currentProjectDataRef: projectRef, currentRawMicAudioPath: '', currentRawVideoPath: '',
      currentRawWebcamVideoPath: '', currentRecordingMode: 'withoutCursor', duration: 10,
      handleProjectRawVideoPathChange: vi.fn(), isPlaying: false, isPlayingRef: { current: false },
      isProjectTransitionRef: { current: false }, pendingSilentSegmentRef: { current: null },
      pendingSilentSegmentTimerRef: { current: null }, rawSetBackgroundConfig: vi.fn(), rawSetComposition,
      rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
      rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(),
      rawSetSegment, rawSetWebcamConfig: vi.fn(), segment, segmentRef,
      setBackgroundConfigState: vi.fn(), setCurrentProjectData: vi.fn(),
      setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
    });
    const generation = useSubtitleGeneration({
      t: { subtitleStatusCancelled: 'Cancelled', subtitleStatusCancelUnconfirmed: 'May still be running' } as never,
      projectResetKey: 'p', segment, setSegment: history.setSegment, setSegmentSilently: history.setSegmentSilently,
      composition, setComposition: history.setComposition, setCompositionSilently: history.setCompositionSilently,
      activeClipId: 'clip', loadedClipIdRef: { current: 'clip' }, currentRawVideoPath: '', currentRawMicAudioPath: '',
      duration: 10, setActivePanel: vi.fn(),
    });
    retire.current = generation.retireSubtitleForHistory;
    return { segment, composition, history, generation };
  });
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  await act(async () => { await vi.advanceTimersByTimeAsync(2501); });
  expect(result.current.segment?.subtitleSegments).toEqual(expect.arrayContaining([
    expect.objectContaining({ text: 'partial' }),
  ]));
  act(() => result.current.history.undo());
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'job' });
  expect(result.current.segment?.subtitleSegments).toEqual([]);
  await act(async () => {
    finishPoll({ state: 'completed', message: '', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 2, skipped: [],
      results: [{ clipId: 'clip', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'late' }] }],
    });
    await Promise.resolve();
  });
  expect(result.current.segment?.subtitleSegments).toEqual([]);
  expect(result.current.composition?.clips[0].segment.subtitleSegments).toEqual([]);
});

it('retires an in-flight subtitle result before backend cancellation completes', async () => {
  let finishPoll!: (status: SubtitleJobStatus) => void;
  let finishCancel!: () => void;
  const poll = new Promise<SubtitleJobStatus>((resolve) => { finishPoll = resolve; });
  const cancel = new Promise<void>((resolve) => { finishCancel = resolve; });
  ipc.invoke.mockImplementation((command: string, args?: { jobId?: string }) => {
    if (command === 'get_subtitle_generation_status') {
      return args?.jobId === 'job' ? poll : new Promise(() => {});
    }
    if (command === 'cancel_subtitle_generation') return cancel;
    return Promise.resolve({ methods: [] });
  });
  saveResumableRun('subtitle-generation', {
    projectEpoch: 0, projectKey: 'p', jobId: 'job', jobContext: {
      indicator: {}, sourceTypeForNative: 'video', replacementRangesByClip: {}, clipTransformsByClip: {},
    }, status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
  });
  const setSegment = vi.fn();
  const setComposition = vi.fn();
  const useGeneration = () => useSubtitleGeneration({
    t: { subtitleStatusCancelled: 'Cancelled' } as never, projectResetKey: 'p',
    segment: { trimStart: 0, trimEnd: 10, subtitleSegments: [] } as VideoSegment,
    setSegment, setSegmentSilently: vi.fn(), composition: null, setComposition, setCompositionSilently: setComposition,
    activeClipId: null, loadedClipIdRef: { current: null }, currentRawVideoPath: '',
    currentRawMicAudioPath: '', duration: 10, setActivePanel: vi.fn(),
  });
  const { result, unmount } = renderHook(useGeneration);
  await act(async () => { await Promise.resolve(); });
  expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.anything());
  act(() => { void result.current.handleCancelSubtitleGeneration(); });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'job' });
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
  await act(async () => {
    finishPoll({ state: 'completed', message: '', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
      results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'late' }] }],
    });
    await Promise.resolve();
  });
  expect(setSegment).not.toHaveBeenCalled();
  expect(setComposition).not.toHaveBeenCalled();
  await act(async () => {
    saveResumableRun('subtitle-generation', {
      projectEpoch: 0, projectKey: 'p', jobId: 'new-job', jobContext: null, status: null,
      lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
    });
    await Promise.resolve();
  });
  await act(async () => { finishCancel(); await cancel; });
  expect(readResumableRun<{ jobId: string }>('subtitle-generation')?.jobId).toBe('new-job');
  unmount();
  const remounted = renderHook(useGeneration);
  await act(async () => { await Promise.resolve(); });
  const polledJobs = ipc.invoke.mock.calls.filter(([command]) => command === 'get_subtitle_generation_status')
    .map(([, args]) => args.jobId);
  expect(polledJobs.filter((jobId) => jobId === 'job')).toHaveLength(1);
  expect(polledJobs).toContain('new-job');
  remounted.unmount();
});

it('reports unconfirmed cancellation without applying the retired result', async () => {
  let finishPoll!: (status: SubtitleJobStatus) => void;
  const poll = new Promise<SubtitleJobStatus>((resolve) => { finishPoll = resolve; });
  ipc.invoke.mockImplementation((command: string) => {
    if (command === 'get_subtitle_generation_status') return poll;
    if (command === 'cancel_subtitle_generation') return Promise.reject(new Error('connection lost'));
    return Promise.resolve({ methods: [] });
  });
  saveResumableRun('subtitle-generation', {
    projectEpoch: 0, projectKey: 'p', jobId: 'job', jobContext: {
      indicator: {}, sourceTypeForNative: 'video', replacementRangesByClip: {}, clipTransformsByClip: {},
    }, status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
  });
  const setSegment = vi.fn();
  const setComposition = vi.fn();
  const { result } = renderHook(() => useSubtitleGeneration({
    t: { subtitleStatusCancelled: 'Cancelled', subtitleStatusCancelUnconfirmed: 'May still be running' } as never,
    projectResetKey: 'p',
    segment: { trimStart: 0, trimEnd: 10, subtitleSegments: [] } as VideoSegment,
    setSegment, setSegmentSilently: vi.fn(), composition: null, setComposition, setCompositionSilently: setComposition,
    activeClipId: null, loadedClipIdRef: { current: null }, currentRawVideoPath: '',
    currentRawMicAudioPath: '', duration: 10, setActivePanel: vi.fn(),
  }));
  await act(async () => { await Promise.resolve(); });
  await act(async () => { await result.current.handleCancelSubtitleGeneration(); });
  expect(result.current.isGeneratingSubtitles).toBe(false);
  expect(result.current.subtitleStatusMessage).toBe('May still be running');
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
  await act(async () => {
    finishPoll({ state: 'completed', message: '', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
      results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'late' }] }],
    });
    await Promise.resolve();
  });
  expect(setSegment).not.toHaveBeenCalled();
  expect(setComposition).not.toHaveBeenCalled();
});

it('retires an in-flight start on history restore and cancels its eventual worker job', async () => {
  const ticket = 'ticket-0123456789abcdef0123456789abcdef-transcription-1';
  let finishStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { finishStart = resolve; });
  ipc.invoke.mockImplementation((command: string) => {
    if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: ticket });
    if (command === 'start_subtitle_generation') return start;
    if (command === 'cancel_subtitle_generation') return Promise.resolve();
    return Promise.resolve({ methods: [] });
  });
  const { result } = renderHook(() => useSubtitleGeneration({
    t: { subtitleStatusCancelUnconfirmed: 'May still be running' } as never,
    projectResetKey: 'p',
    segment: { trimStart: 0, trimEnd: 10, trimSegments: [{ id: 'trim', startTime: 0, endTime: 10 }], subtitleSegments: [] } as VideoSegment,
    setSegment: vi.fn(), setSegmentSilently: vi.fn(), composition: null,
    setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: null, loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 10, setActivePanel: vi.fn(),
  }));
  act(() => result.current.setSubtitleSource('video'));
  let generating!: Promise<void>;
  act(() => { generating = result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything()));
  expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything());
  act(() => result.current.retireSubtitleForHistory());
  expect(result.current.isGeneratingSubtitles).toBe(false);
  expect(result.current.subtitleStatusMessage).toBe('May still be running');
  await act(async () => { finishStart({ jobId: ticket }); await generating; });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
  expect(ipc.invoke).not.toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: ticket }));
});
