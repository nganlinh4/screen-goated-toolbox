import type { Dispatch, MutableRefObject, SetStateAction } from "react";
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
  persistTimelineWorkspaceState: (
    nextSegment: VideoSegment,
    nextComposition: ProjectComposition | null,
    nextDuration: number,
    reason: string,
    rawVideoPath?: string,
  ) => Promise<void>;
  projects: {
    handleLoadProject: (projectId: string) => Promise<unknown>;
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
  setSegment: SegmentSetter;
  setSubtitleSource: (source: SubtitleSource) => void;
  updateCurrentMusicSegments: (
    updater: (segments: ImportedAudioSegment[]) => ImportedAudioSegment[],
    reason: string,
    options?: { persist: boolean },
  ) => void;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
}

export function useAppMediaImports({
  composition,
  currentProjectDataRef,
  currentProjectIdRef,
  currentTime,
  duration,
  isPlaceholderBackedProject,
  persistTimelineWorkspaceState,
  projects,
  segment,
  segmentRef,
  selectedSubtitleIdsRef,
  selectedTextIdsRef,
  setActivePanel,
  setComposition,
  setCurrentVideo,
  setEditingSubtitleId,
  setSegment,
  setSubtitleSource,
  updateCurrentMusicSegments,
  videoControllerRef,
}: AppMediaImportsOptions) {
  const { isImporting, importVideo, importVideoPath } = useVideoImport({
    onProjectCreated: async (project) => {
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      await projects.handleLoadProject(project.id);
    },
  });

  const { isImporting: isImportingAudio, importAudio, importAudios, importAudioPaths } =
    useImportedAudioImport({
      getCurrentProjectId: () =>
        currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? null,
      onAttachToCurrentProject: async (segments) => {
        if (isPlaceholderBackedProject && segmentRef.current) {
          const baseComposition =
            currentProjectDataRef.current?.composition ?? composition ?? null;
          if (!baseComposition) return;
          const existingSegments = baseComposition.audioSegments ?? [];
          const appendStart = existingSegments.reduce((maxEnd, segment) => {
            const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
            return Math.max(maxEnd, segment.startTime + visibleDuration);
          }, 0);
          let cursor = appendStart;
          const placedSegments = segments.map((segment) => {
            const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
            const placed = { ...segment, startTime: cursor };
            cursor += visibleDuration;
            return placed;
          });
          const nextAudioSegments = [...existingSegments, ...placedSegments];
          const nextDuration = Math.max(
            duration,
            segmentRef.current.trimEnd,
            getTimelineContentEnd(
              segmentRef.current,
              nextAudioSegments,
              baseComposition.narrationSegments,
            ),
            1,
          );
          const nextSegment = {
            ...resizeSegmentDuration(segmentRef.current, nextDuration),
            mediaMode: undefined,
          };
          const placeholder = await createAudioPlaceholderVideo(
            nextDuration,
            "attach-audio-to-placeholder-project",
          );
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
          await persistTimelineWorkspaceState(
            nextSegment,
            nextComposition,
            nextDuration,
            "attach-audio-to-placeholder-project",
            placeholder.path,
          );
          const mediaUrl = await getMediaServerUrl(placeholder.path);
          const loadedUrl = await videoControllerRef.current?.loadVideo({
            videoUrl: mediaUrl,
            initialTime: currentTime,
            debugLabel: "attach-audio-to-placeholder-project",
          });
          setCurrentVideo(loadedUrl ?? mediaUrl);
          setSubtitleSource("audio");
          return;
        }
        updateCurrentMusicSegments(
          (existingSegments) => {
            const appendStart = existingSegments.reduce((maxEnd, segment) => {
              const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
              return Math.max(maxEnd, segment.startTime + visibleDuration);
            }, 0);
            let cursor = appendStart;
            const placedSegments = segments.map((segment) => {
              const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
              const placed = { ...segment, startTime: cursor };
              cursor += visibleDuration;
              return placed;
            });
            return [...existingSegments, ...placedSegments];
          },
          "attach-audio-to-current-project",
          { persist: true },
        );
        if (composition?.placeholderVideoForAudio) {
          setSubtitleSource("audio");
        }
      },
      onCreateAudioProject: async (project) => {
        logToHost(`[AudioImport][Frontend] load project start id="${project.id}"`);
        projects.setShowProjectsDialog(false);
        await projects.loadProjects();
        logToHost(`[AudioImport][Frontend] project list refreshed id="${project.id}"`);
        await projects.handleLoadProject(project.id);
        currentProjectIdRef.current = project.id;
        if (project.composition) {
          setComposition(project.composition);
        }
        logToHost(`[AudioImport][Frontend] load project complete id="${project.id}"`);
      },
    });

  const {
    isImporting: isImportingSubtitle,
    importSubtitleFile,
    importSubtitlePayload,
  } = useSubtitleImport({
    segment,
    duration,
    getCurrentProjectId: () =>
      currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? null,
    setSegment,
    setActivePanel,
    setEditingSubtitleId,
    onImportedIntoCurrentProject: () => {
      selectedSubtitleIdsRef.current = [];
      selectedTextIdsRef.current = [];
    },
    onCreateSubtitleProject: async (project) => {
      logToHost(`[SubtitleImport][Frontend] load project start id="${project.id}"`);
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      await projects.handleLoadProject(project.id);
      currentProjectIdRef.current = project.id;
      if (project.composition) {
        setComposition(project.composition);
      }
      logToHost(`[SubtitleImport][Frontend] load project complete id="${project.id}"`);
    },
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
