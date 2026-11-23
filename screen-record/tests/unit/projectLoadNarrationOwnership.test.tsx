import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useProjects, type ProjectLoadResult } from '@/hooks/useProjects';
import { activateNarrationProject, beginNarrationProjectLoad, narrationProjectEpoch, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { createSyntheticProjectFixture } from '@/testHarness/syntheticProject';
import { projectManager } from '@/lib/projectManager';
import type { Project } from '@/types/video';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { invoke } from '@/lib/ipc';
import { useCloseProject } from '@/hooks/useCloseProject';
import { importedProjectOwner } from '@/hooks/importProjectOwnership';
import { assertAudioImportWriteAllowed, beginAudioImportWrite, finishAudioImportWrite, markAudioImportCommitted } from '@/lib/audioImportWriteFence';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined), logToHost: vi.fn() }));
vi.mock('@/lib/projectManager', () => ({ projectManager: {
  loadProject: vi.fn(), getProjects: vi.fn(async () => []), setActiveProjectId: vi.fn(), invalidateEditorWrites: vi.fn(),
} }));
vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: { projectSaveFailed: 'save failed' } }) }));
vi.mock('@/hooks/useProjects/loadProjectMedia', () => ({
  loadProjectVideo: vi.fn(async () => ''),
  loadProjectAudioMedia: vi.fn(async () => ({})),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}

function harness(owner: { current: string | null }, persist: () => Promise<void> = async () => {}) {
  const noop = vi.fn();
  const loaded = vi.fn((project: Project) => { activateNarrationProject(owner, project.id); return importedProjectOwner(owner, project.id); });
  const project = createSyntheticProjectFixture();
  const hook = renderHook(() => {
    const projects = useProjects({
    videoControllerRef: { current: undefined }, setCurrentVideo: noop, setCurrentAudio: noop,
    setCurrentMicAudio: noop, setCurrentWebcamVideo: noop, setSegment: noop,
    setBackgroundConfig: noop, setMousePositions: noop, setThumbnails: noop,
    onProjectLoadStart: (projectId) => { assertAudioImportWriteAllowed(owner.current ?? '', true);
      assertAudioImportWriteAllowed(projectId, true); return beginNarrationProjectLoad(owner); }, onProjectLoaded: loaded,
    currentVideo: null, currentAudio: null, currentMicAudio: null,
  });
    const close = useCloseProject(new Proxy({
      projects, currentProjectId: project.id, currentProjectIdRef: owner,
      currentProjectDataRef: { current: project }, historyProjectResetRef: { current: project.id },
      isRecording: false, isProcessing: false, currentVideo: null, currentAudio: null,
      currentMicAudio: null, currentWebcamVideo: null,
      persistRef: { current: persist },
      editorHistory: { withoutHistory: (callback: () => void) => callback(), resetHistory: noop },
    }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useCloseProject>[0]);
    return { projects, close, subtitle: useSubtitleGeneration({
    t: {} as never, projectResetKey: project.id, currentProjectIdRef: owner,
    segment: project.segment, setSegment: noop, setSegmentSilently: noop,
    composition: null, setComposition: noop, setCompositionSilently: noop,
    activeClipId: null, loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 10, setActivePanel: noop,
  }) };
  });
  return { ...hook, loaded };
}

it('rejects loading a fenced target even when a different project is current', async () => {
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, 'other-project');
  const fence = beginAudioImportWrite('fenced-project');
  markAudioImportCommitted('fenced-project', fence);
  const hook = harness(owner);
  await act(async () => { await expect(hook.result.current.projects.handleLoadProject('fenced-project')).rejects.toThrow(); });
  expect(owner.current).toBe('other-project');
  expect(hook.loaded).not.toHaveBeenCalled();
  finishAudioImportWrite(fence, true); hook.unmount(); setNarrationProjectId(owner, null);
});

it('refuses to close a project while committed audio recovery protects unsaved edits', async () => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const fence = beginAudioImportWrite(project.id);
  markAudioImportCommitted(project.id, fence);
  const hook = harness(owner);
  await act(async () => { expect(await hook.result.current.close()).toBe(false); });
  expect(owner.current).toBe(project.id);
  finishAudioImportWrite(fence, true);
  hook.unmount();
});

it('rechecks the audio fence when a pending close save completes', async () => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const save = deferred<void>();
  const hook = harness(owner, () => save.promise);
  let closing!: Promise<boolean>;
  act(() => { closing = hook.result.current.close(); });
  const fence = beginAudioImportWrite(project.id);
  markAudioImportCommitted(project.id, fence);
  await act(async () => { save.resolve(); expect(await closing).toBe(false); });
  expect(owner.current).toBe(project.id);
  finishAudioImportWrite(fence, true);
  hook.unmount();
});

