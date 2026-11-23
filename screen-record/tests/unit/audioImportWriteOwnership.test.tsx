import 'fake-indexeddb/auto';
import { act, fireEvent, render, renderHook, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useAppMediaImports } from '@/hooks/useAppMediaImports';
import { useTimelineWorkspaceComposition } from '@/hooks/useTimelineWorkspaceComposition';
import { activateNarrationProject, beginNarrationProjectLoad, closeNarrationProject, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { projectManager } from '@/lib/projectManager';
import { idbCreateProjectBundle, idbGet, PROJECTS_STORE } from '@/lib/projectStorage';
import { createSyntheticProjectFixture } from '@/testHarness/syntheticProject';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';
import { invalidateSubtitleSaveEditor, registerSubtitleSaveEditor } from '@/hooks/subtitleCompletedPersistence';
import { getMediaServerUrl } from '@/lib/mediaServer';
import { invoke } from '@/lib/ipc';
import { AudioImportRecoveryNotice } from '@/components/AudioImportRecoveryNotice';
import { clearAudioImportRecovery, retryAudioImportRecovery, useAudioImportRecovery } from '@/hooks/audioImportRecovery';
import { finishAudioImportWrite, isAudioImportWriteAllowed } from '@/lib/audioImportWriteFence';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined), logToHost: vi.fn() }));
vi.mock('@/hooks/useSettings', async () => {
  const { default: translations } = await import('@/i18n/en');
  return { useSettings: () => ({ t: translations }) };
});
vi.mock('@/lib/mediaServer', async (load) => ({ ...await load<typeof import('@/lib/mediaServer')>(),
  createAudioPlaceholderVideo: vi.fn(async () => ({ path: 'new-placeholder.mp4' })),
  importAudioPathToManagedMediaFile: vi.fn(async () => ({ path: 'new-audio.wav', duration: 4 })),
  getMediaServerUrl: vi.fn(async () => 'media-url'),
}));
afterEach(() => vi.restoreAllMocks());

