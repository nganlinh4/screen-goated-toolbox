import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useAppMediaImports } from '@/hooks/useAppMediaImports';
import { createSyntheticProjectFixture } from '@/testHarness/syntheticProject';
import type { ImportedAudioSegment, Project } from '@/types/video';
import { captureImportStartOwner } from '@/hooks/importProjectOwnership';
import { closeNarrationProject, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { markAudioImportCommitted, isAudioImportWriteAllowed, finishAudioImportWrite, type AudioImportWriteFence } from '@/lib/audioImportWriteFence';
import { clearAudioImportRecovery, retryAudioImportRecovery, useAudioImportRecovery } from '@/hooks/audioImportRecovery';
import { registerSubtitleSaveEditor } from '@/hooks/subtitleCompletedPersistence';
import { AudioSavedNotPublishedError } from '@/hooks/timelineWriteOwner';

type ImportCallback = (project: Project, owner?: (() => boolean) | null) => Promise<unknown>;
type AttachCallback = (segments: ImportedAudioSegment[], projectId: string, owner?: () => boolean) => Promise<void>;
const callbacks = vi.hoisted(() => ({ video: null as null | ImportCallback, audio: null as null | ImportCallback, subtitle: null as null | ImportCallback, attach: null as null | AttachCallback, capture: null as null | (() => (() => boolean) | null) }));
vi.mock('@/hooks/useVideoImport', () => ({ useVideoImport: (options: { onProjectCreated: typeof callbacks.video }) => { callbacks.video = options.onProjectCreated; return {}; } }));
vi.mock('@/hooks/useImportedAudioImport', () => ({ useImportedAudioImport: (options: { onCreateAudioProject: typeof callbacks.audio; onAttachToCurrentProject: typeof callbacks.attach; captureStartOwner: typeof callbacks.capture }) => { callbacks.audio = options.onCreateAudioProject; callbacks.attach = options.onAttachToCurrentProject; callbacks.capture = options.captureStartOwner; return {}; } }));
vi.mock('@/hooks/useSubtitleSrtImport', () => ({ useSubtitleImport: (options: { onCreateSubtitleProject: typeof callbacks.subtitle }) => { callbacks.subtitle = options.onCreateSubtitleProject; return {}; } }));
vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined), logToHost: vi.fn() }));
vi.mock('@/lib/mediaServer', () => ({ createAudioPlaceholderVideo: vi.fn(), getMediaServerUrl: vi.fn() }));
import { createAudioPlaceholderVideo } from '@/lib/mediaServer';
import { invoke } from '@/lib/ipc';

it.each(['audio', 'subtitle'] as const)('does not install a %s project after its load was superseded', async (kind) => {
  const owner = { current: 'old' as string | null };
  const setComposition = vi.fn();
  let finish!: (applied: boolean) => void;
  const load = new Promise<boolean>((resolve) => { finish = resolve; });
  const noop = vi.fn();
  const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: { current: null }, setComposition,
    projects: { handleLoadProject: () => load, loadProjects: async () => [], setShowProjectsDialog: noop },
  }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
  const hook = renderHook(() => useAppMediaImports(args));
  const project = createSyntheticProjectFixture();
  let importing!: Promise<void>;
  await act(async () => { importing = callbacks[kind]!(project); await Promise.resolve(); });
  owner.current = 'newer';
  await act(async () => { finish(false); await importing; });
  expect(owner.current).toBe('newer');
  expect(setComposition).not.toHaveBeenCalled();
  hook.unmount();
});

it('rejects a placeholder import if the same project is edited while its media is created', async () => {
  const project = createSyntheticProjectFixture({ audioCount: 1 });
  project.composition = { ...project.composition!, placeholderVideoForAudio: true };
  const owner = { current: project.id as string | null };
  const projectRef = { current: project as Project | null };
  const segmentRef = { current: project.segment };
  let finish!: (value: { path: string }) => void;
  vi.mocked(createAudioPlaceholderVideo).mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
  const persist = vi.fn();
  const noop = vi.fn();
  const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: projectRef,
    composition: project.composition, segmentRef, segment: project.segment, duration: project.duration,
    isPlaceholderBackedProject: true, persistTimelineWorkspaceState: persist,
    projects: { handleLoadProject: noop, loadProjects: noop, setShowProjectsDialog: noop },
  }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
  const hook = renderHook(() => useAppMediaImports(args));
  let importing!: Promise<void>;
  act(() => { importing = callbacks.attach!(project.composition!.audioSegments, project.id, () => true); });
  const edited = { ...project.composition!, focusedClipId: 'later-edit' };
  projectRef.current = { ...project, composition: edited };
  await act(async () => { finish({ path: 'temporary.mp4' }); await expect(importing).rejects.toThrow('timeline changed'); });
  expect(projectRef.current.composition).toBe(edited);
  expect(persist).not.toHaveBeenCalled();
  // This path has no exclusive ownership receipt and may already be referenced.
  expect(invoke).not.toHaveBeenCalledWith('delete_file', { path: 'temporary.mp4' });
  hook.unmount();
});

