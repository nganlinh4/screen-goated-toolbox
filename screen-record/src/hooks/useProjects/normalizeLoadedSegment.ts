import { Project, VideoSegment } from "@/types/video";
import { clampVisibilitySegmentsToDuration } from "@/lib/cursorHiding";
import { normalizeSegmentTrimData } from "@/lib/trimSegments";
import {
  ensureKeystrokeVisibilitySegments,
  filterKeystrokeEventsByMode,
  rebuildKeystrokeVisibilitySegmentsForMode,
} from "@/lib/keystrokeVisibility";
import { normalizeDeviceAudioPoints } from "@/lib/deviceAudio";
import { normalizeMicAudioPoints } from "@/lib/micAudio";
import { normalizeWebcamVisibilitySegments } from "@/lib/webcamVisibility";
import { normalizeSubtitleTrackState } from "@/lib/subtitleTracks";
import {
  normalizeCropRect,
  normalizeTrackDelaySec,
  DEFAULT_KEYSTROKE_DELAY_SEC,
} from "../videoStatePreferences";

export interface NormalizeLoadedSegmentParams {
  project: Project;
  isTimelineOnlyProject: boolean;
  videoDuration: number;
  rawWebcamVideoPath: string;
  micAudioObjectUrl: string | undefined;
  webcamVideoObjectUrl: string | undefined;
}

/**
 * Build the load-corrected segment from a loaded project, applying all the
 * load-time defaulting/normalization. Extracted verbatim from the project load
 * path so behavior is unchanged.
 *
 * Note: this defers to the shared lib normalizers
 * (normalizeSegmentTrimData, normalizeSubtitleTrackState,
 * normalizeDeviceAudioPoints, normalizeMicAudioPoints,
 * normalizeWebcamVisibilitySegments, clampVisibilitySegmentsToDuration,
 * ensureKeystrokeVisibilitySegments, ...) for the parts they own, and keeps the
 * remaining inline defaults that are genuinely load-specific (crop guard,
 * text/subtitle array guards, useCustomCursor / audio-available / webcam-
 * available defaults, keystroke mode/events/delay/overlay defaults, speed
 * points, pointer-segment materialization).
 */
