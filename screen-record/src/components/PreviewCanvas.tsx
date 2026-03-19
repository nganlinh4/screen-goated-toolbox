import React, { type MutableRefObject, type RefObject } from "react";
import { BackgroundConfig } from "@/types/video";
import {
  Placeholder,
  CanvasResizeOverlay,
  SeekIndicator,
} from "@/components/VideoPreview";

export interface KeystrokeEditFrame {
  left: number;
  top: number;
  width: number;
  height: number;
  handleSize: number;
}

export interface PreviewCanvasProps {
  // Container ref for the outer preview div
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  previewCursorClass: string;
  onMouseDown: (e: React.MouseEvent<HTMLDivElement>) => void;
  // Media element refs
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
  // Keystroke overlay edit frame
  keystrokeOverlayEditFrame: KeystrokeEditFrame | null;
  isKeystrokeOverlaySelected: boolean;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;
  // Playback state
  isBuffering: boolean;
  isPreviewPlaying: boolean;
  currentVideo: string | null;
  // Loading state
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
  // Canvas resize
  isCropping: boolean;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  onCanvasResizeDragStateChange: (dragging: boolean) => void;
  // Seek indicator
  seekIndicatorDir: "left" | "right" | null;
  seekIndicatorKey: number;
}

export function PreviewCanvas({
  previewContainerRef,
  previewCursorClass,
  onMouseDown,
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
  onCanvasResizeDragStateChange,
  seekIndicatorDir,
  seekIndicatorKey,
}: PreviewCanvasProps) {
  return (
    <div className="video-preview-container flex-1 min-h-0 overflow-hidden flex items-center justify-center">
      <div className="preview-inner relative w-full h-full flex justify-center items-center">
        <div
          ref={previewContainerRef}
          className={`preview-canvas relative flex items-center justify-center ${previewCursorClass} group w-full h-full focus:outline-none`}
          onMouseDown={onMouseDown}
          tabIndex={-1}
        >
          <canvas
            ref={canvasRef as React.RefObject<HTMLCanvasElement>}
            className="preview-canvas-element absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 max-w-full max-h-full"
          />
          <canvas ref={tempCanvasRef as React.RefObject<HTMLCanvasElement>} className="hidden" />
          <video
            ref={videoRef as React.RefObject<HTMLVideoElement>}
            className="hidden"
            crossOrigin="anonymous"
            playsInline
            preload="auto"
          />
          <video
            ref={webcamVideoRef as React.RefObject<HTMLVideoElement>}
            className="hidden"
            crossOrigin="anonymous"
            playsInline
            preload="auto"
            muted
          />
          <audio ref={audioRef as React.RefObject<HTMLAudioElement>} className="hidden" />
          <audio ref={micAudioRef as React.RefObject<HTMLAudioElement>} className="hidden" />
          <video
            ref={previousPreloadVideoRef as React.RefObject<HTMLVideoElement>}
            className="hidden"
            crossOrigin="anonymous"
            playsInline
            preload="auto"
            muted
          />
          <audio ref={previousPreloadAudioRef as React.RefObject<HTMLAudioElement>} className="hidden" />
          <video
            ref={nextPreloadVideoRef as React.RefObject<HTMLVideoElement>}
            className="hidden"
            crossOrigin="anonymous"
            playsInline
            preload="auto"
            muted
          />
          <audio ref={nextPreloadAudioRef as React.RefObject<HTMLAudioElement>} className="hidden" />

          {keystrokeOverlayEditFrame &&
            (isKeystrokeOverlaySelected ||
              isDraggingKeystrokeOverlayRef.current ||
              isResizingKeystrokeOverlayRef.current) && (
              <div
                className="keystroke-overlay-edit-frame absolute z-30 pointer-events-none"
                style={{
                  left: `${keystrokeOverlayEditFrame.left}px`,
                  top: `${keystrokeOverlayEditFrame.top}px`,
                  width: `${keystrokeOverlayEditFrame.width}px`,
                  height: `${keystrokeOverlayEditFrame.height}px`,
                }}
              >
                <div className="keystroke-overlay-edit-outline absolute inset-0 rounded-lg border border-emerald-300/85 bg-emerald-400/8 shadow-[0_0_0_1px_rgba(0,0,0,0.28)]" />
                <div
                  className="keystroke-overlay-edit-handle absolute rounded-sm border border-emerald-100/90 bg-emerald-300/95 shadow-[0_2px_8px_rgba(0,0,0,0.35)]"
                  style={{
                    width: `${keystrokeOverlayEditFrame.handleSize}px`,
                    height: `${keystrokeOverlayEditFrame.handleSize}px`,
                    right: `${Math.max(-keystrokeOverlayEditFrame.handleSize * 0.35, -6)}px`,
                    bottom: `${Math.max(-keystrokeOverlayEditFrame.handleSize * 0.35, -6)}px`,
                  }}
                />
              </div>
            )}

          {isBuffering && isPreviewPlaying && currentVideo && (
            <div className="buffering-indicator absolute inset-0 flex items-center justify-center z-20 pointer-events-none">
              <div className="buffering-spinner w-10 h-10 rounded-full border-[3px] border-white/20 border-t-white/80 animate-spin" />
            </div>
          )}

          {(!currentVideo || isLoadingVideo) && (
            <Placeholder
              isLoadingVideo={isLoadingVideo}
              loadingProgress={loadingProgress}
              isRecording={isRecording}
              recordingDuration={recordingDuration}
            />
          )}

          {!isCropping &&
            currentVideo &&
            backgroundConfig.canvasMode === "custom" &&
            backgroundConfig.canvasWidth &&
            backgroundConfig.canvasHeight && (
              <CanvasResizeOverlay
                previewContainerRef={
                  previewContainerRef as React.RefObject<HTMLDivElement>
                }
                backgroundConfig={backgroundConfig}
                setBackgroundConfig={setBackgroundConfig}
                beginBatch={beginBatch}
                commitBatch={commitBatch}
                onDragStateChange={onCanvasResizeDragStateChange}
              />
            )}

          {seekIndicatorDir && (
            <SeekIndicator dir={seekIndicatorDir} showKey={seekIndicatorKey} />
          )}
        </div>
      </div>
    </div>
  );
}
