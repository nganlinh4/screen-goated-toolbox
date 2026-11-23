import { useEffect, useRef, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import type {
  ImportedAudioSegment,
  Project,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";
import { type ActivePanel } from "@/components/sidepanel/index";
import type { SubtitleSource } from "@/lib/subtitleGenerationPlan";
import { useVideoImport } from "@/hooks/useVideoImport";
import { useImportedAudioImport } from "@/hooks/useImportedAudioImport";
import { useSubtitleImport } from "@/hooks/useSubtitleSrtImport";
import { logToHost } from "@/lib/ipc";
import { createAudioPlaceholderVideo, getMediaServerUrl } from "@/lib/mediaServer";
import { getTimelineContentEnd, resizeSegmentDuration } from "@/lib/timelineDuration";
import type { VideoController } from "@/lib/videoController";
import { appendAudioSegments } from "@/lib/audioSegmentTime";
import { beginAudioImportWrite, finishAudioImportWrite } from '@/lib/audioImportWriteFence';
import { captureImportStartOwner, importStartOwnerCurrent } from './importProjectOwnership';
import { isSubtitleSaveEditorActive, subscribeSubtitleSaveInvalidation } from './subtitleCompletedPersistence';
import { AudioSavedNotPublishedError, AudioSavedPreviewError, type TimelineWriteOwner } from './timelineWriteOwner';
import { clearAudioImportRecovery, currentAudioRecoveryEditor, registerAudioRecoveryEditor, retainAudioImportRecovery,
  type AudioRecoveryEditor, type AudioRecoveryRequest } from './audioImportRecovery';
import { isNarrationProjectLoading, narrationProjectEpoch } from './narrationProjectOwnership';

type CompositionSetter = (
  value:
    | ProjectComposition
    | null
    | ((prev: ProjectComposition | null) => ProjectComposition | null),
) => void;

type SegmentSetter = (
  value:
    | VideoSegment
    | null
    | ((prev: VideoSegment | null) => VideoSegment | null),
) => void;

interface AppMediaImportsOptions {
  composition: ProjectComposition | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentTime: number;
  duration: number;
  isPlaceholderBackedProject: boolean;
  loadedClipIdRef?: MutableRefObject<string | null>;
  persistTimelineWorkspaceState: (
    nextSegment: VideoSegment,
    nextComposition: ProjectComposition | null,
    nextDuration: number,
    reason: string,
    rawVideoPath?: string,
    owner?: TimelineWriteOwner,
  ) => Promise<boolean | void>;
  projects: {
    handleLoadProject: (projectId: string) => Promise<import('./useProjects').ProjectLoadResult>;
    loadProjects: () => Promise<unknown>;
    setShowProjectsDialog: (show: boolean) => void;
  };
  segment: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  selectedSubtitleIdsRef: MutableRefObject<string[]>;
  selectedTextIdsRef: MutableRefObject<string[]>;
  setActivePanel: (panel: ActivePanel) => void;
  setComposition: CompositionSetter;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setEditingSubtitleId: (id: string | null) => void;
  setError: (message: string) => void;
  setSegment: SegmentSetter;
  setSubtitleSource: (source: SubtitleSource) => void;
  updateCurrentMusicSegments: (
    updater: (segments: ImportedAudioSegment[]) => ImportedAudioSegment[],
    reason: string,
    options?: { persist: boolean; owner?: TimelineWriteOwner },
  ) => void | Promise<boolean | void>;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
}

export function useAppMediaImports({
  composition,
  currentProjectDataRef,
  currentProjectIdRef,
  currentTime,
  duration,
  loadedClipIdRef,
  persistTimelineWorkspaceState,
  projects,
  segment,
  segmentRef,
  selectedSubtitleIdsRef,
  selectedTextIdsRef,
  setActivePanel,
  setCurrentVideo,
  setEditingSubtitleId,
  setError,
  setSegment,
  setSubtitleSource,
  updateCurrentMusicSegments,
  videoControllerRef,
}: AppMediaImportsOptions) {
  const mountedRef = useRef(true);
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  const captureMountedOwner = () => {
    const owner = captureImportStartOwner(currentProjectIdRef);
    return owner ? () => mountedRef.current && owner() : null;
  };
  const { isImporting, importVideo, importVideoPath } = useVideoImport({
    captureStartOwner: captureMountedOwner,
    onProjectCreated: async (project, owner) => {
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      if (!importStartOwnerCurrent(owner)) return false;
      const activated = await projects.handleLoadProject(project.id);
      return typeof activated === 'function' && activated() ? activated : false;
    },
  });

  const attachToCurrentProject = async (input: ImportedAudioSegment[], expectedProjectId: string,
    owner?: () => boolean, recovery?: AudioRecoveryRequest): Promise<void> => {
        const segments = input.map((segment) => ({ ...segment }));
        const originalClipId = recovery ? recovery.clipId : loadedClipIdRef?.current;
        const epoch = recovery?.epoch ?? narrationProjectEpoch(currentProjectIdRef);
        const isCurrent = () => mountedRef.current && currentProjectIdRef.current === expectedProjectId && (owner?.() ?? true)
          && !isNarrationProjectLoading(currentProjectIdRef) && narrationProjectEpoch(currentProjectIdRef) === epoch
          && loadedClipIdRef?.current === originalClipId && (!recovery || isSubtitleSaveEditorActive(currentProjectIdRef));
        if (!isCurrent()) throw new Error('The open project changed while audio was importing');
        const audioFence = recovery?.fence ?? beginAudioImportWrite(expectedProjectId);
        const request: AudioRecoveryRequest = { fence: audioFence, projectId: expectedProjectId, epoch, clipId: originalClipId, segments };
        const attempt = async (): Promise<void> => {
        let didPublish = false;
        try {
        let saved = audioFence.committed;
        for (let attemptNumber = 0; ; attemptNumber += 1) {
        const baseProject = currentProjectDataRef.current;
        const baseEditorSegment = segmentRef.current;
        const baseLoadedClipId = loadedClipIdRef?.current;
        const writeOwner: TimelineWriteOwner = { isCurrent: () => isCurrent()
          && currentProjectDataRef.current === baseProject && segmentRef.current === baseEditorSegment,
          onInvalidated: (listener) => subscribeSubtitleSaveInvalidation(currentProjectIdRef, listener), audioFence };
        const assertProjectUnchanged = () => {
          if (!isCurrent()) {
            throw new Error("The open project changed while audio was importing");
          }
        };
        const appendMissing = (existing: ImportedAudioSegment[]) => {
          const existingIds = new Set(existing.map((item) => item.id));
          return appendAudioSegments(existing, segments.filter((item) => !existingIds.has(item.id)));
        };
        try {
        assertProjectUnchanged();
        if (segmentRef.current && (baseProject?.composition ?? composition)?.placeholderVideoForAudio) {
          if (originalClipId && originalClipId !== 'root') throw new Error('Select the root clip before attaching audio.');
          const baseSegment = segmentRef.current;
          const baseComposition =
            currentProjectDataRef.current?.composition ?? composition ?? null;
          if (!baseComposition) return;
          const existingSegments = baseComposition.audioSegments ?? [];
          const nextAudioSegments = appendMissing(existingSegments);
          const nextDuration = Math.max(
            duration,
            baseProject?.duration ?? 0,
            baseSegment.trimEnd,
            getTimelineContentEnd(
              baseSegment,
              nextAudioSegments,
              baseComposition.narrationSegments,
            ),
            1,
          );
          const nextSegment = {
            ...resizeSegmentDuration(baseSegment, nextDuration),
            mediaMode: undefined,
          };
          const placeholder = await createAudioPlaceholderVideo(
            nextDuration,
            "attach-audio-to-placeholder-project",
          );
          if (currentProjectIdRef.current !== expectedProjectId || owner?.() === false) {
            assertProjectUnchanged();
          }
          if (segmentRef.current !== baseSegment || (currentProjectDataRef.current?.composition ?? composition ?? null) !== baseComposition) {
            throw new Error("The timeline changed while audio was importing");
          }
          const nextComposition = {
            ...baseComposition,
            audioSegments: nextAudioSegments,
            clips: baseComposition.clips.map((clip) =>
              clip.id === "root"
                ? {
                    ...clip,
                    duration: nextDuration,
                    rawVideoPath: placeholder.path,
                    segment: nextSegment,
                  }
                : clip,
            ),
            globalSegment: baseComposition.globalSegment
              ? nextSegment
              : baseComposition.globalSegment,
            placeholderVideoForAudio: true,
            placeholderVideoForSubtitles: baseComposition.placeholderVideoForSubtitles,
            timelineOnly: false,
          };
          const published = await persistTimelineWorkspaceState(
            nextSegment,
            nextComposition,
            nextDuration,
            "attach-audio-to-placeholder-project",
            placeholder.path,
            writeOwner,
          );
          const visible = currentProjectDataRef.current?.composition?.audioSegments ?? [];
          if (!audioFence.committed || published !== true || currentProjectDataRef.current?.rawVideoPath !== placeholder.path
            || !segments.every((segment) => visible.some((item) => item.id === segment.id && item.rawAudioPath === segment.rawAudioPath))) {
            throw audioFence.committed ? new AudioSavedNotPublishedError() : new Error('Audio could not be saved.');
          }
          didPublish = true;
          try {
            const assertPreviewCurrent = () => {
              assertProjectUnchanged();
              if (loadedClipIdRef?.current !== baseLoadedClipId
                || currentProjectDataRef.current?.composition !== nextComposition
                || currentProjectDataRef.current?.rawVideoPath !== placeholder.path) {
                throw new Error('The preview target changed');
              }
            };
            assertPreviewCurrent();
            const mediaUrl = await getMediaServerUrl(placeholder.path);
            assertPreviewCurrent();
            const loadedUrl = await videoControllerRef.current?.loadVideo({
              videoUrl: mediaUrl,
              initialTime: currentTime,
              debugLabel: "attach-audio-to-placeholder-project",
            });
            assertPreviewCurrent();
            setCurrentVideo(loadedUrl ?? mediaUrl);
            setSubtitleSource("audio");
          } catch {
            throw new AudioSavedPreviewError();
          }
          return;
        }
        const published = await updateCurrentMusicSegments(
          appendMissing,
          "attach-audio-to-current-project",
          { persist: true, owner: writeOwner },
        );
        const visible = currentProjectDataRef.current?.composition?.audioSegments ?? [];
        if (!audioFence.committed || published !== true || !segments.every((segment) => visible.some((item) => item.id === segment.id && item.rawAudioPath === segment.rawAudioPath))) {
          throw audioFence.committed ? new AudioSavedNotPublishedError() : new Error('Audio could not be saved.');
        }
        didPublish = true;
        if (composition?.placeholderVideoForAudio) {
          setSubtitleSource("audio");
        }
        return;
        } catch (error) {
          if (audioFence.committed) saved = true;
          if (error instanceof AudioSavedNotPublishedError) {
            saved = true;
            if (isCurrent() && attemptNumber < 2) continue;
          }
          if (saved && !(error instanceof AudioSavedPreviewError) && isCurrent()
            && attemptNumber < 2 && (currentProjectDataRef.current !== baseProject || segmentRef.current !== baseEditorSegment)) continue;
          if (saved && !(error instanceof AudioSavedPreviewError)) throw new AudioSavedNotPublishedError();
          throw error;
        }
        }
        } finally {
          if (didPublish) clearAudioImportRecovery(audioFence);
          else if (audioFence.committed) retainAudioImportRecovery(audioFence, currentProjectDataRef.current?.name ?? expectedProjectId,
            () => currentAudioRecoveryEditor(currentProjectIdRef)?.canResume(request) ?? false,
            async () => {
              const editor = currentAudioRecoveryEditor(currentProjectIdRef);
              if (!editor?.canResume(request)) throw new Error('The original editor is unavailable');
              await editor.resume(request);
            });
          finishAudioImportWrite(audioFence, didPublish);
        }
        };
        await attempt();
      };
  const recoveryEditor = useRef<AudioRecoveryEditor | null>(null);
  recoveryEditor.current = {
    canResume: (request) => mountedRef.current && isSubtitleSaveEditorActive(currentProjectIdRef)
      && currentProjectIdRef.current === request.projectId && narrationProjectEpoch(currentProjectIdRef) === request.epoch
      && !isNarrationProjectLoading(currentProjectIdRef) && loadedClipIdRef?.current === request.clipId,
    resume: async (request) => {
      try { await attachToCurrentProject(request.segments, request.projectId, undefined, request); }
      catch (error) { if (error instanceof AudioSavedPreviewError) setError(error.message); throw error; }
    },
  };
  useEffect(() => registerAudioRecoveryEditor(currentProjectIdRef, recoveryEditor), [currentProjectIdRef]);
  const { isImporting: isImportingAudio, importAudio, importAudios, importAudioPaths } =
    useImportedAudioImport({
      captureStartOwner: captureMountedOwner,
      getCurrentProjectId: () => currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? null,
      onAttachToCurrentProject: attachToCurrentProject,
      onCreateAudioProject: async (project, startOwner) => {
        logToHost(`[AudioImport][Frontend] load project start id="${project.id}"`);
        projects.setShowProjectsDialog(false);
        await projects.loadProjects();
        if (!importStartOwnerCurrent(startOwner)) return false;
        logToHost(`[AudioImport][Frontend] project list refreshed id="${project.id}"`);
        const owner = await projects.handleLoadProject(project.id);
        if (typeof owner !== 'function' || !owner()) return false;
        logToHost(`[AudioImport][Frontend] load project complete id="${project.id}"`);
        return owner;
      },
      onError: setError,
    });

  const {
    isImporting: isImportingSubtitle,
    importSubtitleFile,
    importSubtitlePayload,
  } = useSubtitleImport({
    captureStartOwner: captureMountedOwner,
    segment,
    duration,
    getCurrentProjectId: () =>
      currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? null,
    getCurrentSegment: () =>
      segmentRef.current ?? currentProjectDataRef.current?.segment ?? null,
    setSegment,
    setActivePanel,
    setEditingSubtitleId,
    onImportedIntoCurrentProject: () => {
      selectedSubtitleIdsRef.current = [];
      selectedTextIdsRef.current = [];
    },
    onCreateSubtitleProject: async (project, startOwner) => {
      logToHost(`[SubtitleImport][Frontend] load project start id="${project.id}"`);
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      if (!importStartOwnerCurrent(startOwner)) return false;
      const owner = await projects.handleLoadProject(project.id);
      if (typeof owner !== 'function' || !owner()) return false;
      logToHost(`[SubtitleImport][Frontend] load project complete id="${project.id}"`);
      return owner;
    },
    onError: setError,
  });

  return {
    importAudio,
    importAudioPaths,
    importAudios,
    importSubtitleFile,
    importSubtitlePayload,
    importVideo,
    importVideoPath,
    isImporting,
    isImportingAudio,
    isImportingSubtitle,
  };
}
