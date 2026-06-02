import { useCallback, type RefObject } from "react";
import type {
  SubtitleSourceGroup,
  VideoSegment,
} from "@/types/video";
import { buildTextSplitPreview } from "@/lib/textSplitPreview";
import {
  deleteSubtitleIdsAcrossTracks,
  duplicateSubtitleAcrossTracks,
  splitSubtitleAcrossTracks,
  updateSubtitleSourceGroupAcrossTracks,
} from "@/lib/subtitleTrackMutations";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";

interface UseTimelineSegmentActionsOptions {
  duration: number;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  setCurrentTime: (time: number) => void;
  videoRef: RefObject<HTMLVideoElement>;
  onSeek?: (time: number) => void;
  onClearTimelineFocus?: () => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useTimelineSegmentActions({
  duration,
  segment,
  setSegment,
  setCurrentTime,
  videoRef,
  onSeek,
  onClearTimelineFocus,
  beginBatch,
  commitBatch,
}: UseTimelineSegmentActionsOptions) {
  const handleEmptyTrackClick = useCallback((time: number) => {
    onClearTimelineFocus?.();
    const nextTime = Math.max(0, Math.min(duration, time));
    if (onSeek) {
      onSeek(nextTime);
      return;
    }
    if (videoRef.current && Math.abs(videoRef.current.currentTime - nextTime) > 0.05) {
      videoRef.current.currentTime = nextTime;
    }
    setCurrentTime(nextTime);
  }, [duration, onClearTimelineFocus, onSeek, setCurrentTime, videoRef]);

  const handleDeletePointerSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const remaining = (segment.cursorVisibilitySegments || []).filter(s => !idSet.has(s.id));
    setSegment({ ...segment, cursorVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleTextSplit = useCallback((id: string, splitTime: number) => {
    if (!segment) return;
    beginBatch();
    const texts = segment.textSegments ?? [];
    const target = texts.find(t => t.id === id);
    if (!target || splitTime <= target.startTime + 0.1 || splitTime >= target.endTime - 0.1) {
      commitBatch();
      return;
    }
    const preview = buildTextSplitPreview({
      text: target.text,
      startTime: target.startTime,
      endTime: target.endTime,
      splitTime,
    });
    if (!preview) {
      commitBatch();
      return;
    }
    const left = {
      ...target,
      endTime: splitTime - 0.01,
      text: preview.leftText,
    };
    const right = {
      ...target,
      id: crypto.randomUUID(),
      startTime: splitTime + 0.01,
      text: preview.rightText,
    };
    setSegment({
      ...segment,
      textSegments: texts.map(t => t.id === id ? left : t).concat(right),
    });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteTextSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const remaining = (segment.textSegments ?? []).filter(t => !idSet.has(t.id));
    setSegment({ ...segment, textSegments: remaining });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDuplicateText = useCallback((id: string) => {
    if (!segment) return;
    const texts = segment.textSegments ?? [];
    const source = texts.find((t) => t.id === id);
    if (!source) return;
    const length = source.endTime - source.startTime;
    if (length <= 0) return;
    const next = texts
      .filter((t) => t.startTime > source.endTime)
      .sort((a, b) => a.startTime - b.startTime)[0];
    const desiredStart = source.endTime;
    const maxEnd = next ? next.startTime - 0.01 : duration;
    const clampedEnd = Math.min(desiredStart + length, maxEnd);
    if (clampedEnd - desiredStart < 0.05) return;
    const duplicate = {
      ...JSON.parse(JSON.stringify(source)),
      id: crypto.randomUUID(),
      startTime: desiredStart,
      endTime: clampedEnd,
    };
    beginBatch();
    setSegment({ ...segment, textSegments: [...texts, duplicate] });
    commitBatch();
  }, [segment, duration, setSegment, beginBatch, commitBatch]);

  const handleDuplicateSubtitle = useCallback((id: string) => {
    if (!segment) return;
    const result = duplicateSubtitleAcrossTracks(segment, id, duration);
    if (!result.newSubtitleId) return;
    beginBatch();
    setSegment(result.segment);
    commitBatch();
  }, [segment, duration, setSegment, beginBatch, commitBatch]);

  const handleSubtitleSplit = useCallback((id: string, splitTime: number) => {
    if (!segment) return;
    beginBatch();
    const subtitles = getVisibleSubtitleSegments(segment);
    const target = subtitles.find((subtitle) => subtitle.id === id);
    if (!target || splitTime <= target.startTime + 0.1 || splitTime >= target.endTime - 0.1) {
      commitBatch();
      return;
    }
    const preview = buildTextSplitPreview({
      text: target.text,
      startTime: target.startTime,
      endTime: target.endTime,
      splitTime,
    });
    if (!preview) {
      commitBatch();
      return;
    }
    const result = splitSubtitleAcrossTracks(segment, id, splitTime);
    setSegment(result.segment);
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteSubtitleSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    setSegment(deleteSubtitleIdsAcrossTracks(segment, ids));
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleAssignSubtitleSourceGroup = useCallback((ids: string[], sourceGroup: SubtitleSourceGroup) => {
    if (!segment || ids.length === 0) return;
    beginBatch();
    setSegment(updateSubtitleSourceGroupAcrossTracks(segment, new Set(ids), sourceGroup));
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteKeystrokeSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const mode = segment.keystrokeMode ?? 'off';
    if (mode === 'keyboard') {
      const remaining = (segment.keyboardVisibilitySegments || []).filter(s => !idSet.has(s.id));
      setSegment({ ...segment, keyboardVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    } else if (mode === 'keyboardMouse') {
      const remaining = (segment.keyboardMouseVisibilitySegments || []).filter(s => !idSet.has(s.id));
      setSegment({ ...segment, keyboardMouseVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    }
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteWebcamSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const remaining = (segment.webcamVisibilitySegments || []).filter(s => !idSet.has(s.id));
    setSegment({ ...segment, webcamVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  return {
    handleAssignSubtitleSourceGroup,
    handleDeleteKeystrokeSegments,
    handleDeletePointerSegments,
    handleDeleteSubtitleSegments,
    handleDeleteTextSegments,
    handleDeleteWebcamSegments,
    handleDuplicateSubtitle,
    handleDuplicateText,
    handleEmptyTrackClick,
    handleSubtitleSplit,
    handleTextSplit,
  };
}