it.each((['placeholder', 'ordinary'] as const).flatMap((kind) =>
  (['reload', 'close', 'edit', 'after-commit', 'after-commit-close', 'retry-failure', 'retry-unmounted', 'retry-remount', 'retry-reactivated', 'continual-edits', 'failure', 'unchanged', 'preview-failure', 'preview-stale'] as const)
    .filter((change) => !change.startsWith('preview-') || kind === 'placeholder').map((change) => ({ kind, change }))))(
  'preserves $kind audio import transaction ownership on $change', async ({ kind, change }) => {
    const fixture = createSyntheticProjectFixture({ subtitleCount: 1, narrationCount: 0, audioCount: 0 });
    const id = `audio-write-${kind}-${change}`;
    const composition = { ...fixture.composition!, placeholderVideoForAudio: kind === 'placeholder', audioSegments: [] };
    const project = { ...fixture, id, name: id, composition };
    await projectManager.getProjects();
    await idbCreateProjectBundle(project);
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, id);
    const unregister = registerSubtitleSaveEditor(owner);
    const projectRef = { current: project as Project | null };
    const segmentRef = { current: project.segment as VideoSegment | null };
    const setComposition = (value: ProjectComposition | null | ((previous: ProjectComposition | null) => ProjectComposition | null)) => {
      const next = typeof value === 'function' ? value(projectRef.current?.composition ?? null) : value;
      if (projectRef.current && next) projectRef.current = { ...projectRef.current, composition: next };
    };
    const setSegment = (value: VideoSegment | null | ((previous: VideoSegment | null) => VideoSegment | null)) => {
      segmentRef.current = typeof value === 'function' ? value(segmentRef.current) : value;
    };
    const noop = vi.fn();
    const setError = vi.fn();
    if (change === 'preview-failure') vi.mocked(getMediaServerUrl).mockRejectedValueOnce(new Error('media unavailable'));
    const logError = vi.spyOn(console, 'error').mockImplementation(() => {});
    const logWarning = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const setVideo = vi.fn();
    const loadedClipIdRef = { current: 'root' as string | null };
    if (change === 'preview-stale') vi.mocked(getMediaServerUrl).mockImplementationOnce(async () => {
      loadedClipIdRef.current = 'other';
      return 'media-url';
    });
    const put = IDBObjectStore.prototype.put;
    let invalidated = false;
    const putSpy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
      if (change === 'failure' && this.name === PROJECTS_STORE && (value as { id?: string }).id === id) {
        throw new DOMException('write failed', 'AbortError');
      }
      const request = key === undefined ? put.call(this, value) : put.call(this, value, key);
      if (change !== 'unchanged' && !change.startsWith('preview-') && change !== 'failure' && !change.startsWith('after-commit') && !change.startsWith('retry-') && change !== 'continual-edits' && !invalidated && this.name === PROJECTS_STORE && (value as { id?: string }).id === id) {
        invalidated = true;
        if (change === 'reload') {
          const finish = beginNarrationProjectLoad(owner);
          activateNarrationProject(owner, id);
          finish(true);
        } else if (change === 'close') closeNarrationProject(owner);
        else {
          projectRef.current = { ...projectRef.current!, composition: { ...projectRef.current!.composition!, focusedClipId: 'later-edit' } };
          invalidateSubtitleSaveEditor(owner);
        }
      }
      return request;
    });
    const updateProject = projectManager.updateProject.bind(projectManager);
    const update = vi.spyOn(projectManager, 'updateProject').mockImplementation(async (...args) => {
      if (change.startsWith('retry-') && invalidated) throw new Error('retry failed');
      const applied = await updateProject(...args);
      if ((change === 'after-commit' || change === 'after-commit-close' || change.startsWith('retry-')) && applied && !invalidated) {
        invalidated = true;
        if (change === 'after-commit-close') closeNarrationProject(owner);
        else {
          projectRef.current = { ...projectRef.current!, composition: { ...projectRef.current!.composition!, focusedClipId: 'later-edit' } };
          invalidateSubtitleSaveEditor(owner);
        }
      }
      if (change === 'continual-edits' && applied) {
        invalidated = true;
        projectRef.current = { ...projectRef.current!, composition: { ...projectRef.current!.composition!, focusedClipId: 'later-edit' } };
        invalidateSubtitleSaveEditor(owner);
      }
      return applied;
    });
    const useTestImporter = () => {
      const workspace = useTimelineWorkspaceComposition({ composition, currentProjectDataRef: projectRef,
        currentProjectId: id, currentProjectIdRef: owner, currentTime: 0, duration: project.duration ?? 30,
        editorHistory: { replaceSnapshot: noop }, handleEditorRawVideoPathChange: noop,
        isPlaceholderBackedProject: kind === 'placeholder', isPlaying: false, loadProjects: async () => [],
        rawSetComposition: setComposition, rawSetSegment: setSegment, segmentRef, setComposition,
        setCompositionSilently: setComposition, setCurrentProjectData: noop, setCurrentVideo: setVideo,
        setEditorPreviewDuration: noop, setSegment, videoControllerRef: { current: undefined }, videoRef: { current: null } });
      return useAppMediaImports({ composition, currentProjectDataRef: projectRef, currentProjectIdRef: owner,
        currentTime: 0, duration: project.duration ?? 30, isPlaceholderBackedProject: kind === 'placeholder', loadedClipIdRef,
        persistTimelineWorkspaceState: workspace.persistTimelineWorkspaceState,
        updateCurrentMusicSegments: workspace.updateCurrentMusicSegments,
        projects: { handleLoadProject: async () => false, loadProjects: async () => [], setShowProjectsDialog: noop },
        segment: project.segment, segmentRef, selectedSubtitleIdsRef: { current: [] }, selectedTextIdsRef: { current: [] },
        setActivePanel: noop, setComposition, setCurrentVideo: setVideo, setEditingSubtitleId: noop,
        setError, setSegment, setSubtitleSource: noop, videoControllerRef: { current: undefined } });
    };
    let hook = renderHook(useTestImporter);
    await act(async () => { await hook.result.current.importAudioPaths(['audio.wav']); });
    await waitFor(() => expect(update).toHaveBeenCalledTimes(change === 'continual-edits' ? 3 : change === 'after-commit' || change.startsWith('retry-') ? 2 : 1));
    await act(async () => { await Promise.resolve(update.mock.results[0].value).catch(() => undefined); });
    expect(invalidated).toBe(change !== 'unchanged' && !change.startsWith('preview-') && change !== 'failure');
    const stored = await idbGet(PROJECTS_STORE, id);
    expect(stored?.composition?.audioSegments).toHaveLength(['unchanged', 'after-commit', 'after-commit-close', 'retry-failure', 'retry-unmounted', 'retry-remount', 'retry-reactivated', 'continual-edits', 'preview-failure', 'preview-stale'].includes(change) ? 1 : 0);
    expect(stored?.rawVideoPath).toBe(['unchanged', 'after-commit', 'after-commit-close', 'retry-failure', 'retry-unmounted', 'retry-remount', 'retry-reactivated', 'continual-edits', 'preview-failure', 'preview-stale'].includes(change) && kind === 'placeholder' ? 'new-placeholder.mp4' : project.rawVideoPath);
    expect(setVideo).toHaveBeenCalledTimes((change === 'unchanged' || change === 'after-commit') && kind === 'placeholder' ? 1 : 0);
    expect(setError).toHaveBeenCalledTimes(change === 'unchanged' || change === 'after-commit' ? 0 : 1);
    if (change === 'edit' || change === 'after-commit') expect(projectRef.current?.composition?.focusedClipId).toBe('later-edit');
    if (change === 'after-commit-close' || change.startsWith('retry-') || change === 'continual-edits') expect(setError).toHaveBeenCalledWith(expect.stringContaining('Audio was saved'));
    if (change.startsWith('preview-')) expect(setError).toHaveBeenCalledWith(expect.stringContaining('Audio was saved, but its preview'));
    expect(projectRef.current?.composition?.audioSegments).toHaveLength(['unchanged', 'preview-failure', 'preview-stale', 'after-commit'].includes(change) ? 1 : 0);
    expect(projectRef.current?.rawVideoPath).toBe(['unchanged', 'preview-failure', 'preview-stale', 'after-commit'].includes(change) && kind === 'placeholder' ? 'new-placeholder.mp4' : project.rawVideoPath);
    expect(invoke).not.toHaveBeenCalledWith('delete_file', { path: 'new-placeholder.mp4' });
    putSpy.mockRestore();
    update.mockRestore();
    if (change === 'retry-failure' || change === 'continual-edits') {
      await expect(projectManager.updateProject(id, { composition: projectRef.current?.composition,
        segment: segmentRef.current ?? undefined, rawVideoPath: projectRef.current?.rawVideoPath }))
        .rejects.toThrow('later edits are not saved');
      const afterRejectedAutosave = await idbGet(PROJECTS_STORE, id);
      expect(afterRejectedAutosave?.composition?.audioSegments).toHaveLength(1);
      const reloaded = await projectManager.loadProject(id);
      expect(reloaded?.composition?.audioSegments).toHaveLength(1);
      expect(projectRef.current?.composition?.focusedClipId).toBe('later-edit');
      const notice = render(<AudioImportRecoveryNotice />);
      const item = notice.getByText(id).parentElement!;
      fireEvent.click(within(item).getByRole('button', { name: 'Retry save' }));
      await waitFor(() => expect(notice.queryByText(id)).toBeNull());
      const afterRetry = await idbGet(PROJECTS_STORE, id);
      expect(afterRetry?.composition?.audioSegments).toHaveLength(1);
      expect(afterRetry?.composition?.focusedClipId).toBe('later-edit');
      expect(projectRef.current?.composition?.audioSegments).toHaveLength(1);
      expect(projectRef.current?.composition?.focusedClipId).toBe('later-edit');
      notice.unmount();
    }
    if (change === 'retry-unmounted') {
      const recovery = renderHook(() => useAudioImportRecovery());
      const record = recovery.result.current.find((entry) => entry.fence.projectId === id)!;
      const notice = render(<AudioImportRecoveryNotice />);
      const item = notice.getByText(id).parentElement!;
      let releaseRetry!: () => void;
      const retryBarrier = new Promise<void>((resolve) => { releaseRetry = resolve; });
      const actualUpdate = projectManager.updateProject.bind(projectManager);
      const retryUpdate = vi.spyOn(projectManager, 'updateProject').mockImplementation(async (...args) => {
        await retryBarrier;
        return actualUpdate(...args);
      });
      fireEvent.click(within(item).getByRole('button', { name: 'Retry save' }));
      await waitFor(() => expect(retryUpdate).toHaveBeenCalledOnce());
      unregister();
      const replacementOwner = { current: id as string | null };
      const unregisterReplacement = registerSubtitleSaveEditor(replacementOwner);
      await act(async () => { releaseRetry(); await retryBarrier; });
      notice.rerender(<AudioImportRecoveryNotice />);
      await waitFor(() => expect(within(item).getByRole('button', { name: 'Retry save' })).toBeDisabled());
      await act(async () => { await retryAudioImportRecovery(record.fence); });
      expect(retryUpdate).toHaveBeenCalledOnce();
      expect(isAudioImportWriteAllowed(id)).toBe(false);
      expect(projectRef.current?.composition?.audioSegments).toHaveLength(0);
      expect(projectRef.current?.composition?.focusedClipId).toBe('later-edit');
      clearAudioImportRecovery(record.fence); finishAudioImportWrite(record.fence, true);
      unregisterReplacement(); notice.unmount(); recovery.unmount();
    }
    if (change === 'retry-remount') {
      const recovery = renderHook(() => useAudioImportRecovery());
      const record = recovery.result.current.find((entry) => entry.fence.projectId === id)!;
      hook.unmount();
      expect(record.isCurrent()).toBe(false);
      await act(async () => { await retryAudioImportRecovery(record.fence); });
      expect(isAudioImportWriteAllowed(id)).toBe(false);
      hook = renderHook(useTestImporter);
      await waitFor(() => expect(recovery.result.current.find((entry) => entry.fence === record.fence)?.isCurrent()).toBe(true));
      await act(async () => { await retryAudioImportRecovery(record.fence); });
      expect(isAudioImportWriteAllowed(id)).toBe(true);
      expect(projectRef.current?.composition?.audioSegments).toHaveLength(1);
      expect(projectRef.current?.composition?.focusedClipId).toBe('later-edit');
      const reconciled = await idbGet(PROJECTS_STORE, id);
      expect(reconciled?.composition?.audioSegments?.map(({ id: audioId, rawAudioPath }) => ({ id: audioId, rawAudioPath })))
        .toEqual(stored?.composition?.audioSegments?.map(({ id: audioId, rawAudioPath }) => ({ id: audioId, rawAudioPath })));
      recovery.unmount();
    }
    if (change === 'retry-reactivated') {
      const recovery = renderHook(() => useAudioImportRecovery());
      const record = recovery.result.current.find((entry) => entry.fence.projectId === id)!;
      hook.unmount();
      activateNarrationProject(owner, id);
      hook = renderHook(useTestImporter);
      expect(record.isCurrent()).toBe(false);
      await act(async () => { await retryAudioImportRecovery(record.fence); });
      expect(isAudioImportWriteAllowed(id)).toBe(false);
      expect(projectRef.current?.composition?.audioSegments).toHaveLength(0);
      clearAudioImportRecovery(record.fence); finishAudioImportWrite(record.fence, true);
      recovery.unmount();
    }
    if (change === 'failure' || change === 'edit' || change === 'after-commit') {
      await projectManager.updateProject(id, { composition: projectRef.current?.composition,
        segment: segmentRef.current ?? undefined, rawVideoPath: projectRef.current?.rawVideoPath });
      const afterAutosave = await idbGet(PROJECTS_STORE, id);
      expect(afterAutosave?.composition?.audioSegments).toHaveLength(change === 'after-commit' ? 1 : 0);
      expect(afterAutosave?.rawVideoPath).toBe(change === 'after-commit' && kind === 'placeholder' ? 'new-placeholder.mp4' : project.rawVideoPath);
    }
    hook.unmount(); unregister(); logError.mockRestore(); logWarning.mockRestore(); setNarrationProjectId(owner, null);
  },
);
