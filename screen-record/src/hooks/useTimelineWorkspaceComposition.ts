import { narrationProjectEpoch } from './narrationProjectOwnership';
import { invalidateSubtitleSaveEditor } from './subtitleCompletedPersistence';
import type { TimelineWriteOwner } from './timelineWriteOwner';
import { audioSaveFailure, useAudioCompositionPersistence } from './useAudioCompositionPersistence';
import {
  startTransition,
  useCallback,
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type RefObject,
  type SetStateAction,
} from "react";
import type {
  ImportedAudioSegment,
  NarrationSegment,
  Project,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";
import type { EditorHistorySnapshot } from "@/hooks/useEditorHistory";
import { logToHost } from "@/lib/ipc";
import { projectManager } from "@/lib/projectManager";
import { createAudioPlaceholderVideo, getMediaServerUrl } from "@/lib/mediaServer";
import {
  getTimelineContentEnd,
  resizeCompositionRootDuration,
  resizeSegmentDuration,
} from "@/lib/timelineDuration";
import {
  applyLiveNarrationSegments,
  clearLiveNarrationSegments,
} from "@/lib/liveNarrationStreamStore";
import { preserveNarrationGroupEdits } from "@/lib/narrationGroupTakes";
import { excludeDeletedNarrationTakes } from "@/lib/narrationBatchRetirements";
import type { VideoController } from "@/lib/videoController";
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

interface TimelineWorkspaceCompositionOptions {
  composition: ProjectComposition | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectId: string | null;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentTime: number;
  duration: number;
  editorHistory: {
    replaceSnapshot: (snapshot: Partial<EditorHistorySnapshot>) => void;
  };
  handleEditorRawVideoPathChange: (value: string) => void;
  isPlaceholderBackedProject: boolean;
  isPlaying: boolean;
  loadProjects: () => Promise<unknown>;
  rawSetComposition: Dispatch<SetStateAction<ProjectComposition | null>>;
  rawSetSegment: Dispatch<SetStateAction<VideoSegment | null>>;
  segmentRef: MutableRefObject<VideoSegment | null>;
  setComposition: CompositionSetter;
  setCompositionSilently: CompositionSetter;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setEditorPreviewDuration: (value: number) => void;
  setSegment: SegmentSetter;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
  videoRef: RefObject<HTMLVideoElement | null>;
}

export function useTimelineWorkspaceComposition({
  composition,
  currentProjectDataRef,
  currentProjectId,
  currentProjectIdRef,
  currentTime,
  duration,
  editorHistory,
  handleEditorRawVideoPathChange,
  isPlaceholderBackedProject,
  isPlaying,
  loadProjects,
  rawSetComposition,
  rawSetSegment,
  segmentRef,
  setComposition,
  setCompositionSilently,
  setCurrentProjectData,
  setCurrentVideo,
  setEditorPreviewDuration,
  setSegment,
  videoControllerRef,
  videoRef,
}: TimelineWorkspaceCompositionOptions) {
  const hasDeferredNarrationEditorFlushRef = useRef(false);
  const applyCurrentComposition = useAudioCompositionPersistence({
    currentProjectDataRef, currentProjectId, currentProjectIdRef, loadProjects, setComposition,
  });

  const updateCurrentMusicSegments = useCallback(
    (
      updater: (segments: ImportedAudioSegment[]) => ImportedAudioSegment[],
      reason: string,
      options: { persist: boolean; owner?: TimelineWriteOwner } = { persist: false },
    ) => {
      const baseComposition = currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) {
        void logToHost(`[AudioImport][Frontend] skip audio update reason="${reason}" no-composition`);
        return;
      }

      const nextComposition: ProjectComposition = {
        ...baseComposition,
        audioSegments: updater(baseComposition.audioSegments ?? []),
      };

      if (options.persist) {
        return applyCurrentComposition(nextComposition, reason, options.owner);
      }

      setCompositionSilently(nextComposition);
      const currentProject = currentProjectDataRef.current;
      if (currentProject) {
        currentProjectDataRef.current = {
          ...currentProject,
          composition: nextComposition,
        };
      }
    },
    [
      applyCurrentComposition,
      composition,
      currentProjectDataRef,
      setCompositionSilently,
    ],
  );

  const persistCurrentComposition = useCallback(
    (reason: string) => {
      const currentComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!currentComposition) {
        void logToHost(`[AudioImport][Frontend] skip composition persist reason="${reason}" no-composition`);
        return;
      }
      applyCurrentComposition(currentComposition, reason);
    },
    [applyCurrentComposition, composition, currentProjectDataRef],
  );

  const updateCurrentNarrationSegments = useCallback(
    (
      updater: (segments: NarrationSegment[]) => NarrationSegment[],
      reason: string,
      options: { persist: boolean } = { persist: false },
    ) => {
      const baseComposition = currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) {
        void logToHost(`[Narration][Frontend] skip narration update reason="${reason}" no-composition`);
        return;
      }

      const nextComposition: ProjectComposition = {
        ...baseComposition,
        narrationSegments: updater(baseComposition.narrationSegments ?? []),
      };

      if (options.persist) {
        applyCurrentComposition(nextComposition, reason);
        return;
      }

      setCompositionSilently(nextComposition);
      const currentProject = currentProjectDataRef.current;
      if (currentProject) {
        currentProjectDataRef.current = {
          ...currentProject,
          composition: nextComposition,
        };
      }
    },
    [
      applyCurrentComposition,
      composition,
      currentProjectDataRef,
      setCompositionSilently,
    ],
  );

  const applyNarrationAudioSegments = useCallback(
    (
      segments: NarrationSegment[],
      replaceSubtitleIds: string[],
    ) => {
      const projectId = currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? currentProjectId;
      if (projectId) {
        const accepted = excludeDeletedNarrationTakes(projectId, segments);
        if (segments.length > 0 && accepted.length === 0) return;
        if (accepted.length !== segments.length) {
          const removed = new Set(segments.filter((item) => !accepted.includes(item))
            .flatMap((item) => item.sourceSubtitleIds?.length ? item.sourceSubtitleIds : item.sourceSubtitleId ? [item.sourceSubtitleId] : []));
          const kept = new Set(accepted.flatMap((item) => item.sourceSubtitleIds?.length ? item.sourceSubtitleIds : item.sourceSubtitleId ? [item.sourceSubtitleId] : []));
          replaceSubtitleIds = replaceSubtitleIds.filter((id) => !removed.has(id) || kept.has(id));
        }
        segments = accepted;
      }
      const wasPlaying = Boolean(videoRef.current && !videoRef.current.paused);
      const baseComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) {
        void logToHost("[Narration][Frontend] skip apply no-composition");
        return;
      }

      const replaceSet = new Set(replaceSubtitleIds);
      // Each generation gets a new batch id of the form `{family}-{timestamp}`.
      // Drop takes from a PREVIOUS run of the same family: every Gemini run
      // re-segments the audio differently, so old takes without an exact
      // source-id match would otherwise pile up and overlap the new ones.
      const incomingBatchId = segments[0]?.narrationBatchId ?? null;
      const incomingFamily = incomingBatchId
        ? incomingBatchId.replace(/-\d+$/, "")
        : null;
      const isStalePriorRun = (segment: NarrationSegment) =>
        Boolean(
          incomingFamily &&
            segment.narrationBatchId &&
            segment.narrationBatchId !== incomingBatchId &&
            segment.narrationBatchId.replace(/-\d+$/, "") === incomingFamily,
        );
      const nextNarrationSegments = [
        ...(baseComposition.narrationSegments ?? []).filter((segment) => {
          if (isStalePriorRun(segment)) return false;
          const sourceIds = segment.sourceSubtitleIds?.length
            ? segment.sourceSubtitleIds
            : segment.sourceSubtitleId
              ? [segment.sourceSubtitleId]
              : [];
          if (sourceIds.length === 0) return true;
          return !sourceIds.some((id) => replaceSet.has(id));
        }),
        ...preserveNarrationGroupEdits(baseComposition.narrationSegments ?? [], segments),
      ].sort((left, right) => left.startTime - right.startTime);

      let nextComposition: ProjectComposition = {
        ...baseComposition,
        narrationSegments: nextNarrationSegments,
      };
      let nextSegment =
        currentProjectDataRef.current?.segment ??
        segmentRef.current;
      let nextDuration = duration;

      if (isPlaceholderBackedProject && nextSegment) {
        nextDuration = Math.max(
          duration,
          nextSegment.trimEnd,
          getTimelineContentEnd(
            nextSegment,
            baseComposition.audioSegments,
            nextNarrationSegments,
          ),
          1,
        );
        nextSegment = resizeSegmentDuration(nextSegment, nextDuration);
        nextComposition = resizeCompositionRootDuration(
          nextComposition,
          nextSegment,
          nextDuration,
        ) ?? nextComposition;
      }

      // This live path deliberately avoids a React state write while playing.
      // Retire any transaction based on the previous project before replacing it.
      invalidateSubtitleSaveEditor(currentProjectIdRef);
      currentProjectDataRef.current = currentProjectDataRef.current
        ? {
            ...currentProjectDataRef.current,
            composition: nextComposition,
            duration: Math.max(currentProjectDataRef.current.duration ?? 0, nextDuration),
            segment: nextSegment ?? currentProjectDataRef.current.segment,
          }
        : currentProjectDataRef.current;

      if (wasPlaying && (segments.length > 0 || replaceSubtitleIds.length > 0)) {
        applyLiveNarrationSegments(projectId, segments, replaceSubtitleIds);
        hasDeferredNarrationEditorFlushRef.current = true;
        return;
      }

      const shouldResizeSegment = Boolean(nextSegment && nextDuration > duration);
      if (shouldResizeSegment && nextSegment) {
        editorHistory.replaceSnapshot({ segment: nextSegment });
        startTransition(() => {
          rawSetSegment(nextSegment);
          if (shouldResizeSegment) setEditorPreviewDuration(nextDuration);
        });
      }
      editorHistory.replaceSnapshot({ composition: nextComposition });
      startTransition(() => {
        rawSetComposition(nextComposition);
      });
    },
    [
      composition,
      currentProjectDataRef,
      currentProjectId,
      currentProjectIdRef,
      duration,
      editorHistory,
      isPlaceholderBackedProject,
      rawSetComposition,
      rawSetSegment,
      segmentRef,
      setEditorPreviewDuration,
      videoRef,
    ],
  );

  const flushDeferredNarrationEditorState = useCallback((_reason: string) => {
    if (!hasDeferredNarrationEditorFlushRef.current) return;
    const latestComposition = currentProjectDataRef.current?.composition ?? null;
    const latestSegment = currentProjectDataRef.current?.segment ?? null;
    const latestDuration = currentProjectDataRef.current?.duration ?? null;
    const projectId =
      currentProjectIdRef.current ??
      currentProjectDataRef.current?.id ??
      currentProjectId ??
      null;
    if (!latestComposition) return;
    hasDeferredNarrationEditorFlushRef.current = false;
    if (latestSegment) {
      editorHistory.replaceSnapshot({ segment: latestSegment });
      startTransition(() => {
        rawSetSegment(latestSegment);
        if (latestDuration && latestDuration > duration) {
          setEditorPreviewDuration(latestDuration);
        }
      });
    }
    editorHistory.replaceSnapshot({ composition: latestComposition });
    startTransition(() => {
      rawSetComposition(latestComposition);
    });
    window.requestAnimationFrame(() => {
      clearLiveNarrationSegments(projectId);
    });
  }, [
    currentProjectDataRef,
    currentProjectId,
    currentProjectIdRef,
    duration,
    editorHistory,
    rawSetComposition,
    rawSetSegment,
    setEditorPreviewDuration,
  ]);

  useEffect(() => {
    if (isPlaying) return;
    flushDeferredNarrationEditorState("playback-stopped");
  }, [flushDeferredNarrationEditorState, isPlaying]);

  const persistTimelineWorkspaceState = useCallback(
    async (
      nextSegment: VideoSegment,
      nextComposition: ProjectComposition | null,
      nextDuration: number,
      reason: string,
      rawVideoPath?: string,
      owner?: TimelineWriteOwner,
    ) => {
      if (owner && !owner.isCurrent()) throw new Error('The timeline changed while audio was importing');
      const publish = () => {
        setSegment(nextSegment);
        setEditorPreviewDuration(nextDuration);
        if (nextComposition) setComposition(nextComposition);
        if (rawVideoPath !== undefined) handleEditorRawVideoPathChange(rawVideoPath);
        const currentProject = currentProjectDataRef.current;
        if (currentProject) {
          const nextProject = { ...currentProject, composition: nextComposition ?? currentProject.composition,
            duration: nextDuration, rawVideoPath: rawVideoPath ?? currentProject.rawVideoPath, segment: nextSegment };
          currentProjectDataRef.current = nextProject;
          setCurrentProjectData(nextProject);
        }
      };
      if (!owner) publish();

      const projectId =
        currentProjectIdRef.current ??
        currentProjectDataRef.current?.id ??
        currentProjectId ??
        null;
      if (!projectId) {
        void logToHost(`[TimelineDuration] skip persist reason="${reason}" no-project`);
        return;
      }

      try {
        const writeIntent = projectManager.createEditorWriteIntent(projectId);
        const applied = await projectManager.updateProject(
          projectId,
          {
            composition: nextComposition ?? undefined,
            duration: nextDuration,
            segment: nextSegment,
            ...(rawVideoPath !== undefined ? { rawVideoPath } : {}),
          },
          writeIntent,
          owner?.isCurrent,
          owner?.onInvalidated,
          owner?.audioFence,
        );
        if (!applied && owner) throw new Error(owner.isCurrent()
          ? 'The timeline changed while audio was saving'
          : 'The open project changed while audio was saving');
      } catch (error) {
        console.warn(`[TimelineDuration] persist failed reason="${reason}"`, error);
        if (owner) throw audioSaveFailure(error);
        return false;
      }
      if (owner && !owner.isCurrent()) return false;
      if (owner) publish();
      await loadProjects().catch((error) => console.warn(`[TimelineDuration] refresh failed reason="${reason}"`, error));
      return true;
    },
    [
      currentProjectDataRef,
      currentProjectId,
      currentProjectIdRef,
      handleEditorRawVideoPathChange,
      loadProjects,
      setComposition,
      setCurrentProjectData,
      setEditorPreviewDuration,
      setSegment,
    ],
  );

  const updatePlaceholderProjectDuration = useCallback(
    async (requestedDuration: number, reason: string) => {
      const ownerId = currentProjectIdRef.current;
      const ownerEpoch = narrationProjectEpoch(currentProjectIdRef);
      const isCurrent = () => ownerId !== null && currentProjectIdRef.current === ownerId
        && narrationProjectEpoch(currentProjectIdRef) === ownerEpoch;
      if (!isCurrent()) return;
      const currentSegment = segmentRef.current;
      if (!currentSegment) return;
      const currentComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      const contentEnd = getTimelineContentEnd(
        currentSegment,
        currentComposition?.audioSegments,
        currentComposition?.narrationSegments,
      );
      const nextDuration = Math.max(requestedDuration, contentEnd, 1);
      let nextSegment = resizeSegmentDuration(currentSegment, nextDuration);
      let nextComposition = resizeCompositionRootDuration(
        currentComposition,
        nextSegment,
        nextDuration,
      );
      let nextRawVideoPath: string | undefined;
      if (nextComposition) {
        const placeholder = await createAudioPlaceholderVideo(
          nextDuration,
          "placeholder-project-duration",
        );
        if (!isCurrent()) return;
        nextRawVideoPath = placeholder.path;
        nextSegment = { ...nextSegment, mediaMode: undefined };
        nextComposition = {
          ...nextComposition,
          clips: nextComposition.clips.map((clip) =>
            clip.id === "root"
              ? {
                  ...clip,
                  duration: nextDuration,
                  rawVideoPath: placeholder.path,
                  segment: nextSegment,
                }
              : clip,
          ),
          globalSegment: nextComposition.globalSegment
            ? nextSegment
            : nextComposition.globalSegment,
          placeholderVideoForAudio: currentComposition?.placeholderVideoForAudio,
          placeholderVideoForSubtitles:
            currentComposition?.placeholderVideoForSubtitles,
          timelineOnly: false,
        };
      }
      await persistTimelineWorkspaceState(
        nextSegment,
        nextComposition,
        nextDuration,
        reason,
        nextRawVideoPath,
      );
      if (!isCurrent()) return;
      if (nextRawVideoPath) {
        const mediaUrl = await getMediaServerUrl(nextRawVideoPath);
        if (!isCurrent()) return;
        const loadedUrl = await videoControllerRef.current?.loadVideo({
          videoUrl: mediaUrl,
          initialTime: Math.min(currentTime, nextDuration),
          debugLabel: "placeholder-project-duration",
        });
        if (!isCurrent()) return;
        setCurrentVideo(loadedUrl ?? mediaUrl);
      }
    },
    [
      composition,
      currentProjectDataRef,
      currentProjectIdRef,
      currentTime,
      persistTimelineWorkspaceState,
      segmentRef,
      setCurrentVideo,
      videoControllerRef,
    ],
  );

  const finalizeNarrationAudioSegments = useCallback(async () => {
    if (isPlaceholderBackedProject && segmentRef.current) {
      await updatePlaceholderProjectDuration(
        segmentRef.current.trimEnd,
        "subtitle-narration-finalize",
      );
      return;
    }
    persistCurrentComposition("subtitle-narration-finalize");
  }, [
    isPlaceholderBackedProject,
    persistCurrentComposition,
    segmentRef,
    updatePlaceholderProjectDuration,
  ]);

  return {
    applyCurrentComposition,
    applyNarrationAudioSegments,
    finalizeNarrationAudioSegments,
    persistCurrentComposition,
    persistTimelineWorkspaceState,
    updateCurrentMusicSegments,
    updateCurrentNarrationSegments,
    updatePlaceholderProjectDuration,
  };
}
