import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useProjects } from '@/hooks/useProjects';
import { projectManager } from '@/lib/projectManager';
import { ensureProjectComposition } from '@/lib/projectComposition';
import { editCompositionSubtitleView } from '@/lib/compositionSubtitleEditing';
import { getCompositionClipRenderSegment } from '@/lib/compositionSubtitles';
import { createSubtitleTrackStateFromSegments, getSubtitleTracks, RECOVERED_SUBTITLE_TRACK_PREFIX } from '@/lib/subtitleTracks';
import { deleteSubtitleIdsAcrossTracks } from '@/lib/subtitleTrackMutations';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import type { Project, SubtitleSegment, VideoSegment } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined), logToHost: vi.fn() }));
vi.mock('@/lib/projectManager', () => ({ projectManager: {
  loadProject: vi.fn(), getProjects: vi.fn(async () => []), setActiveProjectId: vi.fn(), invalidateEditorWrites: vi.fn(),
} }));
vi.mock('@/hooks/useProjects/loadProjectMedia', () => ({
  loadProjectVideo: vi.fn(async () => ''),
  loadProjectAudioMedia: vi.fn(async () => ({})),
}));

function legacyProject(): Project {
  const cue: SubtitleSegment = { id: 'narration-cue', startTime: 0.5, endTime: 1.5, text: 'Canonical',
    style: defaultSubtitleStyle(), sourceGroup: { kind: 'audio', audioSegmentId: 'lane' },
  };
  const userEdit = { ...cue, text: 'Edited before the fix' };
  const segment = { trimStart: 0, trimEnd: 4, trimSegments: [{ id: 'kept', startTime: 0, endTime: 4 }],
    textSegments: [], ...createSubtitleTrackStateFromSegments([userEdit]),
  } as VideoSegment;
  const root = { id: 'root', name: 'Root', role: 'root' as const, duration: 4, segment,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
  };
  return { id: 'legacy', name: 'Legacy', createdAt: 0, lastModified: 0, duration: 4, segment,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [], recordingMode: 'withoutCursor',
    composition: { mode: 'separate', selectedClipId: 'root', focusedClipId: 'root', clips: [root],
      globalSegment: { ...segment, ...createSubtitleTrackStateFromSegments([cue]) } },
  } as unknown as Project;
}

it('loads a legacy project with its clip-local caption copy recovered in both editor and composition', async () => {
  vi.mocked(projectManager.loadProject).mockResolvedValue(legacyProject());
  const setSegment = vi.fn();
  const loaded = vi.fn((_project: Project) => undefined);
  const noop = vi.fn();
  const hook = renderHook(() => useProjects({
    videoControllerRef: { current: undefined }, setCurrentVideo: noop, setCurrentAudio: noop,
    setCurrentMicAudio: noop, setCurrentWebcamVideo: noop, setSegment,
    setBackgroundConfig: noop, setMousePositions: noop, setThumbnails: noop, onProjectLoaded: loaded,
    currentVideo: null, currentAudio: null, currentMicAudio: null,
  }));
  await act(async () => { await hook.result.current.handleLoadProject('legacy'); });

  const editorSegment = setSegment.mock.calls.at(-1)![0] as VideoSegment;
  const recoveredTrack = `${RECOVERED_SUBTITLE_TRACK_PREFIX}subtitle-track-original`;
  const cuesOf = (segment: VideoSegment, trackId: string) =>
    getSubtitleTracks(segment).find((track) => track.id === trackId)?.segments ?? [];
  expect(cuesOf(editorSegment, 'subtitle-track-original')).toEqual([]);
  expect(cuesOf(editorSegment, recoveredTrack)).toEqual([
    expect.objectContaining({ id: 'narration-cue-recovered', text: 'Edited before the fix' }),
  ]);

  const project = loaded.mock.calls[0][0];
  const composition = ensureProjectComposition(project);
  const view = getCompositionClipRenderSegment(composition, 'root', editorSegment);
  const edited = editCompositionSubtitleView(composition, 'root', editorSegment,
    deleteSubtitleIdsAcrossTracks(view, ['narration-cue']));
  expect(edited.composition.globalSegment?.subtitleSegments).toEqual([]);
  expect(cuesOf(edited.segment, recoveredTrack)).toEqual(cuesOf(editorSegment, recoveredTrack));
  hook.unmount();
});
