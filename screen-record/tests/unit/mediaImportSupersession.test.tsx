import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useAppMediaImports } from '@/hooks/useAppMediaImports';
import { projectManager } from '@/lib/projectManager';
import { logToHost } from '@/lib/ipc';
import { activateNarrationProject, beginNarrationProjectLoad, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { importAudioPathToManagedMediaFile, importVideoPathToManagedMediaFile } from '@/lib/mediaServer';
import { createSyntheticProjectFixture } from '@/testHarness/syntheticProject';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined), logToHost: vi.fn() }));
vi.mock('@/lib/projectManager', () => ({ projectManager: { saveProject: vi.fn(), deleteProject: vi.fn() } }));
vi.mock('@/lib/mediaServer', () => ({
  createAudioPlaceholderVideo: vi.fn(async () => ({ path: 'placeholder.mp4' })),
  importAudioPathToManagedMediaFile: vi.fn(async () => ({ path: 'audio.wav', duration: 3 })),
  importAudioToManagedMediaFile: vi.fn(async () => ({ path: 'audio.wav', duration: 3 })),
  importVideoPathToManagedMediaFile: vi.fn(async () => ({ path: 'video.mp4', hasAudio: true })),
  importVideoToManagedMediaFile: vi.fn(async () => ({ path: 'video.mp4', hasAudio: true })),
  getMediaServerUrl: vi.fn(async () => 'video-url'),
}));

const encodeText = (text: string) => new TextEncoder().encode(text).buffer;

beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.restoreAllMocks());

it.each((['subtitle', 'audio', 'video'] as const).flatMap((kind) =>
  (['before', 'after'] as const).map((timing) => ({ kind, timing }))))(
  'does not perform post-load actions when a connected $kind import is superseded $timing application', async ({ kind, timing }) => {
    const project = createSyntheticProjectFixture({ subtitleCount: 1, narrationCount: 0, audioCount: 0 });
    vi.mocked(projectManager.saveProject).mockResolvedValue(project);
    let finish!: (applied: boolean) => void;
    const load = new Promise<boolean>((resolve) => { finish = resolve; });
    const handleLoadProject = vi.fn(() => load);
    const owner = { current: null as string | null };
    const setActivePanel = vi.fn();
    const setEditingSubtitleId = vi.fn();
    const noop = vi.fn();
    const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: { current: null },
      segmentRef: { current: null }, segment: null, duration: 0, setActivePanel, setEditingSubtitleId,
      projects: { handleLoadProject, loadProjects: async () => [], setShowProjectsDialog: noop },
    }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
    if (kind === 'video') {
      const createElement = document.createElement.bind(document);
      vi.spyOn(document, 'createElement').mockImplementation(((tag: string) => {
        if (tag === 'canvas') return { width: 0, height: 0, getContext: () => null } as unknown as HTMLElement;
        if (tag !== 'video') return createElement(tag);
        const video = { duration: 3, videoWidth: 1920, videoHeight: 1080, removeAttribute: noop, load: noop,
          onloadedmetadata: null as null | (() => void), onseeked: null as null | (() => void) };
        Object.defineProperty(video, 'src', { set: () => queueMicrotask(() => video.onloadedmetadata?.()) });
        Object.defineProperty(video, 'currentTime', { set: () => queueMicrotask(() => video.onseeked?.()) });
        return video as unknown as HTMLElement;
      }) as typeof document.createElement);
    }
    const hook = renderHook(() => useAppMediaImports(args));
    let operation!: Promise<unknown>;
    act(() => {
      operation = kind === 'subtitle' ? hook.result.current.importSubtitlePayload({
        fileName: 'captions.srt', content: '1\n00:00:00,000 --> 00:00:01,000\nHello\n', mimeType: 'application/x-subrip',
      }) : kind === 'audio' ? hook.result.current.importAudioPaths(['track.wav'])
        : hook.result.current.importVideoPath('clip.mp4');
    });
    await waitFor(() => expect(handleLoadProject).toHaveBeenCalledOnce());
    let result: unknown;
    await act(async () => {
      if (timing === 'before') {
        owner.current = 'newer-project';
        finish(false);
      } else {
        setNarrationProjectId(owner, project.id);
        finish(true);
        queueMicrotask(() => activateNarrationProject(owner, project.id));
      }
      result = await operation;
    });
    expect(owner.current).toBe(timing === 'before' ? 'newer-project' : project.id);
    expect(setActivePanel).not.toHaveBeenCalled();
    expect(setEditingSubtitleId).not.toHaveBeenCalled();
    if (kind === 'audio') expect(vi.mocked(logToHost).mock.calls.some(([line]) => line.includes('project callback complete'))).toBe(false);
    if (kind === 'video') expect(result).toBeNull();
    hook.unmount(); setNarrationProjectId(owner, null);
  },
);

