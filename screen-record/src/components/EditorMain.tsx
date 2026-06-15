import React, { useMemo } from "react";
import type { VideoSegment } from "@/types/video";
import { videoTimeToWallClock } from "@/lib/exportEstimator";
import { PreviewCanvas } from "@/components/PreviewCanvas";
import { PlaybackControlsRow } from "@/components/PlaybackControlsRow";
import { SidePanel } from "@/components/sidepanel/index";
import { TimelineArea } from "@/components/timeline";
import { useSettings } from "@/hooks/useSettings";
import type { EditorMainProps } from "./EditorMainTypes";
import { useEditorMainTimelineState } from "./useEditorMainTimelineState";

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
  setSegmentSilently,
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
  autoSplitSubtitles,
  onAutoSplitSubtitlesChange,
  autoSplitSubtitleMaxUnits,
  onAutoSplitSubtitleMaxUnitsChange,
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
  onAudioTrackDownload,
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

  const {
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
  } = useEditorMainTimelineState({
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
  });

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
        <p className="error-message text-[var(--tertiary-color)] mb-2 shrink-0">
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
        <div className="side-panel-container w-[24rem] shrink-0 min-h-0 relative overflow-visible">
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
            composition={composition}
            activeClipId={null}
            currentRawVideoPath={currentRawVideoPath}
            currentRawMicAudioPath={currentRawMicAudioPath}
            duration={duration}
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
            autoSplitSubtitles={autoSplitSubtitles}
            onAutoSplitSubtitlesChange={onAutoSplitSubtitlesChange}
            autoSplitSubtitleMaxUnits={autoSplitSubtitleMaxUnits}
            onAutoSplitSubtitleMaxUnitsChange={onAutoSplitSubtitleMaxUnitsChange}
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
            narrationSegmentsForPanel={narrationSegments}
            selectedNarrationSegmentIds={selectedNarrationSegmentIdSet}
            onUpdateNarrationSegmentForPanel={onUpdateNarrationSegment}
            onDeleteNarrationSegmentsForPanel={onDeleteNarrationSegments}
            onCommitNarrationSegmentsForPanel={onCommitNarrationSegments}
            onAlignSubtitlesToNarration={handleAlignSubtitlesToNarration}
            visibleSubtitlesForNarration={visibleSubtitleSegments}
            subtitleTracksForNarration={allSubtitleTracks}
            onNarrationGroupPreviewChange={setNarrationGroupPreview}
            subtitleTranslation={subtitleTranslation}
            selectedTextIds={selectedTextIds}
            hasMouseData={mousePositionsLength > 0}
            onUpdateSegment={setSegment as (segment: VideoSegment) => void}
            onUpdateSegmentSilently={
              setSegmentSilently as ((segment: VideoSegment) => void) | undefined
            }
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
        className={`timeline-container mt-3 shrink-0 relative ${isOverlayMode ? "overflow-hidden" : ""}`}
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
          onPickSubtitleFile={handleImportSubtitleFile}
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
          subtitleTranslationChunkPreview={narrationGroupPreview ?? subtitleTranslation.subtitleTranslationChunkPreview}
          audioSegments={composition?.audioSegments}
          onUpdateAudioSegment={onUpdateAudioSegment}
          onPickImportedAudioFile={onPickImportedAudioFile}
          onAudioSegmentClick={handleAudioSegmentClick}
          audioTrackVolumePoints={audioTrackVolumePoints}
          onUpdateAudioTrackVolumePoints={onUpdateAudioTrackVolumePoints}
          onDeleteAudioSegments={handleDeleteAudioSegmentsForTimeline}
          onCommitAudioSegments={onCommitAudioSegments}
          selectedAudioSegmentIds={selectedAudioSegmentIdSet}
          selectedAudioSegmentRange={selectedAudioSegmentRange}
          onAudioSelectionChange={handleAudioSelectionChange}
          onAudioRangeChange={handleAudioRangeChange}
          narrationSegments={narrationSegments}
          liveNarrationProjectId={projectResetKey}
          onNarrationSegmentClick={handleNarrationSegmentClick}
          onUpdateNarrationSegment={onUpdateNarrationSegment}
          narrationTrackVolumePoints={narrationTrackVolumePoints}
          onUpdateNarrationTrackVolumePoints={onUpdateNarrationTrackVolumePoints}
          onDeleteNarrationSegments={handleDeleteNarrationSegmentsForTimeline}
          onCommitNarrationSegments={onCommitNarrationSegments}
          selectedNarrationSegmentIds={selectedNarrationSegmentIdSet}
          selectedNarrationSegmentRange={selectedNarrationSegmentRange}
          onNarrationSelectionChange={handleNarrationSelectionChange}
          onNarrationRangeChange={handleNarrationRangeChange}
          onAudioTrackDownload={onAudioTrackDownload}
        />
        {isOverlayMode && (
          <div className="timeline-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />
        )}
      </div>
    </main>
  );
}
