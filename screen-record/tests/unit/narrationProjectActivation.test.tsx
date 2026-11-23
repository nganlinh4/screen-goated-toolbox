import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import en from '@/i18n/en';
import { invoke } from '@/lib/ipc';
import { useProjectActions } from '@/hooks/useProjectActions';
import { useSubtitleNarration } from '@/hooks/useSubtitleNarration';
import { useS2sNarration } from '@/hooks/useS2sNarration';
import { useEditorMainTimelineState } from '@/components/useEditorMainTimelineState';
import { narrationProjectEpoch, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { createSyntheticProjectFixture } from '@/testHarness/syntheticProject';
import { mockNarrationInvoke, mockNarrationSourceSnapshot } from './narrationIpcMock';
import type { NarrationProfilePayload } from '@/hooks/useNarrationSettings';
import type { Project } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));
vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: en }) }));
vi.mock('@/hooks/useSubtitleTranslation', () => ({ useSubtitleTranslation: () => ({}) }));

it('retires the prior alignment and pending narration when the project load callback reloads the same ID', async () => {
  const project = createSyntheticProjectFixture({ subtitleCount: 1, narrationCount: 1, audioCount: 0 });
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const originalEpoch = narrationProjectEpoch(owner);
  const projectRef = { current: project as Project | null };
  const noop = vi.fn();
  const args = new Proxy({
    projects: { currentProjectId: project.id, showProjectsDialog: false, setShowProjectsDialog: noop, handleLoadProject: noop },
    currentProjectIdRef: owner, currentProjectDataRef: projectRef, currentProjectData: project,
    backgroundConfig: project.backgroundConfig, segment: project.segment,
    clipExportSourcePathCacheRef: { current: new Map() }, clipExportWebcamPathCacheRef: { current: new Map() },
    isSwitchingCompositionClipRef: { current: false }, clipLoadRequestSeqRef: { current: 0 },
    spreadAnimationTimerRef: { current: null }, currentVideo: null, currentAudio: null,
    currentMicAudio: null, currentWebcamVideo: null,
  }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useProjectActions>[0];
  let resolveStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { resolveStart = resolve; });
  let resolveS2sStart!: (value: { jobId: string }) => void;
  const s2sStart = new Promise<{ jobId: string }>((resolve) => { resolveS2sStart = resolve; });
  mockNarrationInvoke((command) => command === 'start_subtitle_narration' ? start
    : command === 'start_s2s_narration' ? s2sStart : Promise.resolve(undefined));
  const onApply = vi.fn();
  const onS2sSubtitles = vi.fn();
  const setSegment = vi.fn();
  const source = () => ({ composition: projectRef.current?.composition ?? null, segment: projectRef.current?.segment ?? null,
    loadedClipId: 'root', videoPath: project.rawVideoPath ?? '', micPath: '' });
  const hook = renderHook(() => ({
    actions: useProjectActions(args),
    job: useSubtitleNarration({ projectId: project.id, currentProjectIdRef: owner, t: en,
      visibleSubtitles: [{ id: 'cue', text: 'Hello', startTime: 0, endTime: 1 }],
      profile: {} as NarrationProfilePayload, onApplyNarrationSegments: onApply, onFinalizeNarrationSegments: vi.fn() }),
    s2s: useS2sNarration({ projectId: project.id, currentProjectIdRef: owner, t: en,
      getNarrationSourceSnapshot: mockNarrationSourceSnapshot, duration: 3, sourceType: 'video', targetLanguage: 'en',
      geminiModel: 'm', geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25,
      targetSegmentSec: 4, onApplyNarrationSegments: vi.fn(), onPopulateEmptySubtitles: onS2sSubtitles, onFinalize: vi.fn() }),
    editor: useEditorMainTimelineState({ t: en, activePanel: 'audio', setActivePanel: noop,
      segment: project.segment, setSegment, composition: project.composition ?? null, setComposition: noop,
      currentTime: 0, duration: project.duration ?? 30, editingSubtitleId: null, setEditingKeyframeId: noop,
      setEditingTextId: noop, setEditingSubtitleId: noop, setEditingKeystrokeSegmentId: noop, setEditingPointerId: noop,
      projectResetKey: project.id, currentProjectIdRef: owner, renderedProjectEpoch: originalEpoch,
      loadedClipId: 'root', currentRawVideoPath: project.rawVideoPath ?? '', getNarrationSourceSnapshot: source,
      narrationSegments: project.composition?.narrationSegments, beginBatch: noop, commitBatch: noop }),
  }));
  act(() => hook.result.current.editor.handleNarrationSelectionChange(['synthetic-narration-0']));
  let pending!: Promise<void>;
  let pendingS2s!: Promise<void>;
  await act(async () => {
    pending = hook.result.current.job.handleGenerateNarration();
    pendingS2s = hook.result.current.s2s.handleGenerate();
    await Promise.resolve();
  });
  // A routine synchronization is not a new activation.
  setNarrationProjectId(owner, project.id);
  expect(narrationProjectEpoch(owner)).toBe(originalEpoch);
  act(() => hook.result.current.actions.onProjectLoaded(project));
  expect(narrationProjectEpoch(owner)).toBe(originalEpoch + 1);
  act(() => hook.result.current.editor.handleAlignSubtitlesToNarration());
  expect(setSegment).not.toHaveBeenCalled();
  await act(async () => {
    resolveStart({ jobId: 'old-job' }); resolveS2sStart({ jobId: 'old-s2s' });
    await Promise.all([pending, pendingS2s]);
  });
  expect(invoke).toHaveBeenCalledWith('cancel_subtitle_narration', { jobId: 'old-job' });
  expect(invoke).toHaveBeenCalledWith('cancel_s2s_narration', { jobId: 'old-s2s' });
  expect(invoke).not.toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything());
  expect(onApply).toHaveBeenCalledTimes(1);
  expect(onS2sSubtitles).not.toHaveBeenCalled();
  hook.unmount(); setNarrationProjectId(owner, null);
});
