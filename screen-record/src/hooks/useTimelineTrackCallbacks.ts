import { useCallback, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import type {
  AudioGainPoint,
  ImportedAudioSegment,
  NarrationSegment,
  Project,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";
import { getTimelineContentEnd, resizeSegmentDuration } from "@/lib/timelineDuration";

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

interface TimelineTrackCallbacksOptions {
  applyCurrentComposition: (composition: ProjectComposition, reason: string) => void;
  composition: ProjectComposition | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  duration: number;
  isPlaceholderBackedProject: boolean;
  persistCurrentComposition: (reason: string) => void;
  segmentRef: MutableRefObject<VideoSegment | null>;
  setComposition: CompositionSetter;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setEditorPreviewDuration: (duration: number) => void;
  setSegment: SegmentSetter;
  updateCurrentMusicSegments: (
    updater: (segments: ImportedAudioSegment[]) => ImportedAudioSegment[],
    reason: string,
    options?: { persist: boolean },
  ) => void;
  updateCurrentNarrationSegments: (
    updater: (segments: NarrationSegment[]) => NarrationSegment[],
    reason: string,
    options?: { persist: boolean },
  ) => void;
  updatePlaceholderProjectDuration: (
    duration: number,
    reason: string,
  ) => Promise<void>;
}

export function useTimelineTrackCallbacks({
  applyCurrentComposition,
  composition,
  currentProjectDataRef,
  duration,
  isPlaceholderBackedProject,
  persistCurrentComposition,
  segmentRef,
  setComposition,
  setCurrentProjectData,
  setEditorPreviewDuration,
  setSegment,
  updateCurrentMusicSegments,
  updateCurrentNarrationSegments,
  updatePlaceholderProjectDuration,
}: TimelineTrackCallbacksOptions) {
  const handleUpdateAudioSegment = useCallback((
    id: string,
    patch: Partial<ImportedAudioSegment>,
  ) => {
    if (isPlaceholderBackedProject && segmentRef.current) {
      const baseComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) return;
      const nextAudioSegments = (baseComposition.audioSegments ?? []).map((segment) =>
        segment.id === id ? { ...segment, ...patch } : segment,
      );
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
      const nextSegment = resizeSegmentDuration(segmentRef.current, nextDuration);
      const nextComposition = {
        ...baseComposition,
        audioSegments: nextAudioSegments,
        clips: baseComposition.clips.map((clip) =>
          clip.id === "root"
            ? { ...clip, duration: nextDuration, segment: nextSegment }
            : clip,
        ),
        globalSegment: baseComposition.globalSegment
          ? nextSegment
          : baseComposition.globalSegment,
        placeholderVideoForSubtitles: baseComposition.placeholderVideoForSubtitles,
        timelineOnly: false,
      };
      setSegment(nextSegment);
      setEditorPreviewDuration(nextDuration);
      setComposition(nextComposition);
      currentProjectDataRef.current = currentProjectDataRef.current
        ? {
            ...currentProjectDataRef.current,
            composition: nextComposition,
            duration: nextDuration,
            segment: nextSegment,
          }
        : currentProjectDataRef.current;
      setCurrentProjectData((prev) =>
        prev
          ? {
              ...prev,
              composition: nextComposition,
              duration: nextDuration,
              segment: nextSegment,
            }
          : prev,
      );
      return;
    }
    updateCurrentMusicSegments(
      (segments) =>
        segments.map((segment) =>
          segment.id === id ? { ...segment, ...patch } : segment,
        ),
      "update-audio-segment",
      { persist: false },
    );
  }, [
    composition,
    currentProjectDataRef,
    duration,
    isPlaceholderBackedProject,
    segmentRef,
    setComposition,
    setCurrentProjectData,
    setEditorPreviewDuration,
    setSegment,
    updateCurrentMusicSegments,
  ]);

  const handleDeleteAudioSegments = useCallback((ids: string[]) => {
    const idSet = new Set(ids);
    updateCurrentMusicSegments(
      (segments) => segments.filter((segment) => !idSet.has(segment.id)),
      "delete-audio-segments",
      { persist: true },
    );
  }, [updateCurrentMusicSegments]);

  const handleCommitAudioSegments = useCallback(() => {
    if (isPlaceholderBackedProject && segmentRef.current) {
      void updatePlaceholderProjectDuration(
        segmentRef.current.trimEnd,
        "commit-audio-segment-edit",
      );
      return;
    }
    persistCurrentComposition("commit-audio-segment-edit");
  }, [
    isPlaceholderBackedProject,
    persistCurrentComposition,
    segmentRef,
    updatePlaceholderProjectDuration,
  ]);

  const handleUpdateAudioTrackVolumePoints = useCallback((points: AudioGainPoint[]) => {
    const baseComposition = composition ?? currentProjectDataRef.current?.composition ?? null;
    if (!baseComposition) return;
    applyCurrentComposition(
      { ...baseComposition, audioTrackVolumePoints: points },
      "update-audio-track-volume",
    );
  }, [applyCurrentComposition, composition, currentProjectDataRef]);

  const handleUpdateNarrationSegment = useCallback((
    id: string,
    patch: Partial<NarrationSegment>,
  ) => {
    updateCurrentNarrationSegments(
      (segments) =>
        segments.map((segment) =>
          segment.id === id ? { ...segment, ...patch } : segment,
        ),
      "update-narration-segment",
      { persist: false },
    );
  }, [updateCurrentNarrationSegments]);

  const handleDeleteNarrationSegments = useCallback((ids: string[]) => {
    const idSet = new Set(ids);
    updateCurrentNarrationSegments(
      (segments) => segments.filter((segment) => !idSet.has(segment.id)),
      "delete-narration-segments",
      { persist: true },
    );
  }, [updateCurrentNarrationSegments]);

  const handleCommitNarrationSegments = useCallback(() => {
    if (isPlaceholderBackedProject && segmentRef.current) {
      void updatePlaceholderProjectDuration(
        segmentRef.current.trimEnd,
        "commit-narration-segment-edit",
      );
      return;
    }
    persistCurrentComposition("commit-narration-segment-edit");
  }, [
    isPlaceholderBackedProject,
    persistCurrentComposition,
    segmentRef,
    updatePlaceholderProjectDuration,
  ]);

  const handleUpdateNarrationTrackVolumePoints = useCallback((points: AudioGainPoint[]) => {
    const baseComposition = composition ?? currentProjectDataRef.current?.composition ?? null;
    if (!baseComposition) return;
    applyCurrentComposition(
      { ...baseComposition, narrationTrackVolumePoints: points },
      "update-narration-track-volume",
    );
  }, [applyCurrentComposition, composition, currentProjectDataRef]);

  return {
    handleCommitAudioSegments,
    handleCommitNarrationSegments,
    handleDeleteAudioSegments,
    handleDeleteNarrationSegments,
    handleUpdateAudioSegment,
    handleUpdateAudioTrackVolumePoints,
    handleUpdateNarrationSegment,
    handleUpdateNarrationTrackVolumePoints,
  };
}
