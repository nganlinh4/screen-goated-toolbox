import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Translations } from "@/i18n";
import { useSubtitleTranslation } from "@/hooks/useSubtitleTranslation";
import type { SubtitleNarrationGroupPreview } from "@/hooks/useSubtitleNarration";
import { createManualSubtitleSegment } from "@/lib/subtitleDefaults";
import { importSubtitleFileIntoSegment, saveAudioSubtitleSrts, saveSubtitleSrt } from "@/lib/subtitleSrt";
import {
  deriveSelectionRangeFromIds,
  mergeTextSegmentsInRange,
  type TrackSelectionRange,
} from "@/lib/timelineSegmentSelection";
import {
  addSubtitleAcrossTracks,
  mergeSubtitleSelectionAcrossTracks,
} from "@/lib/subtitleTrackMutations";
import { getSubtitleTracks, getVisibleSubtitleSegments, updateAllSubtitleTracks } from "@/lib/subtitleTracks";
import { inferAudioSourceGroupAtRange } from "@/lib/subtitleSourceGroups";
import { isScreenRecordTestHarnessEnabled } from "@/testHarness/browserIpcMock";
import type {
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  TextSegment,
  VideoSegment,
} from "@/types/video";
import type { ActivePanel } from "./sidepanel";

interface UseEditorMainTimelineStateOptions {
  t: Translations;
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  composition: ProjectComposition | null;
  setComposition: (
    composition:
      | ProjectComposition
      | null
      | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => void;
  currentTime: number;
  duration: number;
  editingSubtitleId: string | null;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  onSelectedTextIdsChange?: (ids: string[]) => void;
  onSelectedSubtitleIdsChange?: (ids: string[]) => void;
  projectResetKey?: string | null;
  currentProjectName?: string | null;
  narrationSegments?: NarrationSegment[];
  onDeleteAudioSegments?: (ids: string[]) => void;
  onDeleteNarrationSegments?: (ids: string[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

function getNarrationTimelineDuration(segment: NarrationSegment) {
  const playbackRate = Number.isFinite(segment.playbackRate ?? 1)
    ? Math.max(0.25, segment.playbackRate ?? 1)
    : 1;
  return Math.max(0.05, (segment.outPoint - segment.inPoint) / playbackRate);
}

function alignSubtitleTracksToNarrationTiming(
  segment: VideoSegment,
  timingBySubtitleId: ReadonlyMap<string, { startTime: number; endTime: number }>,
): VideoSegment {
  if (timingBySubtitleId.size === 0) return segment;
  return updateAllSubtitleTracks(segment, (track) => ({
    ...track,
    segments: track.segments.map((subtitle) => {
      const timing = timingBySubtitleId.get(subtitle.id);
      if (!timing) return subtitle;
      return {
        ...subtitle,
        startTime: timing.startTime,
        endTime: timing.endTime,
      };
    }),
  }));
}

export function useEditorMainTimelineState({
  t,
  activePanel,
  setActivePanel,
  segment,
  setSegment,
  composition,
  setComposition,
  currentTime,
  duration,
  editingSubtitleId,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingSubtitleId,
  setEditingKeystrokeSegmentId,
  setEditingPointerId,
  onSelectedTextIdsChange,
  onSelectedSubtitleIdsChange,
  projectResetKey,
  currentProjectName,
  narrationSegments,
  onDeleteAudioSegments,
  onDeleteNarrationSegments,
  beginBatch,
  commitBatch,
}: UseEditorMainTimelineStateOptions) {
  const [selectedTextIds, setSelectedTextIds] = useState<string[]>([]);
  const [selectedSubtitleIds, setSelectedSubtitleIds] = useState<string[]>([]);
  const [selectedSubtitleRange, setSelectedSubtitleRange] = useState<TrackSelectionRange | null>(null);
  const [selectedPointerIds, setSelectedPointerIds] = useState<string[]>([]);
  const [selectedKeystrokeIds, setSelectedKeystrokeIds] = useState<string[]>([]);
  const [selectedWebcamIds, setSelectedWebcamIds] = useState<string[]>([]);
  const [selectedAudioSegmentIds, setSelectedAudioSegmentIds] = useState<string[]>([]);
  const [selectedAudioSegmentRange, setSelectedAudioSegmentRange] = useState<TrackSelectionRange | null>(null);
  const [selectedNarrationSegmentIds, setSelectedNarrationSegmentIds] = useState<string[]>([]);
  const [selectedNarrationSegmentRange, setSelectedNarrationSegmentRange] = useState<TrackSelectionRange | null>(null);
  const [narrationGroupPreview, setNarrationGroupPreview] = useState<SubtitleNarrationGroupPreview | null>(null);
  const [clearSignal, setClearSignal] = useState(0);
  const exportSubtitleSrtInFlightRef = useRef(false);
  const lastProjectResetKeyRef = useRef<string | null | undefined>(undefined);

  const selectedAudioSegmentIdSet = useMemo(
    () => new Set(selectedAudioSegmentIds),
    [selectedAudioSegmentIds],
  );
  const selectedNarrationSegmentIdSet = useMemo(
    () => new Set(selectedNarrationSegmentIds),
    [selectedNarrationSegmentIds],
  );
  const previewAudioSegments = useMemo<ImportedAudioSegment[]>(
    () => [
      ...(composition?.audioSegments ?? []).map((segment) => ({
        ...segment,
        previewTrackKind: "imported" as const,
      })),
      ...(narrationSegments ?? []).map((segment) => ({
        ...segment,
        previewTrackKind: "narration" as const,
      })),
    ],
    [composition?.audioSegments, narrationSegments],
  );
  const subtitleTranslation = useSubtitleTranslation({
    t,
    projectResetKey,
    segment,
    setSegment: setSegment as (segment: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null)) => void,
    composition,
    setComposition,
    selectedSubtitleIds,
    editingSubtitleId,
    setActivePanel,
  });

  const handleTextSelectionChange = useCallback((ids: string[]) => {
    setSelectedTextIds(ids);
    onSelectedTextIdsChange?.(ids);
    if (ids.length > 0) setActivePanel('text');
  }, [onSelectedTextIdsChange, setActivePanel]);
  const handleSubtitleSelectionChange = useCallback((ids: string[]) => {
    setSelectedSubtitleIds(ids);
    onSelectedSubtitleIdsChange?.(ids);
    if (ids.length > 0) setActivePanel('subtitles');
  }, [onSelectedSubtitleIdsChange, setActivePanel]);
  const handleSubtitleRangeChange = useCallback((range: TrackSelectionRange | null) => {
    setSelectedSubtitleRange(range);
    if (range) setActivePanel('subtitles');
  }, [setActivePanel]);
  const handlePointerSelectionChange = useCallback((ids: string[]) => setSelectedPointerIds(ids), []);
  const handleKeystrokeSelectionChange = useCallback((ids: string[]) => setSelectedKeystrokeIds(ids), []);
  const handleWebcamSelectionChange = useCallback((ids: string[]) => setSelectedWebcamIds(ids), []);
  const handleAudioSelectionChange = useCallback((ids: string[]) => {
    setSelectedAudioSegmentIds(ids);
    if (ids.length > 0) setActivePanel('audio');
  }, [setActivePanel]);
  const handleAudioRangeChange = useCallback((range: TrackSelectionRange | null) => {
    setSelectedAudioSegmentRange(range);
    if (range) setActivePanel('audio');
  }, [setActivePanel]);
  const handleNarrationSelectionChange = useCallback((ids: string[]) => {
    setSelectedNarrationSegmentIds(ids);
    if (ids.length > 0) setActivePanel('audio');
  }, [setActivePanel]);
  const handleNarrationRangeChange = useCallback((range: TrackSelectionRange | null) => {
    setSelectedNarrationSegmentRange(range);
    if (range) setActivePanel('audio');
  }, [setActivePanel]);
  const handleAudioSegmentClick = useCallback((id: string) => {
    setSelectedAudioSegmentIds([id]);
    setSelectedAudioSegmentRange(null);
    setActivePanel('audio');
  }, [setActivePanel]);
  const handleNarrationSegmentClick = useCallback((id: string) => {
    setSelectedNarrationSegmentIds([id]);
    setSelectedNarrationSegmentRange(null);
    setActivePanel('audio');
  }, [setActivePanel]);
  const handleDeleteAudioSegmentsForTimeline = useCallback((ids: string[]) => {
    onDeleteAudioSegments?.(ids);
    setSelectedAudioSegmentIds((current) =>
      current.filter((id) => !ids.includes(id)),
    );
  }, [onDeleteAudioSegments]);
  const handleDeleteNarrationSegmentsForTimeline = useCallback((ids: string[]) => {
    onDeleteNarrationSegments?.(ids);
    setSelectedNarrationSegmentIds((current) =>
      current.filter((id) => !ids.includes(id)),
    );
  }, [onDeleteNarrationSegments]);

  useEffect(() => {
    if (!isScreenRecordTestHarnessEnabled()) return;
    const testWindow = window as Window & {
      __SGT_EDITOR_TEST__?: { selectFirstAudioSegment: () => boolean };
    };
    const hooks = {
      selectFirstAudioSegment: () => {
        const id = composition?.audioSegments?.[0]?.id;
        if (!id) return false;
        setSelectedAudioSegmentIds([id]);
        setSelectedAudioSegmentRange(null);
        setActivePanel('audio');
        return true;
      },
    };
    testWindow.__SGT_EDITOR_TEST__ = hooks;
    return () => {
      if (testWindow.__SGT_EDITOR_TEST__ === hooks) delete testWindow.__SGT_EDITOR_TEST__;
    };
  }, [composition?.audioSegments, setActivePanel]);

  const handleAlignSubtitlesToNarration = useCallback(() => {
    if (!segment || !narrationSegments?.length) return;
    const selectedNarrations = narrationSegments
      .filter((narration) =>
        selectedNarrationSegmentIdSet.has(narration.id) &&
        !!narration.sourceSubtitleId,
      )
      .sort((left, right) => left.startTime - right.startTime || left.id.localeCompare(right.id));
    if (selectedNarrations.length === 0) return;

    const timingBySubtitleId = new Map<string, { startTime: number; endTime: number }>();
    for (const narration of selectedNarrations) {
      const subtitleId = narration.sourceSubtitleId;
      if (!subtitleId) continue;
      const startTime = Math.max(0, narration.startTime);
      timingBySubtitleId.set(subtitleId, {
        startTime,
        endTime: startTime + getNarrationTimelineDuration(narration),
      });
    }
    if (timingBySubtitleId.size === 0) return;

    beginBatch();
    setSegment(alignSubtitleTracksToNarrationTiming(segment, timingBySubtitleId));
    commitBatch();
  }, [
    beginBatch,
    commitBatch,
    narrationSegments,
    segment,
    selectedNarrationSegmentIdSet,
    setSegment,
  ]);

  const totalSelectedCount =
    selectedTextIds.length +
    selectedSubtitleIds.length +
    selectedPointerIds.length +
    selectedKeystrokeIds.length +
    selectedWebcamIds.length +
    selectedAudioSegmentIds.length +
    selectedNarrationSegmentIds.length;

  const clearAllSelections = useCallback(() => {
    setSelectedTextIds([]);
    setSelectedSubtitleIds([]);
    onSelectedTextIdsChange?.([]);
    onSelectedSubtitleIdsChange?.([]);
    setSelectedSubtitleRange(null);
    setSelectedPointerIds([]);
    setSelectedKeystrokeIds([]);
    setSelectedWebcamIds([]);
    setSelectedAudioSegmentIds([]);
    setSelectedAudioSegmentRange(null);
    setSelectedNarrationSegmentIds([]);
    setSelectedNarrationSegmentRange(null);
    setClearSignal(c => c + 1);
  }, [onSelectedSubtitleIdsChange, onSelectedTextIdsChange]);

  const clearTimelineFocus = useCallback(() => {
    clearAllSelections();
    setEditingKeyframeId(null);
    setEditingTextId(null);
    setEditingSubtitleId(null);
    setEditingKeystrokeSegmentId(null);
    setEditingPointerId(null);
  }, [
    clearAllSelections,
    setEditingKeyframeId,
    setEditingKeystrokeSegmentId,
    setEditingPointerId,
    setEditingSubtitleId,
    setEditingTextId,
  ]);

  useEffect(() => {
    const nextKey = projectResetKey ?? null;
    if (lastProjectResetKeyRef.current === undefined) {
      lastProjectResetKeyRef.current = nextKey;
      return;
    }
    if (lastProjectResetKeyRef.current === nextKey) {
      return;
    }
    lastProjectResetKeyRef.current = nextKey;
    clearAllSelections();
    setEditingTextId(null);
    setEditingSubtitleId(null);
    setEditingKeystrokeSegmentId(null);
    setEditingPointerId(null);
  }, [
    clearAllSelections,
    projectResetKey,
    setEditingKeystrokeSegmentId,
    setEditingPointerId,
    setEditingSubtitleId,
    setEditingTextId,
  ]);

  const textMergeRange = useMemo(
    () => deriveSelectionRangeFromIds(selectedTextIds, segment?.textSegments ?? []),
    [segment?.textSegments, selectedTextIds],
  );
  const subtitleMergeRange = useMemo(
    () => deriveSelectionRangeFromIds(selectedSubtitleIds, getVisibleSubtitleSegments(segment)),
    [segment, selectedSubtitleIds],
  );

  const mergeTarget = useMemo(() => {
    if (activePanel === 'subtitles' && selectedSubtitleIds.length >= 2) return 'subtitles' as const;
    if (activePanel === 'text' && selectedTextIds.length >= 2) return 'text' as const;
    if (selectedSubtitleIds.length >= 2) return 'subtitles' as const;
    if (selectedTextIds.length >= 2) return 'text' as const;
    return null;
  }, [activePanel, selectedSubtitleIds.length, selectedTextIds.length]);

  const handleMergeSelection = useCallback(() => {
    if (!segment || !mergeTarget) return;

    if (mergeTarget === 'text' && textMergeRange) {
      const result = mergeTextSegmentsInRange<TextSegment>(
        segment.textSegments,
        textMergeRange,
        '\n',
      );
      if (!result.merged) return;
      setSegment({
        ...segment,
        textSegments: result.segments,
      });
      setEditingTextId(result.merged.id);
      setEditingSubtitleId(null);
      setActivePanel('text');
      clearAllSelections();
      return;
    }

    if (mergeTarget === 'subtitles' && subtitleMergeRange) {
      const result = mergeSubtitleSelectionAcrossTracks(segment, subtitleMergeRange);
      if (!result.mergedId) return;
      setSegment(result.segment);
      setEditingSubtitleId(result.mergedId);
      setActivePanel('subtitles');
      clearAllSelections();
    }
  }, [
    clearAllSelections,
    mergeTarget,
    segment,
    setActivePanel,
    setEditingSubtitleId,
    setEditingTextId,
    setSegment,
    subtitleMergeRange,
    textMergeRange,
  ]);

  const handleAddSubtitle = useCallback((atTime?: number) => {
    if (!segment) return;
    const subtitle = createManualSubtitleSegment(atTime ?? currentTime, duration);
    const sourceGroup = inferAudioSourceGroupAtRange(
      subtitle.startTime,
      subtitle.endTime,
      composition?.audioSegments,
    );
    setSegment(addSubtitleAcrossTracks(segment, {
      ...subtitle,
      sourceGroup: {
        ...sourceGroup,
        assignment: sourceGroup.kind === 'unassigned' ? 'manual' : sourceGroup.assignment,
      },
    }));
    setEditingSubtitleId(subtitle.id);
    setActivePanel('subtitles');
  }, [composition?.audioSegments, currentTime, duration, segment, setActivePanel, setEditingSubtitleId, setSegment]);

  const handleImportSubtitleFile = useCallback(async (file: File) => {
    if (!segment) return;
    try {
      const content = await file.text();
      const { segment: nextSegment, subtitles: importedSubtitles } = importSubtitleFileIntoSegment(
        segment,
        { fileName: file.name, content, mimeType: file.type },
        duration,
      );
      if (importedSubtitles.length === 0) {
        console.error('[SubtitleImport] import failed: no valid subtitles found');
        return;
      }
      setSegment(nextSegment);
      clearAllSelections();
      setEditingSubtitleId(importedSubtitles[0]?.id ?? null);
      setActivePanel('subtitles');
    } catch (error) {
      console.error('[SubtitleImport] import failed:', error);
    }
  }, [
    clearAllSelections,
    duration,
    segment,
    setActivePanel,
    setEditingSubtitleId,
    setSegment,
  ]);

  const visibleSubtitleSegments = useMemo(
    () => getVisibleSubtitleSegments(segment),
    [segment],
  );
  const allSubtitleTracks = useMemo(
    () => getSubtitleTracks(segment),
    [segment],
  );
  const canExportAudioSubtitleSrt = visibleSubtitleSegments.some(
    (subtitle) =>
      subtitle.sourceGroup?.kind === 'audio' ||
      subtitle.provenance?.sourceKind === 'audio',
  );
  useEffect(() => {
    const visibleIds = new Set(visibleSubtitleSegments.map((subtitle) => subtitle.id));
    setSelectedSubtitleIds((prev) => {
      const next = prev.filter((id) => visibleIds.has(id));
      if (next.length === prev.length) {
        return prev;
      }
      onSelectedSubtitleIdsChange?.(next);
      return next;
    });
    if (editingSubtitleId && !visibleIds.has(editingSubtitleId)) {
      setEditingSubtitleId(null);
    }
  }, [editingSubtitleId, onSelectedSubtitleIdsChange, setEditingSubtitleId, visibleSubtitleSegments]);
  const canExportSubtitleSrt = visibleSubtitleSegments.length > 0;

  const handleExportSubtitleSrt = useCallback(async () => {
    if (!visibleSubtitleSegments.length) return;
    if (exportSubtitleSrtInFlightRef.current) return;
    exportSubtitleSrtInFlightRef.current = true;
    try {
      await saveSubtitleSrt(
        visibleSubtitleSegments,
        selectedSubtitleRange,
        currentProjectName
          ? `${currentProjectName}${selectedSubtitleRange ? '-subtitles-range' : '-subtitles'}`
          : selectedSubtitleRange
            ? 'subtitles-range'
            : 'subtitles',
        t.subtitleSrtSavedTo,
      );
    } catch (error) {
      console.error('[SubtitleSrt] Failed to save subtitle file:', error);
    } finally {
      exportSubtitleSrtInFlightRef.current = false;
    }
  }, [currentProjectName, selectedSubtitleRange, t.subtitleSrtSavedTo, visibleSubtitleSegments]);

  const handleExportMusicSubtitleSrts = useCallback(async () => {
    if (!canExportAudioSubtitleSrt) return;
    if (exportSubtitleSrtInFlightRef.current) return;
    exportSubtitleSrtInFlightRef.current = true;
    try {
      await saveAudioSubtitleSrts(
        visibleSubtitleSegments,
        composition?.audioSegments ?? [],
        t.subtitleSrtSavedTo,
      );
    } catch (error) {
      console.error('[SubtitleSrt] Failed to save audio subtitle files:', error);
    } finally {
      exportSubtitleSrtInFlightRef.current = false;
    }
  }, [canExportAudioSubtitleSrt, composition?.audioSegments, t.subtitleSrtSavedTo, visibleSubtitleSegments]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
        return;
      }
      if (event.code !== 'KeyM') return;
      if (!mergeTarget) return;
      event.preventDefault();
      handleMergeSelection();
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [handleMergeSelection, mergeTarget]);

  return {
    allSubtitleTracks,
    canExportAudioSubtitleSrt,
    canExportSubtitleSrt,
    clearAllSelections,
    clearSignal,
    clearTimelineFocus,
    handleAddSubtitle,
    handleAlignSubtitlesToNarration,
    handleAudioRangeChange,
    handleAudioSegmentClick,
    handleAudioSelectionChange,
    handleDeleteAudioSegmentsForTimeline,
    handleDeleteNarrationSegmentsForTimeline,
    handleExportMusicSubtitleSrts,
    handleExportSubtitleSrt,
    handleImportSubtitleFile,
    handleKeystrokeSelectionChange,
    handleMergeSelection,
    handleNarrationRangeChange,
    handleNarrationSegmentClick,
    handleNarrationSelectionChange,
    handlePointerSelectionChange,
    handleSubtitleRangeChange,
    handleSubtitleSelectionChange,
    handleTextSelectionChange,
    handleWebcamSelectionChange,
    mergeTarget,
    narrationGroupPreview,
    previewAudioSegments,
    selectedAudioSegmentIdSet,
    selectedAudioSegmentRange,
    selectedNarrationSegmentIdSet,
    selectedNarrationSegmentRange,
    selectedSubtitleIds,
    selectedSubtitleRange,
    selectedTextIds,
    setNarrationGroupPreview,
    subtitleTranslation,
    totalSelectedCount,
    visibleSubtitleSegments,
  };
}
