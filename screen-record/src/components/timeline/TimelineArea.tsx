import React, { useCallback, useEffect, useRef, useState } from "react";
import type {
  ImportedAudioSegment,
  NarrationSegment,
  SubtitleSourceGroup,
  VideoSegment,
  AudioDownloadTrackKind,
} from "@/types/video";
import { AudioLines, Download, Plus } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import type { SubtitleGenerationIndicator } from "@/lib/subtitleGenerationPlan";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import { KeystrokeTrack } from "./KeystrokeTrack";
import { MicTrack } from "./MicTrack";
import { ImportedAudioTrack } from "./ImportedAudioTrack";
import { NarrationTrack } from "./NarrationTrack";
import { Playhead } from "./Playhead";
import { PointerTrack } from "./PointerTrack";
import { DeviceAudioTrack } from "./DeviceAudioTrack";
import { SpeedTrack } from "./SpeedTrack";
import { SubtitleTrack } from "./SubtitleTrack";
import { TextTrack } from "./TextTrack";
import { TrimTrack } from "./TrimTrack";
import { WebcamVisibilityTrack } from "./WebcamVisibilityTrack";
import { ZoomDebugOverlay } from "./ZoomDebugOverlay";
import { ZoomTrack } from "./ZoomTrack";
import { buildTimelineRulerTicks } from "./timelineRuler";
import { useTimelineDrag } from "./useTimelineDrag";
import { useTimelineViewport } from "./useTimelineViewport";
import { Slider } from "@/components/ui/Slider";
import { Switch } from "@/components/ui/Switch";
import type { ActivePanel } from "@/components/sidepanel";
import {
  clampVisibilitySegmentsToDuration,
  mergePointerSegments,
} from "@/lib/cursorHiding";
import { buildTextSplitPreview } from "@/lib/textSplitPreview";
import {
  deleteSubtitleIdsAcrossTracks,
  duplicateSubtitleAcrossTracks,
  splitSubtitleAcrossTracks,
  updateSubtitleSourceGroupAcrossTracks,
} from "@/lib/subtitleTrackMutations";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";

const TIMELINE_TRACK_GAP_PX = 2;
const SMALL_TRACK_HEIGHT = 28;
const TIMELINE_TRACK_HEIGHTS = {
  zoom: SMALL_TRACK_HEIGHT,
  debug: SMALL_TRACK_HEIGHT,
  speed: SMALL_TRACK_HEIGHT,
  importedAudio: SMALL_TRACK_HEIGHT,
  narration: SMALL_TRACK_HEIGHT,
  deviceAudio: SMALL_TRACK_HEIGHT,
  micAudio: SMALL_TRACK_HEIGHT,
  webcam: SMALL_TRACK_HEIGHT,
  subtitles: SMALL_TRACK_HEIGHT,
  text: SMALL_TRACK_HEIGHT,
  keystroke: SMALL_TRACK_HEIGHT,
  pointer: SMALL_TRACK_HEIGHT,
  trimLane: 40,
} as const;

interface TimelineAreaProps {
  duration: number;
  currentTime: number;
  segment: VideoSegment | null;
  thumbnails: string[];
  timelineRef: React.RefObject<HTMLDivElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  editingSubtitleId: string | null;
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  setActivePanel: (panel: ActivePanel) => void;
  setSegment: (segment: VideoSegment | null) => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  onClearTimelineFocus?: () => void;
  onAddText?: (atTime?: number) => void;
  onAddSubtitle?: (atTime?: number) => void;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  isPlaying?: boolean;
  onViewportZoomChange?: (zoom: number) => void;
  onViewportCanvasWidthChange?: (widthPx: number) => void;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  beginBatch: () => void;
  commitBatch: () => void;
  selectedTextIds: string[];
  selectedSubtitleIds: string[];
  onTextSelectionChange?: (ids: string[]) => void;
  onSubtitleSelectionChange?: (ids: string[]) => void;
  onSubtitleRangeChange?: (range: TrackSelectionRange | null) => void;
  onPointerSelectionChange?: (ids: string[]) => void;
  onKeystrokeSelectionChange?: (ids: string[]) => void;
  onWebcamSelectionChange?: (ids: string[]) => void;
  clearSelectionSignal?: number;
  hasMouseData?: boolean;
  subtitleGenerationIndicator?: SubtitleGenerationIndicator | null;
  subtitleTranslationChunkPreview?: {
    groups: Record<string, number>;
    groupCount: number;
  } | null;
  audioSegments?: ImportedAudioSegment[];
  onPickImportedAudioFile?: (file: File) => void;
  onPickSubtitleFile?: (file: File) => void;
  onPickSubtitleSrtFile?: (file: File) => void;
  onAudioSegmentClick?: (id: string) => void;
  onUpdateAudioSegment?: (id: string, patch: Partial<ImportedAudioSegment>) => void;
  onDeleteAudioSegments?: (ids: string[]) => void;
  onCommitAudioSegments?: () => void;
  selectedAudioSegmentIds?: ReadonlySet<string>;
  selectedAudioSegmentRange?: TrackSelectionRange | null;
  onAudioSelectionChange?: (ids: string[]) => void;
  onAudioRangeChange?: (range: TrackSelectionRange | null) => void;
  audioTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
  onUpdateAudioTrackVolumePoints?: (points: import("@/types/video").AudioGainPoint[]) => void;
  narrationSegments?: NarrationSegment[];
  liveNarrationProjectId?: string | null;
  onNarrationSegmentClick?: (id: string) => void;
  onUpdateNarrationSegment?: (id: string, patch: Partial<NarrationSegment>) => void;
  onDeleteNarrationSegments?: (ids: string[]) => void;
  onCommitNarrationSegments?: () => void;
  selectedNarrationSegmentIds?: ReadonlySet<string>;
  selectedNarrationSegmentRange?: TrackSelectionRange | null;
  onNarrationSelectionChange?: (ids: string[]) => void;
  onNarrationRangeChange?: (range: TrackSelectionRange | null) => void;
  narrationTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
  onUpdateNarrationTrackVolumePoints?: (points: import("@/types/video").AudioGainPoint[]) => void;
  onAudioTrackDownload?: (trackKind: AudioDownloadTrackKind, trackLabel: string) => void;
}