it('suspends the prior owner before a same-project load reads its snapshot', async () => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const oldEpoch = narrationProjectEpoch(owner);
  const pending = deferred<Project>();
  vi.mocked(projectManager.loadProject).mockReturnValueOnce(pending.promise);
  const hook = harness(owner);
  let load!: Promise<ProjectLoadResult>;
  act(() => { load = hook.result.current.projects.handleLoadProject(project.id); });
  expect(owner.current).toBeNull();
  expect(narrationProjectEpoch(owner)).toBeGreaterThan(oldEpoch);
  act(() => setNarrationProjectId(owner, project.id));
  expect(owner.current).toBeNull();
  await act(async () => { await hook.result.current.subtitle.handleGenerateSubtitles(); });
  expect(invoke).not.toHaveBeenCalledWith('start_subtitle_generation', expect.anything());
  await act(async () => { pending.resolve(project); await load; });
  expect(owner.current).toBe(project.id);
  expect(narrationProjectEpoch(owner)).toBeGreaterThan(oldEpoch);
  expect(hook.loaded).toHaveBeenCalledOnce();
  hook.unmount();
});

it.each(['success', 'failure'] as const)('keeps the project closed after a delayed load %s', async (outcome) => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const pending = deferred<Project>();
  vi.mocked(projectManager.loadProject).mockReturnValueOnce(pending.promise);
  const hook = harness(owner);
  let load!: Promise<ProjectLoadResult>;
  act(() => { load = hook.result.current.projects.handleLoadProject(project.id); });
  await act(async () => { expect(await hook.result.current.close()).toBe(true); });
  expect(owner.current).toBeNull();
  await act(async () => {
    if (outcome === 'success') pending.resolve(project); else pending.reject(new Error('late failure'));
    expect(await load).toBeNull();
  });
  expect(owner.current).toBeNull();
  expect(hook.loaded).not.toHaveBeenCalled();
  hook.unmount();
});

it('supersedes a load started while close is persisting', async () => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const save = deferred<void>();
  const pending = deferred<Project>();
  vi.mocked(projectManager.loadProject).mockReturnValueOnce(pending.promise);
  const hook = harness(owner, () => save.promise);
  let closing!: Promise<boolean>;
  act(() => { closing = hook.result.current.close(); });
  let load!: Promise<ProjectLoadResult>;
  act(() => { load = hook.result.current.projects.handleLoadProject(project.id); });
  await act(async () => { save.resolve(); expect(await closing).toBe(true); });
  await act(async () => { pending.resolve(project); expect(await load).toBeNull(); });
  expect(owner.current).toBeNull();
  expect(hook.loaded).not.toHaveBeenCalled();
  hook.unmount();
});

it('keeps the original owner when closing fails to save and still rejects the old load', async () => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const pending = deferred<Project>();
  vi.mocked(projectManager.loadProject).mockReturnValueOnce(pending.promise);
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  const hook = harness(owner, async () => { throw new Error('save failed'); });
  let load!: Promise<ProjectLoadResult>;
  act(() => { load = hook.result.current.projects.handleLoadProject(project.id); });
  await act(async () => { expect(await hook.result.current.close()).toBe(false); });
  expect(owner.current).toBe(project.id);
  await act(async () => { pending.resolve(project); expect(await load).toBeNull(); });
  expect(owner.current).toBe(project.id);
  expect(hook.loaded).not.toHaveBeenCalled();
  error.mockRestore();
  hook.unmount();
});

it('keeps the owner suspended across competing attempts and restores it after the latest failure', async () => {
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, 'old');
  const oldEpoch = narrationProjectEpoch(owner);
  const first = deferred<Project>();
  const second = deferred<Project>();
  vi.mocked(projectManager.loadProject).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const hook = harness(owner);
  let loadFirst!: Promise<ProjectLoadResult>;
  let loadSecond!: Promise<ProjectLoadResult>;
  act(() => {
    loadFirst = hook.result.current.projects.handleLoadProject('first');
    loadSecond = hook.result.current.projects.handleLoadProject('second');
  });
  await act(async () => { first.resolve(createSyntheticProjectFixture()); expect(await loadFirst).toBeNull(); });
  expect(owner.current).toBeNull();
  expect(hook.loaded).not.toHaveBeenCalled();
  await act(async () => { second.reject(new Error('load failed')); await expect(loadSecond).rejects.toThrow('load failed'); });
  expect(owner.current).toBe('old');
  expect(narrationProjectEpoch(owner)).toBeGreaterThan(oldEpoch);
  hook.unmount();
});

it('returns the exact applied activation even if the same ID is reopened before the caller resumes', async () => {
  const project = createSyntheticProjectFixture();
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  vi.mocked(projectManager.loadProject).mockResolvedValueOnce(project);
  const hook = harness(owner);
  let loadedOwner!: ProjectLoadResult;
  await act(async () => {
    loadedOwner = await hook.result.current.projects.handleLoadProject(project.id).then((result) => {
      expect(typeof result).toBe('function');
      expect((result as () => boolean)()).toBe(true);
      activateNarrationProject(owner, project.id);
      return result;
    });
  });
  expect((loadedOwner as () => boolean)()).toBe(false);
  hook.unmount();
});
