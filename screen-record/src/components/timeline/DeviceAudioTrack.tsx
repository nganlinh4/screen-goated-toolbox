import React from "react";
import { DeviceAudioPoint, VideoSegment } from "@/types/video";
import {
  buildFlatDeviceAudioPoints,
  clampDeviceAudioVolume,
  getDeviceAudioVolumeAtTime,
} from "@/lib/deviceAudio";
import { type VolumeTrackGeometry } from "./audioVolumeTrackGeometry";
import { AudioVolumeTrack } from "./AudioVolumeTrack";

const DEVICE_AUDIO_TRACK_TOP_PX = 5;
const DEVICE_AUDIO_TRACK_BOTTOM_PX = 35;
const DEVICE_AUDIO_TRACK_RANGE_PX =
  DEVICE_AUDIO_TRACK_BOTTOM_PX - DEVICE_AUDIO_TRACK_TOP_PX;
const DEVICE_AUDIO_TRACK_VIEWBOX_HEIGHT = 40;
const DEVICE_AUDIO_TRACK_GEOMETRY = {
  topPx: DEVICE_AUDIO_TRACK_TOP_PX,
  bottomPx: DEVICE_AUDIO_TRACK_BOTTOM_PX,
  viewBoxHeight: DEVICE_AUDIO_TRACK_VIEWBOX_HEIGHT,
  emptyPathY: DEVICE_AUDIO_TRACK_TOP_PX,
  clampVolume: clampDeviceAudioVolume,
} satisfies VolumeTrackGeometry;

interface DeviceAudioTrackProps {
  segment: VideoSegment;
  duration: number;
  isAvailable: boolean;
  sourcePath?: string | null;
  viewMode?: "compact" | "volume";
  onUpdateDeviceAudioPoints: (points: DeviceAudioPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const DeviceAudioTrack: React.FC<DeviceAudioTrackProps> = ({
  segment,
  duration,
  isAvailable,
  sourcePath,
  viewMode = "volume",
  onUpdateDeviceAudioPoints,
  beginBatch,
  commitBatch,
}) => {
  const points = segment.deviceAudioPoints?.length
    ? segment.deviceAudioPoints
    : buildFlatDeviceAudioPoints(duration);

  return (
    <AudioVolumeTrack
      points={points}
      onUpdatePoints={onUpdateDeviceAudioPoints}
      duration={duration}
      isAvailable={isAvailable}
      sourcePath={sourcePath}
      viewMode={viewMode}
      geometry={DEVICE_AUDIO_TRACK_GEOMETRY}
      rangePx={DEVICE_AUDIO_TRACK_RANGE_PX}
      classNamePrefix="device-audio"
      tone="danger"
      colorVariable="--timeline-device-audio-color"
      hoveredRingClass="ring-2 ring-[var(--timeline-device-audio-color)]/40"
      getVolumeAtTime={getDeviceAudioVolumeAtTime}
      beginBatch={beginBatch}
      commitBatch={commitBatch}
    />
  );
};
