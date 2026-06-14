import React from "react";
import { MicAudioPoint, VideoSegment } from "@/types/video";
import {
  buildFlatMicAudioPoints,
  clampMicAudioVolume,
  getMicAudioVolumeAtTime,
} from "@/lib/micAudio";
import { type VolumeTrackGeometry } from "./audioVolumeTrackGeometry";
import { AudioVolumeTrack } from "./AudioVolumeTrack";

const MIC_TRACK_TOP_PX = 5;
const MIC_TRACK_BOTTOM_PX = 35;
const MIC_TRACK_RANGE_PX = MIC_TRACK_BOTTOM_PX - MIC_TRACK_TOP_PX;
const MIC_TRACK_VIEWBOX_HEIGHT = 40;
const MIC_TRACK_GEOMETRY = {
  topPx: MIC_TRACK_TOP_PX,
  bottomPx: MIC_TRACK_BOTTOM_PX,
  viewBoxHeight: MIC_TRACK_VIEWBOX_HEIGHT,
  emptyPathY: MIC_TRACK_BOTTOM_PX,
  clampVolume: clampMicAudioVolume,
} satisfies VolumeTrackGeometry;

interface MicTrackProps {
  segment: VideoSegment;
  duration: number;
  isAvailable: boolean;
  sourcePath?: string | null;
  viewMode?: "compact" | "volume";
  onUpdateMicAudioPoints: (points: MicAudioPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const MicTrack: React.FC<MicTrackProps> = ({
  segment,
  duration,
  isAvailable,
  sourcePath,
  viewMode = "volume",
  onUpdateMicAudioPoints,
  beginBatch,
  commitBatch,
}) => {
  const points = segment.micAudioPoints?.length
    ? segment.micAudioPoints
    : buildFlatMicAudioPoints(duration);

  return (
    <AudioVolumeTrack
      points={points}
      onUpdatePoints={onUpdateMicAudioPoints}
      duration={duration}
      isAvailable={isAvailable}
      sourcePath={sourcePath}
      viewMode={viewMode}
      geometry={MIC_TRACK_GEOMETRY}
      rangePx={MIC_TRACK_RANGE_PX}
      classNamePrefix="mic-audio"
      tone="info"
      colorVariable="--timeline-mic-audio-color"
      hoveredRingClass="ring-2 ring-[var(--timeline-mic-audio-color)]/40"
      getVolumeAtTime={getMicAudioVolumeAtTime}
      offsetSec={segment.micAudioOffsetSec ?? 0}
      beginBatch={beginBatch}
      commitBatch={commitBatch}
    />
  );
};