export const TimelineArea: React.FC<TimelineAreaProps> = ({
  duration,
  currentTime,
  segment,
  thumbnails,
  timelineRef,
  videoRef,
  editingKeyframeId,
  editingTextId,
  editingSubtitleId,
  editingKeystrokeSegmentId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingSubtitleId,
  setEditingKeystrokeSegmentId,
  setEditingPointerId,
  setActivePanel,
  setSegment,
  onSeek,
  onSeekEnd,
  onClearTimelineFocus,
  onAddText,
  onAddSubtitle,
  onAddKeystrokeSegment,
  onAddPointerSegment,
  isPlaying,
  onViewportZoomChange,
  onViewportCanvasWidthChange,
  isDeviceAudioAvailable,
  isMicAudioAvailable,
  isWebcamAvailable,
  currentRawVideoPath,
  currentRawMicAudioPath,
  beginBatch,
  commitBatch,
  selectedTextIds,
  selectedSubtitleIds,
  onTextSelectionChange,
  onSubtitleSelectionChange,
  onSubtitleRangeChange,
  onPointerSelectionChange,
  onKeystrokeSelectionChange,
  onWebcamSelectionChange,
  clearSelectionSignal,
  hasMouseData = true,
  subtitleGenerationIndicator,
  subtitleTranslationChunkPreview,
  audioSegments,
  onPickImportedAudioFile,
  onPickSubtitleFile,
  onPickSubtitleSrtFile,
  onAudioSegmentClick,
  onUpdateAudioSegment,
  onDeleteAudioSegments,
  onCommitAudioSegments,
  selectedAudioSegmentIds,
  selectedAudioSegmentRange,
  onAudioSelectionChange,
  onAudioRangeChange,
  audioTrackVolumePoints,
  onUpdateAudioTrackVolumePoints,
  narrationSegments,
  liveNarrationProjectId,
  onNarrationSegmentClick,
  onUpdateNarrationSegment,
  onDeleteNarrationSegments,
  onCommitNarrationSegments,
  selectedNarrationSegmentIds,
  selectedNarrationSegmentRange,
  onNarrationSelectionChange,
  onNarrationRangeChange,
  narrationTrackVolumePoints,
  onUpdateNarrationTrackVolumePoints,
  onAudioTrackDownload,
}) => {
  const { t } = useSettings();
  const [showDebug, setShowDebug] = useState(false);
  const [volumeViewEnabled, setVolumeViewEnabled] = useState(false);
  const showEmptyRuler = duration <= 0;
  const clampTrackDelay = (value: number) =>
    Math.max(-2, Math.min(2, value));
  const renderTrackDelayLabel = ({
    className,
    groupClassName,
    label,
    value,
    onChange,
    isAvailable,
    heightClassName,
    action,
  }: {
    className: string;
    groupClassName: string;
    label: string;
    value: number;
    onChange: (value: number) => void;
    isAvailable: boolean;
    heightClassName: string;
    action?: React.ReactNode;
  }) => (
    <div
      className={`${className} ${heightClassName} relative flex items-center ${
        isAvailable ? "" : "timeline-label-unavailable"
      } ${groupClassName}`}
    >
      <div className="timeline-label-hover-bridge absolute left-full inset-y-0 w-3" />
      <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
        {label}
      </span>
      {action}
      <div className="playback-keystroke-delay-popover absolute left-[calc(100%+8px)] top-1/2 z-30 -translate-y-1/2 w-[218px] px-2.5 py-2 rounded-lg border pointer-events-none opacity-0 translate-x-1 transition-all duration-150 group-hover:opacity-100 group-hover:translate-x-0 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:translate-x-0 group-focus-within:pointer-events-auto">
        <div className="flex items-center gap-3">
          <div className="flex-1 rounded-full px-1 py-[3px]">
            <Slider
              min={-2}
              max={2}
              step={0.01}
              value={value}
              disabled={!isAvailable || !segment}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(nextValue) => onChange(clampTrackDelay(nextValue))}
              className="playback-keystroke-delay-slider block w-full"
            />
          </div>
          <span className="text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-12 text-right">
            {value.toFixed(2)}s
          </span>
        </div>
      </div>
    </div>
  );
  const keystrokeTrackLabel =
    segment?.keystrokeMode === "keyboard"
      ? t.trackKeyboard
      : segment?.keystrokeMode === "keyboardMouse"
        ? t.trackKeyboardMouse
        : t.trackKeystrokesOff;
  const showZoom = true;
  const showSpeed = true;
  const showTrimLane = true;
  const showDeviceAudio = isDeviceAudioAvailable;
  const showMicAudio = isMicAudioAvailable;
  const showWebcam = isWebcamAvailable;
  const showKeystroke = (segment?.keystrokeMode ?? 'off') !== 'off';
  const showPointer = hasMouseData;
  const showImportedAudio = Boolean(onPickImportedAudioFile) || (audioSegments?.length ?? 0) > 0;
  const showNarration = (narrationSegments?.length ?? 0) > 0;

  const renderDownloadButton = (
    trackKind: AudioDownloadTrackKind,
    label: string,
    groupName: string,
    disabled = false,
    offsetIndex = 0,
  ) => {
    const hoverClass =
      groupName === "imported-audio-label"
        ? "group-hover/imported-audio-label:opacity-100"
        : groupName === "device-audio-label"
          ? "group-hover/device-audio-label:opacity-100"
          : groupName === "mic-audio-label"
            ? "group-hover/mic-audio-label:opacity-100"
            : "group-hover/narration-label:opacity-100";
    return (
      <button
        type="button"
        onClick={() => onAudioTrackDownload?.(trackKind, label)}
        disabled={disabled || !onAudioTrackDownload}
        className={`timeline-label-audio-download ui-icon-button absolute left-full top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 text-[var(--primary-color)] opacity-0 shadow-sm transition-opacity duration-150 ${hoverClass} focus-visible:opacity-100 disabled:opacity-30`}
        style={{ marginLeft: `${4 + offsetIndex * 24}px` }}
        title={t.downloadAudioTrack}
        aria-label={`${t.downloadAudioTrack}: ${label}`}
      >
        <Download className="h-3 w-3" strokeWidth={2.6} />
      </button>
    );
  };

  const importedAudioFileInputRef = useRef<HTMLInputElement>(null);
  const subtitleFileInputRef = useRef<HTMLInputElement>(null);
  const handleTriggerImportedAudioPicker = useCallback(() => {
    importedAudioFileInputRef.current?.click();
  }, []);
  const handleTriggerSubtitlePicker = useCallback(() => {
    subtitleFileInputRef.current?.click();
  }, []);
  const handleImportedAudioFilePicked = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      if (file && onPickImportedAudioFile) onPickImportedAudioFile(file);
    },
    [onPickImportedAudioFile],
  );
  const handleSubtitleFilePicked = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      const pickSubtitle = onPickSubtitleFile ?? onPickSubtitleSrtFile;
      if (file && pickSubtitle) pickSubtitle(file);
    },
    [onPickSubtitleFile, onPickSubtitleSrtFile],
  );

  const trackHeightsBeforeTrim = [
    ...(showZoom ? [TIMELINE_TRACK_HEIGHTS.zoom] : []),
    ...(showZoom && showDebug ? [TIMELINE_TRACK_HEIGHTS.debug] : []),
    ...(showSpeed ? [TIMELINE_TRACK_HEIGHTS.speed] : []),
    ...(showImportedAudio ? [TIMELINE_TRACK_HEIGHTS.importedAudio] : []),
    ...(showDeviceAudio ? [TIMELINE_TRACK_HEIGHTS.deviceAudio] : []),
    ...(showMicAudio ? [TIMELINE_TRACK_HEIGHTS.micAudio] : []),
    ...(showWebcam ? [TIMELINE_TRACK_HEIGHTS.webcam] : []),
    ...(showNarration ? [TIMELINE_TRACK_HEIGHTS.narration] : []),
    TIMELINE_TRACK_HEIGHTS.subtitles,
    TIMELINE_TRACK_HEIGHTS.text,
    ...(showKeystroke ? [TIMELINE_TRACK_HEIGHTS.keystroke] : []),
    ...(showPointer ? [TIMELINE_TRACK_HEIGHTS.pointer] : []),
  ];
  const trackStackHeight =
    trackHeightsBeforeTrim.reduce((sum, height) => sum + height, 0) +
    Math.max(trackHeightsBeforeTrim.length - 1, 0) * TIMELINE_TRACK_GAP_PX;
  const trimHeadCenterY =
    trackStackHeight +
    (trackHeightsBeforeTrim.length > 0 ? TIMELINE_TRACK_GAP_PX : 0) +
    TIMELINE_TRACK_HEIGHTS.trimLane / 2;
  const trimLaneBottomY = trimHeadCenterY + TIMELINE_TRACK_HEIGHTS.trimLane / 2;
  const playheadHeadCenterY = showTrimLane
    ? trimHeadCenterY
    : Math.max(trackStackHeight / 2, 8);
  const playheadLineBottomY = showTrimLane
    ? trimLaneBottomY
    : Math.max(trackStackHeight, 1);
  const {
    dragState,
    handleTrimDragStart,
    handleTrimSplit,
    handleTrimAddSegment,
    handleZoomDragStart,
    handleTextDragStart,
    handleTextClick,
    handleSubtitleDragStart,
    handleSubtitleClick,
    handleKeystrokeDragStart,
    handleKeystrokeClick,
    handlePointerDragStart,
    handlePointerClick,
    handleWebcamDragStart,
    handleWebcamClick,
    handleKeyframeClick,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
  } = useTimelineDrag({
    duration,
    segment,
    timelineRef,
    videoRef,
    setCurrentTime,
    setSegment,
    setEditingKeyframeId,
    setEditingTextId,
    setEditingSubtitleId,
    setEditingKeystrokeId: setEditingKeystrokeSegmentId,
    setEditingPointerId,
    setActivePanel,
    selectedTextIds,
    selectedSubtitleIds,
    onSeek,
    onSeekEnd,
    onClearTimelineFocus,
    beginBatch,
    commitBatch,
  });

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

  const isTimelineInteracting =
    dragState.isDraggingTrimStart ||
    dragState.isDraggingTrimEnd ||
    dragState.isDraggingTextStart ||
    dragState.isDraggingTextEnd ||
    dragState.isDraggingTextBody ||
    dragState.isDraggingSubtitleStart ||
    dragState.isDraggingSubtitleEnd ||
    dragState.isDraggingSubtitleBody ||
    dragState.isDraggingKeystrokeStart ||
    dragState.isDraggingKeystrokeEnd ||
    dragState.isDraggingKeystrokeBody ||
    dragState.isDraggingPointerStart ||
    dragState.isDraggingPointerEnd ||
    dragState.isDraggingPointerBody ||
    dragState.isDraggingWebcamStart ||
    dragState.isDraggingWebcamEnd ||
    dragState.isDraggingWebcamBody ||
    dragState.isDraggingZoom ||
    dragState.isDraggingSeek;

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

  const {
    viewportRef,
    scrollbarTrackRef,
    scrollbarThumbRef,
    zoom,
    showScrollbar,
    canvasWidth,
    canvasWidthPx,
    visibleTimeRange,
    handleScrollbarTrackPointerDown,
    handleScrollbarThumbPointerDown,
  } = useTimelineViewport({
    duration,
    currentTime,
    segment,
    timelineRef,
    videoRef,
    isPlaying: !!isPlaying,
    isInteracting: isTimelineInteracting,
    disableVideoSync: segment?.mediaMode === "timelineOnly",
  });
  const rulerTicks = buildTimelineRulerTicks({
    duration,
    widthPx: canvasWidthPx,
    speedPoints: segment?.speedPoints,
  });

  useEffect(() => {
    onViewportZoomChange?.(zoom);
  }, [onViewportZoomChange, zoom]);

  useEffect(() => {
    onViewportCanvasWidthChange?.(canvasWidthPx);
  }, [canvasWidthPx, onViewportCanvasWidthChange]);

  return (
    <div className="timeline-area select-none mx-2">
      <input
        ref={importedAudioFileInputRef}
        type="file"
        accept="audio/*"
        className="hidden"
        onChange={handleImportedAudioFilePicked}
      />
      <input
        ref={subtitleFileInputRef}
        type="file"
        accept=".srt,.vtt,text/plain,text/vtt,application/x-subrip"
        className="timeline-subtitle-file-input hidden"
        onChange={handleSubtitleFilePicked}
      />
      <div className="timeline-shell flex gap-4">
        <div className="timeline-side-column w-[4rem] flex-shrink-0">
          <div className="timeline-label-gutter flex flex-col gap-[2px] border-r border-[var(--ui-border)] pr-2">
            {showZoom && (
              <div className="timeline-label-zoom h-7 flex items-center justify-between">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackZoom}
                </span>
                <button
                  onClick={() => setShowDebug((value) => !value)}
                  className={`timeline-debug-btn w-3 h-3 rounded-sm text-[7px] font-bold leading-none flex items-center justify-center transition-colors ${
                    showDebug
                      ? "bg-blue-500 text-white"
                      : "ui-surface text-[var(--outline)] hover:text-[var(--on-surface)]"
                  }`}
                  title="Debug zoom curve"
                >
                  D
                </button>
              </div>
            )}
            {showZoom && showDebug && (
              <div className="timeline-label-debug h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none opacity-50">
                  dbg
                </span>
              </div>
            )}
            {showSpeed && (
              <div className="timeline-label-speed h-7 flex items-center justify-between">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackSpeed || "Speed"}
                </span>
                <button
                  onClick={() => {
                    if (!segment) return;
                    beginBatch();
                    setSegment({
                      ...segment,
                      speedPoints: [
                        { time: 0, speed: 1 },
                        { time: duration, speed: 1 },
                      ],
                    });
                    commitBatch();
                  }}
                  disabled={!segment}
                  className="timeline-speed-reset-btn ui-icon-button p-1 text-[9px] font-mono leading-none disabled:opacity-40 disabled:hover:text-[var(--outline)] disabled:hover:bg-transparent"
                  title={t.resetSpeed || "Reset"}
                >
                  R
                </button>
              </div>
            )}
            {showImportedAudio && (
              <div className="timeline-label-imported-audio group/imported-audio-label relative h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackAudio}
                </span>
                {renderDownloadButton("imported", t.trackAudio, "imported-audio-label", (audioSegments?.length ?? 0) === 0, onPickImportedAudioFile ? 1 : 0)}
                {onPickImportedAudioFile && (
                  <button
                    type="button"
                    onClick={handleTriggerImportedAudioPicker}
                    className="timeline-label-imported-audio-add ui-icon-button absolute left-full ml-1 top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 text-[var(--primary-color)] opacity-0 shadow-sm transition-opacity duration-150 group-hover/imported-audio-label:opacity-100 focus-visible:opacity-100"
                    title={t.addAudioFile}
                    aria-label={t.addAudioFile}
                  >
                    <Plus className="h-3 w-3" strokeWidth={3} />
                  </button>
                )}
              </div>
            )}
            {showDeviceAudio && (
              <div className="timeline-label-device-audio group/device-audio-label relative h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackDeviceAudio}
                </span>
                {renderDownloadButton("device", t.trackDeviceAudio, "device-audio-label", !segment || segment.deviceAudioAvailable === false)}
              </div>
            )}
            {showMicAudio && renderTrackDelayLabel({
              className: "timeline-label-mic-audio",
              groupClassName: "group group/mic-audio-label",
              label: t.trackMicAudio,
              value: segment?.micAudioOffsetSec ?? 0,
              onChange: (value) => {
                if (!segment || !isMicAudioAvailable) return;
                setSegment({ ...segment, micAudioOffsetSec: value });
              },
              isAvailable: true,
              heightClassName: "h-7",
              action: renderDownloadButton("mic", t.trackMicAudio, "mic-audio-label", !segment || !currentRawMicAudioPath.trim()),
            })}
            {showWebcam && renderTrackDelayLabel({
              className: "timeline-label-webcam",
              groupClassName: "group",
              label: t.trackWebcam,
              value: segment?.webcamOffsetSec ?? 0,
              onChange: (value) => {
                if (!segment || !isWebcamAvailable) return;
                setSegment({ ...segment, webcamOffsetSec: value });
              },
              isAvailable: true,
              heightClassName: "h-7",
            })}
            {showNarration && (
              <div className="timeline-label-narration group/narration-label relative h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.tabNarration || "Narration"}
                </span>
                {renderDownloadButton("narration", t.tabNarration || "Narration", "narration-label", (narrationSegments?.length ?? 0) === 0)}
              </div>
            )}
            <div className="timeline-label-subtitles group/subtitle-label relative h-7 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackSubtitles}
              </span>
              {(onPickSubtitleFile || onPickSubtitleSrtFile) && (
                <button
                  type="button"
                  onClick={handleTriggerSubtitlePicker}
                  className="timeline-label-subtitles-add ui-icon-button absolute left-full ml-1 top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 text-[var(--primary-color)] opacity-0 shadow-sm transition-opacity duration-150 group-hover/subtitle-label:opacity-100 focus-visible:opacity-100"
                  title={t.importSubtitleSrt}
                  aria-label={t.importSubtitleSrt}
                >
                  <Plus className="h-3 w-3" strokeWidth={3} />
                </button>
              )}
            </div>
            <div className="timeline-label-text h-7 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackText}
              </span>
            </div>
            {showKeystroke && (
              <div className="timeline-label-keystrokes h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {keystrokeTrackLabel}
                </span>
              </div>
            )}
            {showPointer && (
              <div className="timeline-label-pointer h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackPointer}
                </span>
              </div>
            )}
            {showTrimLane && (
              <div className="timeline-label-video h-10 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackVideo}
                </span>
              </div>
            )}
          </div>
          <div className="timeline-ruler-spacer h-4 mt-0.5 flex items-center justify-end">
            <div
              className="timeline-volume-view-toggle flex items-center gap-1"
              title={t.volumeView}
              aria-label={t.volumeView}
            >
              <AudioLines
                className="timeline-volume-view-icon h-3 w-3 text-[var(--on-surface-variant)]"
                strokeWidth={2}
                aria-hidden="true"
              />
              <Switch
                checked={volumeViewEnabled}
                onCheckedChange={setVolumeViewEnabled}
              />
            </div>
          </div>
        </div>

        <div className="timeline-main-column flex-1 min-w-0">
          <div
            ref={viewportRef}
            className="timeline-scroll-viewport"
            data-zoomed={zoom > 1 ? "true" : "false"}
          >
            <div className="timeline-canvas" style={{ width: canvasWidth }}>
              <div
                ref={timelineRef}
                className={`timeline-content relative touch-none w-full ${
                  dragState.isDraggingSeek ? "cursor-grabbing" : "cursor-grab"
                }`}
                onPointerDown={handleMouseDown}
                onPointerMove={handleMouseMove}
                onPointerUp={handleMouseUp}
                onPointerCancel={handleMouseUp}
              >
                <div className="timeline-tracks flex flex-col gap-[2px]">
                  {showZoom && (segment ? (
                    <ZoomTrack
                      segment={segment}
                      duration={duration}
                      editingKeyframeId={editingKeyframeId}
                      onKeyframeClick={handleKeyframeClick}
                      onKeyframeDragStart={handleZoomDragStart}
                      onUpdateInfluencePoints={(points) => {
                        const nextSegment = {
                          ...segment,
                          zoomInfluencePoints: points,
                        };
                        if (points.length === 0) nextSegment.smoothMotionPath = [];
                        setSegment(nextSegment);
                      }}
                      onUpdateKeyframes={(keyframes) => {
                        setSegment({ ...segment, zoomKeyframes: keyframes });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="zoom-track-empty timeline-track-empty h-7" />
                  ))}

                  {showZoom && showDebug && segment && (
                    <ZoomDebugOverlay segment={segment} duration={duration} />
                  )}

                  {showSpeed && (segment ? (
                    <SpeedTrack
                      segment={segment}
                      duration={duration}
                      onUpdateSpeedPoints={(points) => {
                        setSegment({ ...segment, speedPoints: points });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="speed-track-empty timeline-track-empty h-7" />
                  ))}

                  {showImportedAudio && (
                    <ImportedAudioTrack
                      segments={audioSegments ?? []}
                      duration={duration}
                      onSegmentClick={onAudioSegmentClick}
                      onUpdateSegment={onUpdateAudioSegment}
                      onDeleteSegments={onDeleteAudioSegments}
                      selectedIds={selectedAudioSegmentIds}
                      selectedRange={selectedAudioSegmentRange}
                      onSelectionChange={onAudioSelectionChange}
                      onRangeChange={onAudioRangeChange}
                      viewMode={volumeViewEnabled ? "volume" : "compact"}
                      clearSignal={clearSelectionSignal}
                      volumePoints={audioTrackVolumePoints}
                      onUpdateVolumePoints={onUpdateAudioTrackVolumePoints}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                      onCommitSegments={onCommitAudioSegments}
                      onEmptyClick={handleEmptyTrackClick}
                      canvasWidthPx={canvasWidthPx}
                      visibleTimeRange={visibleTimeRange}
                    />
                  )}

                  {showDeviceAudio && (segment ? (
                    <DeviceAudioTrack
                      segment={segment}
                      duration={duration}
                      isAvailable={isDeviceAudioAvailable}
                      sourcePath={currentRawVideoPath}
                      viewMode={volumeViewEnabled ? "volume" : "compact"}
                      onUpdateDeviceAudioPoints={(points) => {
                        setSegment({ ...segment, deviceAudioPoints: points });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="device-audio-track-empty timeline-track-empty h-7" />
                  ))}

                  {showMicAudio && (segment ? (
                    <MicTrack
                      segment={segment}
                      duration={duration}
                      isAvailable={isMicAudioAvailable}
                      sourcePath={currentRawMicAudioPath}
                      viewMode={volumeViewEnabled ? "volume" : "compact"}
                      onUpdateMicAudioPoints={(points) => {
                        setSegment({ ...segment, micAudioPoints: points });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="mic-audio-track-empty timeline-track-empty h-7" />
                  ))}

                  {showWebcam && (segment ? (
                    <WebcamVisibilityTrack
                      segment={segment}
                      duration={duration}
                      isAvailable={isWebcamAvailable}
                      onWebcamClick={handleWebcamClick}
                      onHandleDragStart={handleWebcamDragStart}
                      onAddWebcamSegment={(atTime) => {
                        if (!segment || !isWebcamAvailable || typeof atTime !== "number") return;
                        const segDur = Math.min(2, Math.max(0.3, duration * 0.08));
                        let startTime = Math.max(0, atTime - segDur / 2);
                        let endTime = Math.min(duration, startTime + segDur);
                        if (endTime - startTime < 0.1) {
                          startTime = Math.max(0, endTime - 0.1);
                        }
                        beginBatch();
                        setSegment({
                          ...segment,
                          webcamVisibilitySegments: clampVisibilitySegmentsToDuration(
                            mergePointerSegments([
                              ...(segment.webcamVisibilitySegments ?? []),
                              {
                                id: crypto.randomUUID(),
                                startTime,
                                endTime,
                              },
                            ]),
                            duration,
                          ),
                        });
                        commitBatch();
                      }}
                      onDeleteWebcamSegments={handleDeleteWebcamSegments}
                      onSelectionChange={onWebcamSelectionChange}
                      clearSignal={clearSelectionSignal}
                      onEmptyClick={handleEmptyTrackClick}
                    />
                  ) : (
                    <div className="webcam-visibility-track-empty timeline-track-empty h-7" />
                  ))}

                  {showNarration && (
                    <NarrationTrack
                      segments={narrationSegments ?? []}
                      liveProjectId={liveNarrationProjectId}
                      duration={duration}
                      onSegmentClick={onNarrationSegmentClick}
                      onUpdateSegment={onUpdateNarrationSegment}
                      onDeleteSegments={onDeleteNarrationSegments}
                      selectedIds={selectedNarrationSegmentIds}
                      selectedRange={selectedNarrationSegmentRange}
                      onSelectionChange={onNarrationSelectionChange}
                      onRangeChange={onNarrationRangeChange}
                      viewMode={volumeViewEnabled ? "volume" : "compact"}
                      clearSignal={clearSelectionSignal}
                      volumePoints={narrationTrackVolumePoints}
                      onUpdateVolumePoints={onUpdateNarrationTrackVolumePoints}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                      onCommitSegments={onCommitNarrationSegments}
                      onEmptyClick={handleEmptyTrackClick}
                      canvasWidthPx={canvasWidthPx}
                      visibleTimeRange={visibleTimeRange}
                    />
                  )}

                  {segment ? (
                    <SubtitleTrack
                      segment={segment}
                      duration={duration}
                      editingSubtitleId={editingSubtitleId}
                      onSubtitleClick={handleSubtitleClick}
                      onSubtitleSplit={handleSubtitleSplit}
                      onSubtitleDuplicate={handleDuplicateSubtitle}
                      onHandleDragStart={handleSubtitleDragStart}
                      onAddSubtitle={onAddSubtitle}
                      onDeleteSubtitleSegments={handleDeleteSubtitleSegments}
                      onSelectionChange={onSubtitleSelectionChange}
                      onRangeChange={onSubtitleRangeChange}
                      clearSignal={clearSelectionSignal}
                      generationIndicator={subtitleGenerationIndicator}
                      translationChunkPreview={subtitleTranslationChunkPreview}
                      audioSegments={audioSegments}
                      isDeviceAudioAvailable={isDeviceAudioAvailable}
                      isMicAudioAvailable={isMicAudioAvailable}
                      onAssignSubtitleSourceGroup={handleAssignSubtitleSourceGroup}
                      onEmptyClick={handleEmptyTrackClick}
                      canvasWidthPx={canvasWidthPx}
                      visibleTimeRange={visibleTimeRange}
                    />
                  ) : (
                    <div className="subtitle-track-empty timeline-track-empty h-7" />
                  )}

                  {segment ? (
                    <TextTrack
                      segment={segment}
                      duration={duration}
                      editingTextId={editingTextId}
                      onTextClick={handleTextClick}
                      onTextSplit={handleTextSplit}
                      onTextDuplicate={handleDuplicateText}
                      onHandleDragStart={handleTextDragStart}
                      onAddText={onAddText}
                      onDeleteTextSegments={handleDeleteTextSegments}
                      onSelectionChange={onTextSelectionChange}
                      clearSignal={clearSelectionSignal}
                      onEmptyClick={handleEmptyTrackClick}
                    />
                  ) : (
                    <div className="text-track-empty timeline-track-empty h-7" />
                  )}

                  {showKeystroke && (segment ? (
                    <KeystrokeTrack
                      segment={segment}
                      duration={duration}
                      editingKeystrokeSegmentId={editingKeystrokeSegmentId}
                      onKeystrokeClick={handleKeystrokeClick}
                      onHandleDragStart={handleKeystrokeDragStart}
                      onAddKeystrokeSegment={onAddKeystrokeSegment}
                      onKeystrokeHover={setEditingKeystrokeSegmentId}
                      onDeleteKeystrokeSegments={handleDeleteKeystrokeSegments}
                      onSelectionChange={onKeystrokeSelectionChange}
                      clearSignal={clearSelectionSignal}
                      onEmptyClick={handleEmptyTrackClick}
                    />
                  ) : (
                    <div className="keystroke-track-empty timeline-track-empty h-7" />
                  ))}

                  {showPointer && (segment ? (
                    <PointerTrack
                      segment={segment}
                      duration={duration}
                      onPointerClick={handlePointerClick}
                      onHandleDragStart={handlePointerDragStart}
                      onAddPointerSegment={onAddPointerSegment}
                      onPointerHover={setEditingPointerId}
                      onDeletePointerSegments={handleDeletePointerSegments}
                      onSelectionChange={onPointerSelectionChange}
                      clearSignal={clearSelectionSignal}
                      onEmptyClick={handleEmptyTrackClick}
                    />
                  ) : (
                    <div className="pointer-track-empty timeline-track-empty h-7" />
                  ))}

                  {showTrimLane && (segment ? (
                    <TrimTrack
                      segment={segment}
                      duration={duration}
                      thumbnails={thumbnails}
                      onTrimDragStart={handleTrimDragStart}
                      onTrimSplit={handleTrimSplit}
                      onTrimAddSegment={handleTrimAddSegment}
                      isDraggingTrim={
                        dragState.isDraggingTrimStart || dragState.isDraggingTrimEnd
                      }
                      isSeeking={dragState.isDraggingSeek}
                    />
                  ) : (
                    <div className="trim-track-empty-shell relative h-14">
                      <div className="trim-track-empty timeline-track-empty h-10" />
                    </div>
                  ))}
                </div>

                {segment && (
                  <Playhead
                    currentTime={currentTime}
                    duration={duration}
                    isPlaying={!!isPlaying}
                    videoRef={videoRef}
                    segment={segment}
                    disableVideoSync={segment.mediaMode === "timelineOnly"}
                    headCenterY={playheadHeadCenterY}
                    lineBottomY={playheadLineBottomY}
                  />
                )}
              </div>

              <div className="timeline-ruler relative h-4 mt-0.5 select-none">
                {!showEmptyRuler &&
                  rulerTicks.map((tick, index) => (
                    <div
                      key={`${tick.time}-${index}`}
                      className="timeline-tick absolute flex flex-col items-center"
                      style={{
                        left: `${tick.leftPct}%`,
                        transform: "translateX(-50%)",
                        top: 0,
                        contentVisibility: "auto",
                        containIntrinsicSize: "auto 16px",
                      } as React.CSSProperties}
                    >
                      <div className="timeline-tick-mark h-1.5 w-px bg-[var(--outline)]/40" />
                      <span className="timeline-tick-label mt-0.5 text-[9px] font-mono text-[var(--outline)] leading-none">
                        {tick.label}
                      </span>
                    </div>
                  ))}
                {showEmptyRuler && (
                  <div
                    className="timeline-ruler-empty absolute inset-0 flex items-start justify-between px-[2%] opacity-65"
                    aria-hidden="true"
                  >
                    {Array.from({ length: 6 }).map((_, index) => (
                      <div
                        key={index}
                        className="timeline-ruler-empty-tick flex flex-col items-center"
                      >
                        <div className="h-1.5 w-px bg-[var(--outline)]/18" />
                        <div className="mt-0.5 h-2.5 w-7 rounded-full bg-[var(--ui-surface-2)]" />
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
          <div
            className="timeline-scrollbar-shell mt-1"
            data-visible={showScrollbar ? "true" : "false"}
          >
            <div
              ref={scrollbarTrackRef}
              className="timeline-scrollbar-track"
              onPointerDown={handleScrollbarTrackPointerDown}
            >
              <div
                ref={scrollbarThumbRef}
                className="timeline-scrollbar-thumb"
                onPointerDown={handleScrollbarThumbPointerDown}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