it.each((['subtitle', 'audio', 'video'] as const).flatMap((kind) =>
  (['loading', 'same-id'] as const).map((change) => ({ kind, change }))))(
  'rejects a deferred $kind read when ownership changes: $change', async ({ kind, change }) => {
    const owner = { current: null as string | null };
    if (change === 'same-id') setNarrationProjectId(owner, 'original');
    let finishRead!: (value: never) => void;
    const read = new Promise<never>((resolve) => { finishRead = resolve; });
    if (kind === 'audio') vi.mocked(importAudioPathToManagedMediaFile).mockReturnValue(read);
    if (kind === 'video') vi.mocked(importVideoPathToManagedMediaFile).mockReturnValue(read);
    const handleLoadProject = vi.fn();
    const setActivePanel = vi.fn();
    const setSegment = vi.fn();
    const noop = vi.fn();
    const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: { current: null },
      segmentRef: { current: null }, segment: null, duration: 0, setActivePanel, setSegment,
      projects: { handleLoadProject, loadProjects: async () => [], setShowProjectsDialog: noop },
    }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
    const hook = renderHook(() => useAppMediaImports(args));
    let operation!: Promise<unknown>;
    act(() => {
      operation = kind === 'subtitle' ? hook.result.current.importSubtitleFile({ name: 'captions.srt',
        type: 'application/x-subrip', arrayBuffer: () => read.then(encodeText) } as unknown as File)
        : kind === 'audio' ? hook.result.current.importAudioPaths(['track.wav'])
          : hook.result.current.importVideoPath('clip.mp4');
    });
    if (kind === 'audio') await waitFor(() => expect(importAudioPathToManagedMediaFile).toHaveBeenCalledOnce());
    if (kind === 'video') await waitFor(() => expect(importVideoPathToManagedMediaFile).toHaveBeenCalledOnce());
    const finishLoad = change === 'loading' ? beginNarrationProjectLoad(owner) : null;
    if (change === 'same-id') activateNarrationProject(owner, 'original');
    await act(async () => {
      finishRead((kind === 'subtitle' ? '1\n00:00:00,000 --> 00:00:01,000\nHello\n'
        : kind === 'audio' ? { path: 'audio.wav', duration: 3 } : { path: 'video.mp4', hasAudio: true }) as never);
      await operation;
    });
    expect(handleLoadProject).not.toHaveBeenCalled();
    expect(setActivePanel).not.toHaveBeenCalled();
    expect(setSegment).not.toHaveBeenCalled();
    finishLoad?.(false); hook.unmount(); setNarrationProjectId(owner, null);
  },
);

it.each(['subtitle', 'audio'] as const)(
  'does not load a newly created %s project after a competing load starts', async (kind) => {
    const project = createSyntheticProjectFixture();
    let finishSave!: (value: typeof project) => void;
    const saved = new Promise<typeof project>((resolve) => { finishSave = resolve; });
    vi.mocked(projectManager.saveProject).mockReturnValue(saved);
    const owner = { current: null as string | null };
    const handleLoadProject = vi.fn();
    const noop = vi.fn();
    const args = new Proxy({ currentProjectIdRef: owner, currentProjectDataRef: { current: null },
      segmentRef: { current: null }, segment: null, duration: 0,
      projects: { handleLoadProject, loadProjects: async () => [], setShowProjectsDialog: noop },
    }, { get: (target, key) => key in target ? Reflect.get(target, key) : noop }) as Parameters<typeof useAppMediaImports>[0];
    const hook = renderHook(() => useAppMediaImports(args));
    let operation!: Promise<unknown>;
    act(() => { operation = kind === 'subtitle' ? hook.result.current.importSubtitlePayload({ fileName: 'captions.srt',
      content: '1\n00:00:00,000 --> 00:00:01,000\nHello\n' }) : hook.result.current.importAudioPaths(['track.wav']); });
    await waitFor(() => expect(projectManager.saveProject).toHaveBeenCalledOnce());
    const finishLoad = beginNarrationProjectLoad(owner);
    await act(async () => { finishSave(project); await operation; });
    expect(handleLoadProject).not.toHaveBeenCalled();
    finishLoad(false); hook.unmount(); setNarrationProjectId(owner, null);
  },
);