it('rejects a delayed audio attachment after its editor unmounts', async () => {
  const project = createSyntheticProjectFixture({ audioCount: 1 });
  const owner = { current: project.id as string | null };
  const update = vi.fn();
  const noop = vi.fn();
  const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: { current: project },
    segmentRef: { current: project.segment }, segment: project.segment, composition: project.composition,
    isPlaceholderBackedProject: false, updateCurrentMusicSegments: update,
    projects: { handleLoadProject: noop, loadProjects: noop, setShowProjectsDialog: noop },
  }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
  const hook = renderHook(() => useAppMediaImports(args));
  const attach = callbacks.attach!;
  const importOwner = callbacks.capture!();
  expect(importOwner?.()).toBe(true);
  let finish!: () => void;
  const mediaReady = new Promise<void>((resolve) => { finish = resolve; });
  const pending = mediaReady.then(() => attach(project.composition!.audioSegments, project.id, () => true));
  hook.unmount();
  expect(importOwner?.()).toBe(false);
  await act(async () => { finish(); await expect(pending).rejects.toThrow('project changed'); });
  expect(update).not.toHaveBeenCalled();
  expect(isAudioImportWriteAllowed(project.id)).toBe(true);
});

it.each(['video', 'audio', 'subtitle'] as const)('does not load a stale %s import after close while the list is refreshing', async (kind) => {
  const owner = { current: null as string | null };
  const startOwner = captureImportStartOwner(owner);
  let finish!: () => void;
  const list = new Promise<void>((resolve) => { finish = resolve; });
  const load = vi.fn();
  const noop = vi.fn();
  const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: { current: null },
    projects: { handleLoadProject: load, loadProjects: () => list, setShowProjectsDialog: noop },
  }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
  const hook = renderHook(() => useAppMediaImports(args));
  let importing!: Promise<unknown>;
  act(() => { importing = callbacks[kind]!(createSyntheticProjectFixture(), startOwner); });
  closeNarrationProject(owner);
  await act(async () => { finish(); expect(await importing).toBe(false); });
  expect(load).not.toHaveBeenCalled();
  hook.unmount();
});

it('retains the fence when a committed audio retry returns no publication acknowledgement', async () => {
  const project = createSyntheticProjectFixture({ audioCount: 0 });
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, project.id);
  const unregister = registerSubtitleSaveEditor(owner);
  const projectRef = { current: project as Project | null };
  const segmentRef = { current: project.segment };
  let fence: AudioImportWriteFence | undefined;
  let calls = 0;
  const update = vi.fn(async (_update, _reason, options) => {
    calls += 1;
    fence = options.owner.audioFence;
    if (calls === 1) {
      markAudioImportCommitted(project.id, fence);
      projectRef.current = { ...projectRef.current!, composition: { ...projectRef.current!.composition!, focusedClipId: 'later-edit' } };
      throw new AudioSavedNotPublishedError();
    }
    return undefined;
  });
  const noop = vi.fn();
  const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: projectRef, segmentRef,
    segment: project.segment, composition: project.composition, duration: project.duration,
    isPlaceholderBackedProject: false, updateCurrentMusicSegments: update,
    projects: { handleLoadProject: noop, loadProjects: noop, setShowProjectsDialog: noop },
  }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
  const hook = renderHook(() => useAppMediaImports(args));
  const audio = { id: 'incoming', rawAudioPath: 'incoming.wav', name: 'Audio', duration: 2,
    startTime: 0, inPoint: 0, outPoint: 2, addedAt: 0 };
  await act(async () => { await expect(callbacks.attach!([audio], project.id, () => true)).rejects.toThrow('Audio was saved'); });
  const recoveries = renderHook(() => useAudioImportRecovery());
  expect(recoveries.result.current.some((entry) => entry.fence === fence)).toBe(true);
  expect(isAudioImportWriteAllowed(project.id)).toBe(false);
  await act(async () => { await retryAudioImportRecovery(fence!); });
  expect(isAudioImportWriteAllowed(project.id)).toBe(false);
  expect(recoveries.result.current.some((entry) => entry.fence === fence)).toBe(true);
  expect(projectRef.current?.composition?.audioSegments).toEqual([]);
  clearAudioImportRecovery(fence!); finishAudioImportWrite(fence!, true);
  recoveries.unmount(); hook.unmount(); unregister(); setNarrationProjectId(owner, null);
});
