import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";
import type { BackgroundConfig, VideoSegment } from "@/types/video";

export function buildPlaybackStructureSignature(nextSegment: VideoSegment) {
  return JSON.stringify({
    trimStart: nextSegment.trimStart,
    trimEnd: nextSegment.trimEnd,
    trimSegments: (nextSegment.trimSegments ?? []).map((trimSegment) => [
      trimSegment.startTime,
      trimSegment.endTime,
    ]),
    crop: nextSegment.crop ?? null,
    zoomBlocks: (nextSegment.zoomBlocks ?? []).map((block) => [
      block.startTime,
      block.endTime,
      block.easeIn,
      block.easeOut,
      block.zoomFactor,
      block.positionX,
      block.positionY,
      block.followCursor ? 1 : 0,
      block.directTransitionToNext ? 1 : 0,
      block.enabled === false ? 0 : 1,
    ]),
    speedPoints: (nextSegment.speedPoints ?? []).map((point) => [
      point.time,
      point.speed,
    ]),
    deviceAudioPoints: (nextSegment.deviceAudioPoints ?? []).map((point) => [
      point.time,
      point.volume,
    ]),
    micAudioPoints: (nextSegment.micAudioPoints ?? []).map((point) => [
      point.time,
      point.volume,
    ]),
    textSegments: (nextSegment.textSegments ?? []).map((textSegment) => [
      textSegment.id,
      textSegment.startTime,
      textSegment.endTime,
      textSegment.text,
    ]),
    activeSubtitleView: nextSegment.activeSubtitleView ?? null,
    subtitleCustomChain: nextSegment.subtitleCustomChain ?? [],
    subtitleSegments: getVisibleSubtitleSegments(nextSegment).map((subtitleSegment) => [
      subtitleSegment.id,
      subtitleSegment.startTime,
      subtitleSegment.endTime,
      subtitleSegment.text,
      subtitleSegment.style,
    ]),
    keystrokeOverlay: nextSegment.keystrokeOverlay ?? null,
    cursorVisibilitySegments: nextSegment.cursorVisibilitySegments ?? [],
    useCustomCursor: nextSegment.useCustomCursor,
    deviceAudioOffsetSec: nextSegment.deviceAudioOffsetSec,
    micAudioOffsetSec: nextSegment.micAudioOffsetSec,
    webcamOffsetSec: nextSegment.webcamOffsetSec,
  });
}

export function getPlaybackRenderSegment(
  segment: VideoSegment,
  isCropping: boolean,
) {
  return isCropping
    ? {
        ...segment,
        crop: undefined,
        zoomBlocks: [],
      }
    : segment;
}

export function getPlaybackRenderBackground(
  backgroundConfig: BackgroundConfig,
  isCropping: boolean,
) {
  return isCropping
    ? {
        ...backgroundConfig,
        scale: 100,
        borderRadius: 0,
        shadow: 0,
        backgroundType: "solid" as const,
        customBackground: undefined,
        cropBottom: 0,
        canvasMode: "auto" as const,
      }
    : cloneBackgroundConfig(backgroundConfig);
}
