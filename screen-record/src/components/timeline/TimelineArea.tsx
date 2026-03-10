import React, { useState } from "react";
import { VideoSegment } from "@/types/video";
import { ZoomTrack } from "./ZoomTrack";
import { TextTrack } from "./TextTrack";
import { KeystrokeTrack } from "./KeystrokeTrack";
import { PointerTrack } from "./PointerTrack";
import { SpeedTrack } from "./SpeedTrack";
import { TrimTrack } from "./TrimTrack";
import { Playhead } from "./Playhead";
import { useTimelineDrag } from "./useTimelineDrag";
import { useSettings } from "@/hooks/useSettings";
import { ZoomDebugOverlay } from "./ZoomDebugOverlay";
import { videoTimeToWallClock } from "@/lib/exportEstimator";

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

function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
}

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

  return (
    <div className="timeline-area select-none mx-2">
      {/* Track container with label gutter + content area */}
      <div className="timeline-tracks-row flex gap-4">
        {/* Label gutter */}
        <div className="timeline-label-gutter w-[4rem] flex-shrink-0 flex flex-col gap-[2px] border-r border-[var(--ui-border)] pr-2">
          <div className="timeline-label-zoom h-10 flex items-center justify-between">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.trackZoom}
            </span>
            <button
              onClick={() => setShowDebug((v) => !v)}
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

        {/* Content area - timelineRef only covers this, so seek math is correct */}
        <div
          ref={timelineRef}
          className={`timeline-content flex-1 relative touch-none ${
            dragState.isDraggingSeek ? "cursor-grabbing" : "cursor-grab"
          }`}
          onPointerDown={handleMouseDown}
          onPointerMove={handleMouseMove}
          onPointerUp={handleMouseUp}
          onPointerCancel={handleMouseUp}
        >
          <div className="timeline-tracks flex flex-col gap-[2px]">
            {/* Zoom Track */}
            {segment ? (
              <ZoomTrack
                segment={segment}
                duration={duration}
                editingKeyframeId={editingKeyframeId}
                onKeyframeClick={handleKeyframeClick}
                onKeyframeDragStart={handleZoomDragStart}
                onUpdateInfluencePoints={(points) => {
                  const newSegment = {
                    ...segment,
                    zoomInfluencePoints: points,
                  };
                  if (points.length === 0) newSegment.smoothMotionPath = [];
                  setSegment(newSegment);
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

            {/* Debug Overlay */}
            {showDebug && segment && (
              <ZoomDebugOverlay segment={segment} duration={duration} />
            )}

            {/* Speed Track */}
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

            {/* Text Track */}
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

            {/* Keystroke Track */}
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

            {/* Pointer Track */}
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

            {/* Video/Trim Track */}
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
              <div className="trim-track-empty timeline-track-empty h-10" />
            )}
          </div>

          {/* Playhead spanning all tracks */}
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
      </div>

      {/* Time ruler */}
      <div className="timeline-ruler-row flex gap-4 mt-0.5">
        <div className="timeline-ruler-gutter w-[4rem] flex-shrink-0" />
        <div className="timeline-ruler flex-1 relative h-4 select-none">
          {duration > 0 &&
            (() => {
              const speedPoints = segment?.speedPoints;
              const tickCount =
                duration <= 5
                  ? 5
                  : duration <= 15
                    ? 8
                    : duration <= 30
                      ? 10
                      : 12;
              return Array.from({ length: tickCount + 1 }).map((_, i) => {
                const videoTime = (duration * i) / tickCount;
                const left = (i / tickCount) * 100;
                const isMajor =
                  i === 0 ||
                  i === tickCount ||
                  i % Math.ceil(tickCount / 4) === 0;
                const displayTime = speedPoints?.length
                  ? videoTimeToWallClock(videoTime, speedPoints)
                  : videoTime;
                return (
                  <div
                    key={i}
                    className="timeline-tick absolute flex flex-col items-center"
                    style={{
                      left: `${left}%`,
                      transform: "translateX(-50%)",
                      top: 0,
                    }}
                  >
                    <div
                      className={`w-px ${isMajor ? "h-1.5 bg-[var(--outline)]/40" : "h-1 bg-[var(--outline)]/20"}`}
                    />
                    {isMajor && (
                      <span className="text-[9px] font-mono text-[var(--outline)] leading-none mt-0.5">
                        {formatTime(displayTime)}
                      </span>
                    )}
                  </div>
                );
              });
            })()}
        </div>
      </div>
    </div>
  );
};
