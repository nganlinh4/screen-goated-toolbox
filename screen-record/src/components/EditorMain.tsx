import React, { useCallback, useDeferredValue, useEffect, useMemo, useState, type MutableRefObject, type RefObject } from "react";
import {
  BackgroundConfig,
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  TextSegment,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import { videoTimeToWallClock } from "@/lib/exportEstimator";
import { PreviewCanvas, type KeystrokeEditFrame } from "@/components/PreviewCanvas";
import { PlaybackControlsRow } from "@/components/PlaybackControlsRow";
import { SidePanel, type ActivePanel } from "@/components/sidepanel/index";
import { TimelineArea } from "@/components/timeline";
import type { CanvasModeToggleProps } from "@/components/CanvasModeToggle";
import { useSettings } from "@/hooks/useSettings";
import type { SubtitleMethod } from "@/hooks/useSubtitleGeneration";
import type { SubtitleSource } from "@/lib/subtitleGenerationPlan";
import { useSubtitleTranslation } from "@/hooks/useSubtitleTranslation";
import { createManualSubtitleSegment } from "@/lib/subtitleDefaults";
import { importSubtitleSrtIntoSegment, saveAudioSubtitleSrts, saveSubtitleSrt } from "@/lib/subtitleSrt";
import type { SubtitleGenerationIndicator } from "@/lib/subtitleGenerationPlan";
import {
  deriveSelectionRangeFromIds,
  mergeTextSegmentsInRange,
  type TrackSelectionRange,
} from "@/lib/timelineSegmentSelection";
import {
  addSubtitleAcrossTracks,
  mergeSubtitleSelectionAcrossTracks,
} from "@/lib/subtitleTrackMutations";
import { getSubtitleTracks, getVisibleSubtitleSegments } from "@/lib/subtitleTracks";
import { inferAudioSourceGroupAtRange } from "@/lib/subtitleSourceGroups";

export interface EditorMainProps {
  // Error
  error: string | null;
  // Overlay mode
  isOverlayMode: boolean;
  // PreviewCanvas props
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  previewCursorClass: string;
  handlePreviewMouseDown: (e: React.MouseEvent<HTMLDivElement>) => void;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  videoRef: RefObject<HTMLVideoElement | null>;
  webcamVideoRef: RefObject<HTMLVideoElement | null>;
  audioRef: RefObject<HTMLAudioElement | null>;
  micAudioRef: RefObject<HTMLAudioElement | null>;
  previousPreloadVideoRef: RefObject<HTMLVideoElement | null>;
  previousPreloadAudioRef: RefObject<HTMLAudioElement | null>;
  nextPreloadVideoRef: RefObject<HTMLVideoElement | null>;
  nextPreloadAudioRef: RefObject<HTMLAudioElement | null>;
  keystrokeOverlayEditFrame: KeystrokeEditFrame | null;
  isKeystrokeOverlaySelected: boolean;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isBuffering: boolean;
  isPreviewPlaying: boolean;
  currentVideo: string | null;
  isTimelineOnly: boolean;
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
  isCropping: boolean;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  setIsCanvasResizeDragging: (dragging: boolean) => void;
  seekIndicatorDir: "left" | "right" | null;
  seekIndicatorKey: number;
  audioResetKey?: number;
  // PlaybackControlsRow props
  isPlaying: boolean;
  isProcessing: boolean;
  isVideoReady: boolean;
  hasAppliedCrop: boolean;
  currentTime: number;
  duration: number;
  handleTogglePlayPause: () => void;
  handleToggleCrop: () => void;
  onSetProjectDuration?: (duration: number) => void;
  customCanvasBaseDimensions: { width: number; height: number };
  getAutoCanvasSelectionConfig: CanvasModeToggleProps["getAutoCanvasSelectionConfig"];
  handleActivateCustomCanvas: () => void;
  handleApplyCanvasRatioPreset: (ratioWidth: number, ratioHeight: number) => void;
  isAutoCanvasDisabled?: boolean;
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment | null) => void;
  composition: ProjectComposition | null;
  setComposition: (
    composition:
      | ProjectComposition
      | null
      | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => void;
  handleToggleKeystrokeMode: () => void;
  handleKeystrokeDelayChange: (delay: number) => void;
  mousePositionsLength: number;
  handleAutoZoom: () => void;
  autoZoomConfig: import("@/types/video").AutoZoomConfig;
  handleAutoZoomConfigChange: (config: import("@/types/video").AutoZoomConfig) => void;
  handleSmartPointerHiding: () => void;
  // SidePanel props
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  editingKeyframeId: number | null;
  zoomFactor: number;
  setZoomFactor: (factor: number) => void;
  handleDeleteKeyframe: () => void;
  throttledUpdateZoom: (updates: { zoomFactor?: number; positionX?: number; positionY?: number }) => void;
  webcamConfig: WebcamConfig;
  setWebcamConfig: React.Dispatch<React.SetStateAction<WebcamConfig>>;
  recentUploads: string[];
  handleRemoveRecentUpload: (url: string) => void;
  handleBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  isBackgroundUploadProcessing: boolean;
  editingTextId: string | null;
  editingSubtitleId: string | null;
  subtitleSource: SubtitleSource;
  onSubtitleSourceChange: (value: SubtitleSource) => void;
  subtitleMethod: SubtitleMethod;
  onSubtitleMethodChange: (value: SubtitleMethod) => void;
  subtitleMethodCapabilities: Array<{ method: SubtitleMethod; available: boolean; reason?: string | null }>;
  canUseSelectedSubtitleMethod: boolean;
  selectedSubtitleMethodReason?: string | null;
  subtitleLanguageHint: string;
  onSubtitleLanguageHintChange: (value: string) => void;
  subtitleGeminiPrompt: string;
  onSubtitleGeminiPromptChange: (value: string) => void;
  subtitleGroqVocabulary: string[];
  onSubtitleGroqVocabularyChange: (value: string[]) => void;
  isGeneratingSubtitles: boolean;
  subtitleStatusMessage?: string | null;
  subtitleGenerationIndicator?: SubtitleGenerationIndicator | null;
  handleGenerateSubtitles: (selectedRange?: TrackSelectionRange | null) => void;
  handleCancelSubtitleGeneration: () => void;
  onApplyNarrationSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  onFinalizeNarrationSegments: () => void | Promise<void>;
  onSelectedTextIdsChange?: (ids: string[]) => void;
  onSelectedSubtitleIdsChange?: (ids: string[]) => void;
  projectResetKey?: string | null;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  currentProjectName?: string | null;
  // TimelineArea props
  thumbnails: string[];
  timelineRef: React.RefObject<HTMLDivElement>;
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  seek: (time: number) => void;
  flushSeek: () => void;
  handleAddText: () => void;
  handleAddKeystrokeSegment: (atTime?: number) => void;
  handleAddPointerSegment: (atTime?: number) => void;
  setTimelineCanvasWidthPx: (width: number) => void;
  // Audio track
  onPickImportedAudioFile?: (file: File) => void;
  onUpdateAudioSegment?: (
    id: string,
    patch: Partial<ImportedAudioSegment>,
  ) => void;
  onDeleteAudioSegments?: (ids: string[]) => void;
  onCommitAudioSegments?: () => void;
  audioTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
  onUpdateAudioTrackVolumePoints?: (points: import("@/types/video").AudioGainPoint[]) => void;
  // Narration track
  narrationSegments?: NarrationSegment[];
  onUpdateNarrationSegment?: (
    id: string,
    patch: Partial<NarrationSegment>,
  ) => void;
  onDeleteNarrationSegments?: (ids: string[]) => void;
  onCommitNarrationSegments?: () => void;
  narrationTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
  onUpdateNarrationTrackVolumePoints?: (points: import("@/types/video").AudioGainPoint[]) => void;
}

export function EditorMain({
  error,
  isOverlayMode,
  previewContainerRef,
  previewCursorClass,
  handlePreviewMouseDown,
  canvasRef,
  tempCanvasRef,
  videoRef,
  webcamVideoRef,
  audioRef,
  micAudioRef,
  previousPreloadVideoRef,
  previousPreloadAudioRef,
  nextPreloadVideoRef,
  nextPreloadAudioRef,
  keystrokeOverlayEditFrame,
  isKeystrokeOverlaySelected,
  isDraggingKeystrokeOverlayRef,
  isResizingKeystrokeOverlayRef,
  isBuffering,
  isPreviewPlaying,
  currentVideo,
  isTimelineOnly,
  isLoadingVideo,
  loadingProgress,
  isRecording,
  recordingDuration,
  isCropping,
  backgroundConfig,
  setBackgroundConfig,
  beginBatch,
  commitBatch,
  setIsCanvasResizeDragging,
  seekIndicatorDir,
  seekIndicatorKey,
  audioResetKey,
  isPlaying,
  isProcessing,
  isVideoReady,
  hasAppliedCrop,
  currentTime,
  duration,
  handleTogglePlayPause,
  handleToggleCrop,
  onSetProjectDuration,
  customCanvasBaseDimensions,
  getAutoCanvasSelectionConfig,
  handleActivateCustomCanvas,
  handleApplyCanvasRatioPreset,
  isAutoCanvasDisabled,
  segment,
  setSegment,
  composition,
  setComposition,
  handleToggleKeystrokeMode,
  handleKeystrokeDelayChange,
  mousePositionsLength,
  handleAutoZoom,
  autoZoomConfig,
  handleAutoZoomConfigChange,
  handleSmartPointerHiding,
  activePanel,
  setActivePanel,
  editingKeyframeId,
  zoomFactor,
  setZoomFactor,
  handleDeleteKeyframe,
  throttledUpdateZoom,
  webcamConfig,
  setWebcamConfig,
  recentUploads,
  handleRemoveRecentUpload,
  handleBackgroundUpload,
  isBackgroundUploadProcessing,
  editingTextId,
  editingSubtitleId,
  subtitleSource,
  onSubtitleSourceChange,
  subtitleMethod,
  onSubtitleMethodChange,
  subtitleMethodCapabilities,
  canUseSelectedSubtitleMethod,
  selectedSubtitleMethodReason,
  subtitleLanguageHint,
  onSubtitleLanguageHintChange,
  subtitleGeminiPrompt,
  onSubtitleGeminiPromptChange,
  subtitleGroqVocabulary,
  onSubtitleGroqVocabularyChange,
  isGeneratingSubtitles,
  subtitleStatusMessage,
  subtitleGenerationIndicator,
  handleGenerateSubtitles,
  handleCancelSubtitleGeneration,
  onApplyNarrationSegments,
  onFinalizeNarrationSegments,
  onSelectedTextIdsChange,
  onSelectedSubtitleIdsChange,
  projectResetKey,
  currentRawVideoPath,
  currentRawMicAudioPath,
  currentProjectName,
  thumbnails,
  timelineRef,
  editingKeystrokeSegmentId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingSubtitleId,
  setEditingKeystrokeSegmentId,
  setEditingPointerId,
  seek,
  flushSeek,
  handleAddText,
  handleAddKeystrokeSegment,
  handleAddPointerSegment,
  setTimelineCanvasWidthPx,
  onPickImportedAudioFile,
  onUpdateAudioSegment,
  onDeleteAudioSegments,
  onCommitAudioSegments,
  audioTrackVolumePoints,
  onUpdateAudioTrackVolumePoints,
  narrationSegments,
  onUpdateNarrationSegment,
  onDeleteNarrationSegments,
  onCommitNarrationSegments,
  narrationTrackVolumePoints,
  onUpdateNarrationTrackVolumePoints,
}: EditorMainProps) {
  const { t } = useSettings();
  const showPlaybackControls = Boolean(
    (currentVideo || isTimelineOnly) && !isLoadingVideo && !isOverlayMode,
  );
  const showPlaybackControlsGhost = Boolean(
    !currentVideo && !isTimelineOnly && !isLoadingVideo && !isCropping,
  );
  const wallClockDuration = useMemo(() => {
    const pts = segment?.speedPoints;
    if (!pts?.length || !duration) return duration;
    return videoTimeToWallClock(duration, pts);
  }, [duration, segment?.speedPoints]);

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
  const deferredNarrationSegments = useDeferredValue(narrationSegments);
  const narrationSegmentCount = narrationSegments?.length ?? 0;
  const deferredNarrationSegmentCount = deferredNarrationSegments?.length ?? 0;
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
      ...(deferredNarrationSegments ?? []).map((segment) => ({
        ...segment,
        previewTrackKind: "narration" as const,
      })),
    ],
    [composition?.audioSegments, deferredNarrationSegments],
  );
  useEffect(() => {
    const start = performance.now();
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        const twoFrameMs = performance.now() - start;
        if (!isPlaying && twoFrameMs < 80) return;
        console.info(
          `[NarrationPerf][EditorCommit] live=${narrationSegmentCount} deferred=${deferredNarrationSegmentCount} playing=${isPlaying ? 1 : 0} two_frame_ms=${twoFrameMs.toFixed(1)}`,
        );
      });
    });
  }, [deferredNarrationSegmentCount, isPlaying, narrationSegmentCount]);
  const exportSubtitleSrtInFlightRef = React.useRef(false);
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

  const totalSelectedCount =
    selectedTextIds.length +
    selectedSubtitleIds.length +
    selectedPointerIds.length +
    selectedKeystrokeIds.length +
    selectedWebcamIds.length +
    selectedAudioSegmentIds.length +
    selectedNarrationSegmentIds.length;
  const [clearSignal, setClearSignal] = useState(0);
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
  const lastProjectResetKeyRef = React.useRef<string | null | undefined>(undefined);
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

  const handleImportSubtitleSrt = useCallback(async (file: File) => {
    if (!segment) return;
    try {
      const content = await file.text();
      const { segment: nextSegment, subtitles: importedSubtitles } = importSubtitleSrtIntoSegment(
        segment,
        content,
        duration,
      );
      if (importedSubtitles.length === 0) {
        console.error('[SubtitleSrt] import failed: no valid subtitles found');
        return;
      }
      setSegment(nextSegment);
      clearAllSelections();
      setEditingSubtitleId(importedSubtitles[0]?.id ?? null);
      setActivePanel('subtitles');
    } catch (error) {
      console.error('[SubtitleSrt] import failed:', error);
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

  const wallClockCurrentTime = useMemo(() => {
    const pts = segment?.speedPoints;
    if (!pts?.length) return currentTime;
    return videoTimeToWallClock(currentTime, pts);
  }, [currentTime, segment?.speedPoints]);

  return (
    <main
      className="app-main flex flex-col px-3 py-3 overflow-hidden"
      style={{ height: "calc(100vh - 44px)" }}
    >
      {error && (
        <p className="error-message text-[var(--tertiary-color)] mb-2 flex-shrink-0">
          {error}
        </p>
      )}

      <div className="content-layout flex gap-4 flex-1 min-h-0 pb-1">
        <div className="preview-and-controls flex-1 flex flex-col min-w-0 gap-3 relative">
          <PreviewCanvas
            previewContainerRef={previewContainerRef}
            previewCursorClass={previewCursorClass}
            onMouseDown={handlePreviewMouseDown}
            canvasRef={canvasRef}
            tempCanvasRef={tempCanvasRef}
            videoRef={videoRef}
            webcamVideoRef={webcamVideoRef}
            audioRef={audioRef}
            micAudioRef={micAudioRef}
            previousPreloadVideoRef={previousPreloadVideoRef}
            previousPreloadAudioRef={previousPreloadAudioRef}
            nextPreloadVideoRef={nextPreloadVideoRef}
            nextPreloadAudioRef={nextPreloadAudioRef}
            keystrokeOverlayEditFrame={keystrokeOverlayEditFrame}
            isKeystrokeOverlaySelected={isKeystrokeOverlaySelected}
            isDraggingKeystrokeOverlayRef={isDraggingKeystrokeOverlayRef}
            isResizingKeystrokeOverlayRef={isResizingKeystrokeOverlayRef}
            isBuffering={isBuffering}
            isPreviewPlaying={isPreviewPlaying}
            currentVideo={currentVideo}
            isTimelineOnly={isTimelineOnly}
            isLoadingVideo={isLoadingVideo}
            loadingProgress={loadingProgress}
            isRecording={isRecording}
            recordingDuration={recordingDuration}
            isCropping={isCropping}
            backgroundConfig={backgroundConfig}
            setBackgroundConfig={setBackgroundConfig}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
            onCanvasResizeDragStateChange={setIsCanvasResizeDragging}
            seekIndicatorDir={seekIndicatorDir}
            seekIndicatorKey={seekIndicatorKey}
            audioSegments={previewAudioSegments}
            audioTrackVolumePoints={audioTrackVolumePoints}
            narrationTrackVolumePoints={narrationTrackVolumePoints}
            speedPoints={segment?.speedPoints}
            currentTime={currentTime}
            isPlaying={isPlaying}
            audioResetKey={audioResetKey}
            liveNarrationProjectId={projectResetKey}
          />

          <PlaybackControlsRow
            showPlaybackControls={showPlaybackControls}
            showPlaybackControlsGhost={showPlaybackControlsGhost}
            isPlaying={isPlaying}
            isProcessing={isProcessing}
            isVideoReady={isVideoReady}
            isCropping={isCropping}
            hasAppliedCrop={hasAppliedCrop}
            currentTime={currentTime}
            duration={duration}
            wallClockCurrentTime={wallClockCurrentTime}
            wallClockDuration={wallClockDuration}
            onTogglePlayPause={handleTogglePlayPause}
            onToggleCrop={handleToggleCrop}
            onSetProjectDuration={onSetProjectDuration}
            backgroundConfig={backgroundConfig}
            setBackgroundConfig={setBackgroundConfig}
            customCanvasBaseDimensions={customCanvasBaseDimensions}
            getAutoCanvasSelectionConfig={getAutoCanvasSelectionConfig}
            handleActivateCustomCanvas={handleActivateCustomCanvas}
            handleApplyCanvasRatioPreset={handleApplyCanvasRatioPreset}
            isAutoCanvasDisabled={isAutoCanvasDisabled}
            segment={segment}
            setSegment={setSegment as (s: VideoSegment) => void}
            handleToggleKeystrokeMode={handleToggleKeystrokeMode}
            handleKeystrokeDelayChange={handleKeystrokeDelayChange}
            currentVideo={currentVideo}
            mousePositionsLength={mousePositionsLength}
            handleAutoZoom={handleAutoZoom}
            autoZoomConfig={autoZoomConfig}
            handleAutoZoomConfigChange={handleAutoZoomConfigChange}
            handleSmartPointerHiding={handleSmartPointerHiding}
            selectedSegmentCount={totalSelectedCount}
            canMergeSelection={mergeTarget !== null}
            onMergeSelection={handleMergeSelection}
            onClearSelection={clearAllSelections}
          />

        </div>

        {/* Side Panel */}
        <div className="side-panel-container w-[24rem] flex-shrink-0 min-h-0 relative overflow-visible">
          <SidePanel
            activePanel={activePanel}
            setActivePanel={setActivePanel}
            segment={segment}
            editingKeyframeId={editingKeyframeId}
            zoomFactor={zoomFactor}
            setZoomFactor={setZoomFactor}
            onDeleteKeyframe={handleDeleteKeyframe}
            onUpdateZoom={throttledUpdateZoom}
            backgroundConfig={backgroundConfig}
            setBackgroundConfig={setBackgroundConfig}
            webcamConfig={webcamConfig}
            setWebcamConfig={setWebcamConfig}
            webcamAvailable={Boolean(segment?.webcamAvailable)}
            recentUploads={recentUploads}
            onRemoveRecentUpload={handleRemoveRecentUpload}
            onBackgroundUpload={handleBackgroundUpload}
            isBackgroundUploadProcessing={isBackgroundUploadProcessing}
            editingTextId={editingTextId}
            editingSubtitleId={editingSubtitleId}
            selectedSubtitleIds={selectedSubtitleIds}
            selectedSubtitleRange={selectedSubtitleRange}
            subtitleSource={subtitleSource}
            onSubtitleSourceChange={onSubtitleSourceChange}
            subtitleMethod={subtitleMethod}
            onSubtitleMethodChange={onSubtitleMethodChange}
            subtitleMethodCapabilities={subtitleMethodCapabilities}
            canUseSelectedSubtitleMethod={canUseSelectedSubtitleMethod}
            selectedSubtitleMethodReason={selectedSubtitleMethodReason}
            subtitleLanguageHint={subtitleLanguageHint}
            onSubtitleLanguageHintChange={onSubtitleLanguageHintChange}
            subtitleGeminiPrompt={subtitleGeminiPrompt}
            onSubtitleGeminiPromptChange={onSubtitleGeminiPromptChange}
            subtitleGroqVocabulary={subtitleGroqVocabulary}
            onSubtitleGroqVocabularyChange={onSubtitleGroqVocabularyChange}
            isGeneratingSubtitles={isGeneratingSubtitles}
            subtitleStatusMessage={subtitleStatusMessage}
            canUseVideoSubtitleSource={segment?.deviceAudioAvailable !== false}
            canUseMicSubtitleSource={Boolean(segment?.micAudioAvailable)}
            canUseAudioSubtitleSource={(composition?.audioSegments?.length ?? 0) > 0}
            audioSegments={composition?.audioSegments}
            onGenerateSubtitles={() => handleGenerateSubtitles(selectedSubtitleRange)}
            onCancelSubtitleGeneration={handleCancelSubtitleGeneration}
            canExportSubtitleSrt={canExportSubtitleSrt}
            onExportSubtitleSrt={handleExportSubtitleSrt}
            canExportAudioSubtitleSrt={canExportAudioSubtitleSrt}
            onExportMusicSubtitleSrt={handleExportMusicSubtitleSrts}
            onApplyNarrationSegments={onApplyNarrationSegments}
            onFinalizeNarrationSegments={onFinalizeNarrationSegments}
            audioSegmentsForPanel={composition?.audioSegments}
            selectedAudioSegmentIds={selectedAudioSegmentIdSet}
            onUpdateAudioSegmentForPanel={onUpdateAudioSegment}
            onDeleteAudioSegmentsForPanel={onDeleteAudioSegments}
            onCommitAudioSegmentsForPanel={onCommitAudioSegments}
            narrationSegmentsForPanel={deferredNarrationSegments}
            selectedNarrationSegmentIds={selectedNarrationSegmentIdSet}
            onUpdateNarrationSegmentForPanel={onUpdateNarrationSegment}
            onDeleteNarrationSegmentsForPanel={onDeleteNarrationSegments}
            onCommitNarrationSegmentsForPanel={onCommitNarrationSegments}
            visibleSubtitlesForNarration={visibleSubtitleSegments}
            subtitleTracksForNarration={allSubtitleTracks}
            subtitleTranslation={subtitleTranslation}
            selectedTextIds={selectedTextIds}
            hasMouseData={mousePositionsLength > 0}
            onUpdateSegment={setSegment as (segment: VideoSegment) => void}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
          />
          {isOverlayMode && (
            <div className="panel-block-overlay absolute inset-0 bg-[var(--surface)] z-50 rounded-xl" />
          )}
        </div>
      </div>

      {/* Timeline */}
      <div
        className={`timeline-container mt-3 flex-shrink-0 relative ${isOverlayMode ? "overflow-hidden" : ""}`}
      >
        <TimelineArea
          duration={duration}
          currentTime={currentTime}
          segment={segment}
          thumbnails={thumbnails}
          timelineRef={timelineRef}
          videoRef={videoRef as React.RefObject<HTMLVideoElement>}
          editingKeyframeId={editingKeyframeId}
          editingTextId={editingTextId}
          editingSubtitleId={editingSubtitleId}
          editingKeystrokeSegmentId={editingKeystrokeSegmentId}
          setCurrentTime={setCurrentTime}
          setEditingKeyframeId={setEditingKeyframeId}
          setEditingTextId={setEditingTextId}
          setEditingSubtitleId={setEditingSubtitleId}
          setEditingKeystrokeSegmentId={setEditingKeystrokeSegmentId}
          setEditingPointerId={setEditingPointerId}
          setActivePanel={setActivePanel}
          setSegment={setSegment}
          onSeek={seek}
          onSeekEnd={flushSeek}
          onClearTimelineFocus={clearTimelineFocus}
          onAddText={handleAddText}
          onAddSubtitle={subtitleTranslation.canCreateManualSubtitles ? handleAddSubtitle : undefined}
          onPickSubtitleSrtFile={handleImportSubtitleSrt}
          onAddKeystrokeSegment={handleAddKeystrokeSegment}
          onAddPointerSegment={handleAddPointerSegment}
          isPlaying={isPlaying}
          onViewportCanvasWidthChange={setTimelineCanvasWidthPx}
          isDeviceAudioAvailable={segment?.deviceAudioAvailable !== false}
          isMicAudioAvailable={Boolean(segment?.micAudioAvailable)}
          isWebcamAvailable={Boolean(segment?.webcamAvailable)}
          currentRawVideoPath={currentRawVideoPath}
          currentRawMicAudioPath={currentRawMicAudioPath}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
          selectedTextIds={selectedTextIds}
          selectedSubtitleIds={selectedSubtitleIds}
          onTextSelectionChange={handleTextSelectionChange}
          onSubtitleSelectionChange={handleSubtitleSelectionChange}
          onSubtitleRangeChange={handleSubtitleRangeChange}
          onPointerSelectionChange={handlePointerSelectionChange}
          onKeystrokeSelectionChange={handleKeystrokeSelectionChange}
          onWebcamSelectionChange={handleWebcamSelectionChange}
          clearSelectionSignal={clearSignal}
          hasMouseData={mousePositionsLength > 0}
          subtitleGenerationIndicator={subtitleGenerationIndicator}
          subtitleTranslationChunkPreview={subtitleTranslation.subtitleTranslationChunkPreview}
          audioSegments={composition?.audioSegments}
          onUpdateAudioSegment={onUpdateAudioSegment}
          onPickImportedAudioFile={onPickImportedAudioFile}
          onAudioSegmentClick={handleAudioSegmentClick}
          audioTrackVolumePoints={audioTrackVolumePoints}
          onUpdateAudioTrackVolumePoints={onUpdateAudioTrackVolumePoints}
          onDeleteAudioSegments={(ids) => {
            onDeleteAudioSegments?.(ids);
            setSelectedAudioSegmentIds((current) =>
              current.filter((id) => !ids.includes(id)),
            );
          }}
          onCommitAudioSegments={onCommitAudioSegments}
          selectedAudioSegmentIds={selectedAudioSegmentIdSet}
          selectedAudioSegmentRange={selectedAudioSegmentRange}
          onAudioSelectionChange={handleAudioSelectionChange}
          onAudioRangeChange={handleAudioRangeChange}
          narrationSegments={deferredNarrationSegments}
          liveNarrationProjectId={projectResetKey}
          onNarrationSegmentClick={handleNarrationSegmentClick}
          onUpdateNarrationSegment={onUpdateNarrationSegment}
          narrationTrackVolumePoints={narrationTrackVolumePoints}
          onUpdateNarrationTrackVolumePoints={onUpdateNarrationTrackVolumePoints}
          onDeleteNarrationSegments={(ids) => {
            onDeleteNarrationSegments?.(ids);
            setSelectedNarrationSegmentIds((current) =>
              current.filter((id) => !ids.includes(id)),
            );
          }}
          onCommitNarrationSegments={onCommitNarrationSegments}
          selectedNarrationSegmentIds={selectedNarrationSegmentIdSet}
          selectedNarrationSegmentRange={selectedNarrationSegmentRange}
          onNarrationSelectionChange={handleNarrationSelectionChange}
          onNarrationRangeChange={handleNarrationRangeChange}
        />
        {isOverlayMode && (
          <div className="timeline-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />
        )}
      </div>
    </main>
  );
}