export function normalizeLoadedSegment({
  project,
  isTimelineOnlyProject,
  videoDuration,
  rawWebcamVideoPath,
  micAudioObjectUrl,
  webcamVideoObjectUrl,
}: NormalizeLoadedSegmentParams): VideoSegment {
  let correctedSegment = { ...project.segment };
  if (isTimelineOnlyProject) {
    correctedSegment.mediaMode = "timelineOnly";
  }
  const hasExplicitPointerSegments = Array.isArray(
    correctedSegment.cursorVisibilitySegments,
  );
  if (
    correctedSegment.trimEnd === 0 ||
    correctedSegment.trimEnd > videoDuration
  ) {
    correctedSegment.trimEnd = videoDuration;
  }
  correctedSegment = normalizeSegmentTrimData(
    correctedSegment,
    videoDuration,
  );
  if (typeof correctedSegment.useCustomCursor !== "boolean") {
    correctedSegment.useCustomCursor =
      project.recordingMode === "withCursor" ? false : true;
  }
  correctedSegment.crop = normalizeCropRect(correctedSegment.crop);
  correctedSegment.textSegments = Array.isArray(correctedSegment.textSegments)
    ? correctedSegment.textSegments
    : [];
  correctedSegment.subtitleSegments = Array.isArray(correctedSegment.subtitleSegments)
    ? correctedSegment.subtitleSegments
    : [];
  correctedSegment = normalizeSubtitleTrackState(correctedSegment);
  correctedSegment.deviceAudioPoints = normalizeDeviceAudioPoints(
    correctedSegment.deviceAudioPoints,
    videoDuration,
    project.backgroundConfig.volume,
  );
  correctedSegment.micAudioPoints = normalizeMicAudioPoints(
    correctedSegment.micAudioPoints,
    videoDuration,
  );
  correctedSegment.micAudioOffsetSec = normalizeTrackDelaySec(
    correctedSegment.micAudioOffsetSec,
  );
  correctedSegment.deviceAudioAvailable =
    correctedSegment.deviceAudioAvailable !== false;
  correctedSegment.micAudioAvailable =
    typeof correctedSegment.micAudioAvailable === "boolean"
      ? correctedSegment.micAudioAvailable
      : Boolean(project.rawMicAudioPath || project.micAudioBlob || micAudioObjectUrl);
  correctedSegment.webcamAvailable =
    typeof correctedSegment.webcamAvailable === "boolean"
      ? correctedSegment.webcamAvailable
      : Boolean(rawWebcamVideoPath || project.webcamBlob || webcamVideoObjectUrl);
  correctedSegment.webcamOffsetSec = normalizeTrackDelaySec(
    correctedSegment.webcamOffsetSec,
  );
  correctedSegment.webcamVisibilitySegments = normalizeWebcamVisibilitySegments(
    correctedSegment.webcamVisibilitySegments,
    videoDuration,
    correctedSegment.webcamAvailable !== false,
  );
  correctedSegment.cursorVisibilitySegments =
    clampVisibilitySegmentsToDuration(
      correctedSegment.cursorVisibilitySegments,
      videoDuration,
    );
  correctedSegment.keyboardVisibilitySegments =
    clampVisibilitySegmentsToDuration(
      correctedSegment.keyboardVisibilitySegments,
      videoDuration,
    );
  correctedSegment.keyboardMouseVisibilitySegments =
    clampVisibilitySegmentsToDuration(
      correctedSegment.keyboardMouseVisibilitySegments,
      videoDuration,
    );
  // Materialize pointer segments for backward-compat (old projects have undefined)
  if (!hasExplicitPointerSegments) {
    correctedSegment.cursorVisibilitySegments = [
      {
        id: crypto.randomUUID(),
        startTime: 0,
        endTime: videoDuration,
      },
    ];
  }
  if (
    !correctedSegment.speedPoints ||
    correctedSegment.speedPoints.length === 0
  ) {
    correctedSegment.speedPoints = [
      { time: 0, speed: 1 },
      { time: videoDuration, speed: 1 },
    ];
  }
  if (!correctedSegment.keystrokeMode) {
    correctedSegment.keystrokeMode = "off";
  }
  if (!Array.isArray(correctedSegment.keystrokeEvents)) {
    correctedSegment.keystrokeEvents = [];
  }
  if (
    typeof correctedSegment.keystrokeDelaySec !== "number" ||
    Number.isNaN(correctedSegment.keystrokeDelaySec)
  ) {
    correctedSegment.keystrokeDelaySec = DEFAULT_KEYSTROKE_DELAY_SEC;
  } else {
    correctedSegment.keystrokeDelaySec = Math.max(
      -1,
      Math.min(1, correctedSegment.keystrokeDelaySec),
    );
  }
  const overlay = correctedSegment.keystrokeOverlay;
  correctedSegment.keystrokeOverlay = {
    x:
      typeof overlay?.x === "number"
        ? Math.max(0, Math.min(100, overlay.x))
        : 50,
    y:
      typeof overlay?.y === "number"
        ? Math.max(0, Math.min(100, overlay.y))
        : 100,
    scale:
      typeof overlay?.scale === "number" && Number.isFinite(overlay.scale)
        ? Math.max(0.45, Math.min(2.4, overlay.scale))
        : 1,
  };
  correctedSegment = ensureKeystrokeVisibilitySegments(
    correctedSegment,
    videoDuration,
  );
  const loadedMode = correctedSegment.keystrokeMode ?? "off";
  if (loadedMode === "keyboard" || loadedMode === "keyboardMouse") {
    const modeEvents = filterKeystrokeEventsByMode(
      correctedSegment.keystrokeEvents ?? [],
      loadedMode,
    );
    const modeSegments =
      loadedMode === "keyboard"
        ? (correctedSegment.keyboardVisibilitySegments ?? [])
        : (correctedSegment.keyboardMouseVisibilitySegments ?? []);
    if (modeSegments.length === 0 && modeEvents.length > 0) {
      correctedSegment = rebuildKeystrokeVisibilitySegmentsForMode(
        correctedSegment,
        loadedMode,
        videoDuration,
      );
    }
  }

  return correctedSegment;
}
