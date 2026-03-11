import React, { useState } from "react";
import type { VideoSegment } from "@/types/video";
import { useSettings } from "@/hooks/useSettings";
import { KeystrokeTrack } from "./KeystrokeTrack";
import { Playhead } from "./Playhead";
import { PointerTrack } from "./PointerTrack";
import { SpeedTrack } from "./SpeedTrack";
import { TextTrack } from "./TextTrack";
import { TrimTrack } from "./TrimTrack";
import { ZoomDebugOverlay } from "./ZoomDebugOverlay";
import { ZoomTrack } from "./ZoomTrack";
import { buildTimelineRulerTicks } from "./timelineRuler";
import { useTimelineDrag } from "./useTimelineDrag";
import { useTimelineViewport } from "./useTimelineViewport";

const TIMELINE_TRACK_GAP_PX = 2;
const TIMELINE_TRACK_HEIGHTS = {
  zoom: 40,
  debug: 40,
  speed: 40,
  text: 28,
  keystroke: 28,
  pointer: 28,
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
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text") => void;
  setSegment: (segment: VideoSegment | null) => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  onAddText?: (atTime?: number) => void;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  isPlaying?: boolean;
  beginBatch: () => void;
  commitBatch: () => void;
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
  editingKeystrokeSegmentId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingKeystrokeSegmentId,
  setEditingPointerId,
  setActivePanel,
  setSegment,
  onSeek,
  onSeekEnd,
  onAddText,
  onAddKeystrokeSegment,
  onAddPointerSegment,
  isPlaying,
  beginBatch,
  commitBatch,
}) => {
  const { t } = useSettings();
  const [showDebug, setShowDebug] = useState(false);
  const showEmptyRuler = duration <= 0;
  const keystrokeTrackLabel =
    segment?.keystrokeMode === "keyboard"
      ? t.trackKeyboard
      : segment?.keystrokeMode === "keyboardMouse"
        ? t.trackKeyboardMouse
        : t.trackKeystrokesOff;
  const trackHeightsBeforeTrim = [
    TIMELINE_TRACK_HEIGHTS.zoom,
    ...(showDebug ? [TIMELINE_TRACK_HEIGHTS.debug] : []),
    TIMELINE_TRACK_HEIGHTS.speed,
    TIMELINE_TRACK_HEIGHTS.text,
    TIMELINE_TRACK_HEIGHTS.keystroke,
    TIMELINE_TRACK_HEIGHTS.pointer,
  ];
  const trimHeadCenterY =
    trackHeightsBeforeTrim.reduce((sum, height) => sum + height, 0) +
    trackHeightsBeforeTrim.length * TIMELINE_TRACK_GAP_PX +
    TIMELINE_TRACK_HEIGHTS.trimLane / 2;
  const trimLaneBottomY = trimHeadCenterY + TIMELINE_TRACK_HEIGHTS.trimLane / 2;
  const {
    dragState,
    handleTrimDragStart,
    handleTrimSplit,
    handleTrimAddSegment,
    handleZoomDragStart,
    handleTextDragStart,
    handleTextClick,
    handleKeystrokeDragStart,
    handleKeystrokeClick,
    handlePointerDragStart,
    handlePointerClick,
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
    setEditingKeystrokeId: setEditingKeystrokeSegmentId,
    setEditingPointerId,
    setActivePanel,
    onSeek,
    onSeekEnd,
    beginBatch,
    commitBatch,
  });

  const isTimelineInteracting =
    dragState.isDraggingTrimStart ||
    dragState.isDraggingTrimEnd ||
    dragState.isDraggingTextStart ||
    dragState.isDraggingTextEnd ||
    dragState.isDraggingTextBody ||
    dragState.isDraggingKeystrokeStart ||
    dragState.isDraggingKeystrokeEnd ||
    dragState.isDraggingKeystrokeBody ||
    dragState.isDraggingPointerStart ||
    dragState.isDraggingPointerEnd ||
    dragState.isDraggingPointerBody ||
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
    handleWheel,
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
  });
  const rulerTicks = buildTimelineRulerTicks({
    duration,
    widthPx: canvasWidthPx,
    speedPoints: segment?.speedPoints,
  });

  return (
    <div className="timeline-area select-none mx-2">
      <div className="timeline-shell flex gap-4">
        <div className="timeline-side-column w-[4rem] flex-shrink-0">
          <div className="timeline-label-gutter flex flex-col gap-[2px] border-r border-[var(--ui-border)] pr-2">
            <div className="timeline-label-zoom h-10 flex items-center justify-between">
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
            {showDebug && (
              <div className="timeline-label-debug h-10 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none opacity-50">
                  dbg
                </span>
              </div>
            )}
            <div className="timeline-label-speed h-10 flex items-center justify-between">
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
            <div className="timeline-label-text h-7 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackText}
              </span>
            </div>
            <div className="timeline-label-keystrokes h-7 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {keystrokeTrackLabel}
              </span>
            </div>
            <div className="timeline-label-pointer h-7 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackPointer}
              </span>
            </div>
            <div className="timeline-label-video h-10 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackVideo}
              </span>
            </div>
          </div>
          <div className="timeline-ruler-spacer h-4 mt-0.5" />
        </div>

        <div className="timeline-main-column flex-1 min-w-0">
          <div
            ref={viewportRef}
            className="timeline-scroll-viewport"
            data-zoomed={zoom > 1 ? "true" : "false"}
            onWheel={handleWheel}
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
                  {segment ? (
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
                    <div className="zoom-track-empty timeline-track-empty h-10" />
                  )}

                  {showDebug && segment && (
                    <ZoomDebugOverlay segment={segment} duration={duration} />
                  )}

                  {segment ? (
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
                    <div className="speed-track-empty timeline-track-empty h-10" />
                  )}

                  {segment ? (
                    <TextTrack
                      segment={segment}
                      duration={duration}
                      editingTextId={editingTextId}
                      onTextClick={handleTextClick}
                      onHandleDragStart={handleTextDragStart}
                      onAddText={onAddText}
                    />
                  ) : (
                    <div className="text-track-empty timeline-track-empty h-7" />
                  )}

                  {segment ? (
                    <KeystrokeTrack
                      segment={segment}
                      duration={duration}
                      editingKeystrokeSegmentId={editingKeystrokeSegmentId}
                      onKeystrokeClick={handleKeystrokeClick}
                      onHandleDragStart={handleKeystrokeDragStart}
                      onAddKeystrokeSegment={onAddKeystrokeSegment}
                      onKeystrokeHover={setEditingKeystrokeSegmentId}
                    />
                  ) : (
                    <div className="keystroke-track-empty timeline-track-empty h-7" />
                  )}

                  {segment ? (
                    <PointerTrack
                      segment={segment}
                      duration={duration}
                      onPointerClick={handlePointerClick}
                      onHandleDragStart={handlePointerDragStart}
                      onAddPointerSegment={onAddPointerSegment}
                      onPointerHover={setEditingPointerId}
                    />
                  ) : (
                    <div className="pointer-track-empty timeline-track-empty h-7" />
                  )}

                  {segment ? (
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
                  )}
                </div>

                {segment && (
                  <Playhead
                    currentTime={currentTime}
                    duration={duration}
                    isPlaying={!!isPlaying}
                    videoRef={videoRef}
                    segment={segment}
                    headCenterY={trimHeadCenterY}
                    lineBottomY={trimLaneBottomY}
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
                      }}
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
