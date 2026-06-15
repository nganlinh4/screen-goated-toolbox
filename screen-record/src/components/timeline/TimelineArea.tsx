import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSettings } from "@/hooks/useSettings";
import type { TimelineAreaProps } from "./TimelineAreaTypes";
import { buildTimelineRulerTicks } from "./timelineRuler";
import { Playhead } from "./Playhead";
import { TimelineLabelColumn } from "./TimelineLabelColumn";
import { TimelineTrackStack } from "./TimelineTrackStack";
import { useTimelineDrag } from "./useTimelineDrag";
import { useTimelineSegmentActions } from "./useTimelineSegmentActions";
import { useTimelineViewport } from "./useTimelineViewport";
import { countFrontendRender } from "@/lib/frontendPerfDiagnostics";

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
  countFrontendRender("TimelineArea");
  const { t } = useSettings();
  const [showDebug, setShowDebug] = useState(false);
  const [volumeViewEnabled, setVolumeViewEnabled] = useState(false);
  const showEmptyRuler = duration <= 0;
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

  // Track geometry depends only on which lanes are visible, never on
  // `currentTime`. Memoize the spread + reduce so the ~60fps playback ticks
  // (which re-render TimelineArea via the viewport follow loop) don't redo it.
  const { playheadHeadCenterY, playheadLineBottomY } = useMemo(() => {
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
    return {
      playheadHeadCenterY: showTrimLane
        ? trimHeadCenterY
        : Math.max(trackStackHeight / 2, 8),
      playheadLineBottomY: showTrimLane
        ? trimLaneBottomY
        : Math.max(trackStackHeight, 1),
    };
  }, [
    showZoom,
    showDebug,
    showSpeed,
    showImportedAudio,
    showDeviceAudio,
    showMicAudio,
    showWebcam,
    showNarration,
    showKeystroke,
    showPointer,
    showTrimLane,
  ]);
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

  const {
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
  } = useTimelineSegmentActions({
    duration,
    segment,
    setSegment,
    setCurrentTime,
    videoRef,
    onSeek,
    onClearTimelineFocus,
    beginBatch,
    commitBatch,
  });

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
        <TimelineLabelColumn
          t={t}
          segment={segment}
          duration={duration}
          showZoom={showZoom}
          showDebug={showDebug}
          setShowDebug={setShowDebug}
          showSpeed={showSpeed}
          showImportedAudio={showImportedAudio}
          showDeviceAudio={showDeviceAudio}
          showMicAudio={showMicAudio}
          showWebcam={showWebcam}
          showNarration={showNarration}
          showKeystroke={showKeystroke}
          showPointer={showPointer}
          showTrimLane={showTrimLane}
          keystrokeTrackLabel={keystrokeTrackLabel}
          audioSegments={audioSegments}
          narrationSegments={narrationSegments}
          onTriggerImportedAudioPicker={handleTriggerImportedAudioPicker}
          onTriggerSubtitlePicker={handleTriggerSubtitlePicker}
          canPickImportedAudioFile={Boolean(onPickImportedAudioFile)}
          canPickSubtitleFile={Boolean(onPickSubtitleFile || onPickSubtitleSrtFile)}
          onAudioTrackDownload={onAudioTrackDownload}
          currentRawMicAudioPath={currentRawMicAudioPath}
          isMicAudioAvailable={isMicAudioAvailable}
          isWebcamAvailable={isWebcamAvailable}
          volumeViewEnabled={volumeViewEnabled}
          setVolumeViewEnabled={setVolumeViewEnabled}
          setSegment={setSegment}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
        />

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
                <TimelineTrackStack
                  segment={segment}
                  setSegment={setSegment}
                  duration={duration}
                  thumbnails={thumbnails}
                  editingKeyframeId={editingKeyframeId}
                  editingTextId={editingTextId}
                  editingSubtitleId={editingSubtitleId}
                  editingKeystrokeSegmentId={editingKeystrokeSegmentId}
                  showZoom={showZoom}
                  showDebug={showDebug}
                  showSpeed={showSpeed}
                  showImportedAudio={showImportedAudio}
                  showDeviceAudio={showDeviceAudio}
                  showMicAudio={showMicAudio}
                  showWebcam={showWebcam}
                  showNarration={showNarration}
                  showKeystroke={showKeystroke}
                  showPointer={showPointer}
                  showTrimLane={showTrimLane}
                  volumeViewEnabled={volumeViewEnabled}
                  isPlaying={isPlaying}
                  isDeviceAudioAvailable={isDeviceAudioAvailable}
                  isMicAudioAvailable={isMicAudioAvailable}
                  isWebcamAvailable={isWebcamAvailable}
                  currentRawVideoPath={currentRawVideoPath}
                  currentRawMicAudioPath={currentRawMicAudioPath}
                  audioSegments={audioSegments}
                  narrationSegments={narrationSegments}
                  liveNarrationProjectId={liveNarrationProjectId}
                  selectedAudioSegmentIds={selectedAudioSegmentIds}
                  selectedAudioSegmentRange={selectedAudioSegmentRange}
                  selectedNarrationSegmentIds={selectedNarrationSegmentIds}
                  selectedNarrationSegmentRange={selectedNarrationSegmentRange}
                  clearSelectionSignal={clearSelectionSignal}
                  subtitleGenerationIndicator={subtitleGenerationIndicator}
                  subtitleTranslationChunkPreview={subtitleTranslationChunkPreview}
                  audioTrackVolumePoints={audioTrackVolumePoints}
                  narrationTrackVolumePoints={narrationTrackVolumePoints}
                  canvasWidthPx={canvasWidthPx}
                  visibleTimeRange={visibleTimeRange}
                  dragState={dragState}
                  beginBatch={beginBatch}
                  commitBatch={commitBatch}
                  onKeyframeClick={handleKeyframeClick}
                  onZoomDragStart={handleZoomDragStart}
                  onAudioSegmentClick={onAudioSegmentClick}
                  onUpdateAudioSegment={onUpdateAudioSegment}
                  onDeleteAudioSegments={onDeleteAudioSegments}
                  onCommitAudioSegments={onCommitAudioSegments}
                  onAudioSelectionChange={onAudioSelectionChange}
                  onAudioRangeChange={onAudioRangeChange}
                  onUpdateAudioTrackVolumePoints={onUpdateAudioTrackVolumePoints}
                  onWebcamClick={handleWebcamClick}
                  onWebcamDragStart={handleWebcamDragStart}
                  onDeleteWebcamSegments={handleDeleteWebcamSegments}
                  onWebcamSelectionChange={onWebcamSelectionChange}
                  onNarrationSegmentClick={onNarrationSegmentClick}
                  onUpdateNarrationSegment={onUpdateNarrationSegment}
                  onDeleteNarrationSegments={onDeleteNarrationSegments}
                  onCommitNarrationSegments={onCommitNarrationSegments}
                  onNarrationSelectionChange={onNarrationSelectionChange}
                  onNarrationRangeChange={onNarrationRangeChange}
                  onUpdateNarrationTrackVolumePoints={onUpdateNarrationTrackVolumePoints}
                  onSubtitleClick={handleSubtitleClick}
                  onSubtitleSplit={handleSubtitleSplit}
                  onSubtitleDuplicate={handleDuplicateSubtitle}
                  onSubtitleDragStart={handleSubtitleDragStart}
                  onAddSubtitle={onAddSubtitle}
                  onDeleteSubtitleSegments={handleDeleteSubtitleSegments}
                  onSubtitleSelectionChange={onSubtitleSelectionChange}
                  onSubtitleRangeChange={onSubtitleRangeChange}
                  onAssignSubtitleSourceGroup={handleAssignSubtitleSourceGroup}
                  onTextClick={handleTextClick}
                  onTextSplit={handleTextSplit}
                  onTextDuplicate={handleDuplicateText}
                  onTextDragStart={handleTextDragStart}
                  onAddText={onAddText}
                  onDeleteTextSegments={handleDeleteTextSegments}
                  onTextSelectionChange={onTextSelectionChange}
                  onKeystrokeClick={handleKeystrokeClick}
                  onKeystrokeDragStart={handleKeystrokeDragStart}
                  onAddKeystrokeSegment={onAddKeystrokeSegment}
                  onKeystrokeHover={setEditingKeystrokeSegmentId}
                  onDeleteKeystrokeSegments={handleDeleteKeystrokeSegments}
                  onKeystrokeSelectionChange={onKeystrokeSelectionChange}
                  onPointerClick={handlePointerClick}
                  onPointerDragStart={handlePointerDragStart}
                  onAddPointerSegment={onAddPointerSegment}
                  onPointerHover={setEditingPointerId}
                  onDeletePointerSegments={handleDeletePointerSegments}
                  onPointerSelectionChange={onPointerSelectionChange}
                  onTrimDragStart={handleTrimDragStart}
                  onTrimSplit={handleTrimSplit}
                  onTrimAddSegment={handleTrimAddSegment}
                  onEmptyTrackClick={handleEmptyTrackClick}
                />

                {/* Rendered as a sibling of the memoized track stack so the
                    ~60fps `currentTime` ticks update only the Playhead. Its
                    dual time source (own RAF while playing, `currentTime` prop
                    while paused/seeking) is preserved. */}
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
