import React, { useCallback, useMemo, useState, type MutableRefObject, type RefObject } from "react";
import {
  BackgroundConfig,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import { videoTimeToWallClock } from "@/lib/exportEstimator";
import { PreviewCanvas, type KeystrokeEditFrame } from "@/components/PreviewCanvas";
import { PlaybackControlsRow } from "@/components/PlaybackControlsRow";
import { SidePanel, type ActivePanel } from "@/components/sidepanel/index";
import { TimelineArea } from "@/components/timeline";
import type { CanvasModeToggleProps } from "@/components/CanvasModeToggle";

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
  // PlaybackControlsRow props
  isPlaying: boolean;
  isProcessing: boolean;
  isVideoReady: boolean;
  hasAppliedCrop: boolean;
  currentTime: number;
  duration: number;
  handleTogglePlayPause: () => void;
  handleToggleCrop: () => void;
  customCanvasBaseDimensions: { width: number; height: number };
  getAutoCanvasSelectionConfig: CanvasModeToggleProps["getAutoCanvasSelectionConfig"];
  handleActivateCustomCanvas: () => void;
  handleApplyCanvasRatioPreset: (ratioWidth: number, ratioHeight: number) => void;
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment | null) => void;
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
  // TimelineArea props
  thumbnails: string[];
  timelineRef: React.RefObject<HTMLDivElement>;
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  seek: (time: number) => void;
  flushSeek: () => void;
  handleAddText: () => void;
  handleAddKeystrokeSegment: (atTime?: number) => void;
  handleAddPointerSegment: (atTime?: number) => void;
  setTimelineCanvasWidthPx: (width: number) => void;
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
  isPlaying,
  isProcessing,
  isVideoReady,
  hasAppliedCrop,
  currentTime,
  duration,
  handleTogglePlayPause,
  handleToggleCrop,
  customCanvasBaseDimensions,
  getAutoCanvasSelectionConfig,
  handleActivateCustomCanvas,
  handleApplyCanvasRatioPreset,
  segment,
  setSegment,
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
  thumbnails,
  timelineRef,
  editingKeystrokeSegmentId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingKeystrokeSegmentId,
  setEditingPointerId,
  seek,
  flushSeek,
  handleAddText,
  handleAddKeystrokeSegment,
  handleAddPointerSegment,
  setTimelineCanvasWidthPx,
}: EditorMainProps) {
  const showPlaybackControls = Boolean(
    currentVideo && !isLoadingVideo && !isOverlayMode,
  );
  const showPlaybackControlsGhost = Boolean(
    !currentVideo && !isLoadingVideo && !isCropping,
  );
  const wallClockDuration = useMemo(() => {
    const pts = segment?.speedPoints;
    if (!pts?.length || !duration) return duration;
    return videoTimeToWallClock(duration, pts);
  }, [duration, segment?.speedPoints]);

  const [selectedTextIds, setSelectedTextIds] = useState<string[]>([]);
  const [selectedPointerIds, setSelectedPointerIds] = useState<string[]>([]);
  const [selectedKeystrokeIds, setSelectedKeystrokeIds] = useState<string[]>([]);
  const [selectedWebcamIds, setSelectedWebcamIds] = useState<string[]>([]);

  const handleTextSelectionChange = useCallback((ids: string[]) => {
    setSelectedTextIds(ids);
    if (ids.length > 0) setActivePanel('text');
  }, [setActivePanel]);
  const handlePointerSelectionChange = useCallback((ids: string[]) => setSelectedPointerIds(ids), []);
  const handleKeystrokeSelectionChange = useCallback((ids: string[]) => setSelectedKeystrokeIds(ids), []);
  const handleWebcamSelectionChange = useCallback((ids: string[]) => setSelectedWebcamIds(ids), []);

  const totalSelectedCount = selectedTextIds.length + selectedPointerIds.length + selectedKeystrokeIds.length + selectedWebcamIds.length;
  const [clearSignal, setClearSignal] = useState(0);
  const clearAllSelections = useCallback(() => {
    setSelectedTextIds([]);
    setSelectedPointerIds([]);
    setSelectedKeystrokeIds([]);
    setSelectedWebcamIds([]);
    setClearSignal(c => c + 1);
  }, []);

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
            backgroundConfig={backgroundConfig}
            setBackgroundConfig={setBackgroundConfig}
            customCanvasBaseDimensions={customCanvasBaseDimensions}
            getAutoCanvasSelectionConfig={getAutoCanvasSelectionConfig}
            handleActivateCustomCanvas={handleActivateCustomCanvas}
            handleApplyCanvasRatioPreset={handleApplyCanvasRatioPreset}
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
          editingKeystrokeSegmentId={editingKeystrokeSegmentId}
          setCurrentTime={setCurrentTime}
          setEditingKeyframeId={setEditingKeyframeId}
          setEditingTextId={setEditingTextId}
          setEditingKeystrokeSegmentId={setEditingKeystrokeSegmentId}
          setEditingPointerId={setEditingPointerId}
          setActivePanel={setActivePanel}
          setSegment={setSegment}
          onSeek={seek}
          onSeekEnd={flushSeek}
          onAddText={handleAddText}
          onAddKeystrokeSegment={handleAddKeystrokeSegment}
          onAddPointerSegment={handleAddPointerSegment}
          isPlaying={isPlaying}
          onViewportCanvasWidthChange={setTimelineCanvasWidthPx}
          isDeviceAudioAvailable={segment?.deviceAudioAvailable !== false}
          isMicAudioAvailable={Boolean(segment?.micAudioAvailable)}
          isWebcamAvailable={Boolean(segment?.webcamAvailable)}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
          onTextSelectionChange={handleTextSelectionChange}
          onPointerSelectionChange={handlePointerSelectionChange}
          onKeystrokeSelectionChange={handleKeystrokeSelectionChange}
          onWebcamSelectionChange={handleWebcamSelectionChange}
          clearSelectionSignal={clearSignal}
          hasMouseData={mousePositionsLength > 0}
        />
        {isOverlayMode && (
          <div className="timeline-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />
        )}
      </div>
    </main>
  );
}
