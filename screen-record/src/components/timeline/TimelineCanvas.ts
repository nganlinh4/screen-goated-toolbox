// Canvas-based timeline renderer.
//
// Replaces DOM-based track components with a single <canvas> element.
// Eliminates 33K+ DOM nodes and React reconciliation overhead at zoom.
//
// All non-interactive visuals are drawn here:
// - Ruler ticks
// - Audio waveform curves (device audio + mic)
// - Speed curve
// - Zoom curve + keyframe dots
// - Trim segments (active regions + gap overlays)
// - Text/Keystroke/Pointer/Webcam segment blocks
// - Playhead line

import type { VideoSegment } from '@/types/video';
import { getTrimSegments } from '@/lib/trimSegments';
import type { TimelineRulerTick } from './timelineRuler';

// ─── Theme colors (CSS variable fallbacks) ─────────────────────────────────

interface TimelineTheme {
  rulerTickColor: string;
  rulerLabelColor: string;
  rulerLabelFont: string;
  playheadColor: string;
  deviceAudioColor: string;
  micAudioColor: string;
  speedColor: string;
  zoomColor: string;
  trimActiveColor: string;
  trimGapColor: string;
  textSegmentColor: string;
  keystrokeSegmentColor: string;
  pointerSegmentColor: string;
  webcamSegmentColor: string;
  trackBorderColor: string;
  surfaceColor: string;
}

const DARK_THEME: TimelineTheme = {
  rulerTickColor: 'rgba(255,255,255,0.25)',
  rulerLabelColor: 'rgba(255,255,255,0.45)',
  rulerLabelFont: '9px ui-monospace, monospace',
  playheadColor: '#ffffff',
  deviceAudioColor: '#3b82f6',
  micAudioColor: '#f97316',
  speedColor: '#a855f7',
  zoomColor: '#22c55e',
  trimActiveColor: 'rgba(255,255,255,0.08)',
  trimGapColor: 'rgba(0,0,0,0.55)',
  textSegmentColor: '#60a5fa',
  keystrokeSegmentColor: '#4ade80',
  pointerSegmentColor: '#facc15',
  webcamSegmentColor: '#f472b6',
  trackBorderColor: 'rgba(255,255,255,0.06)',
  surfaceColor: 'rgba(255,255,255,0.03)',
};

// ─── Track layout constants ────────────────────────────────────────────────

interface TrackLayout {
  y: number;
  h: number;
  label: string;
}

// Heights in pixels — must match the CSS track heights
const RULER_HEIGHT = 18;
const ZOOM_TRACK_HEIGHT = 40;
const SPEED_TRACK_HEIGHT = 40;
const DEVICE_AUDIO_TRACK_HEIGHT = 40;
const MIC_AUDIO_TRACK_HEIGHT = 40;
const WEBCAM_TRACK_HEIGHT = 28;
const TEXT_TRACK_HEIGHT = 28;
const KEYSTROKE_TRACK_HEIGHT = 28;
const POINTER_TRACK_HEIGHT = 28;
const TRIM_TRACK_HEIGHT = 40;
const TRACK_GAP = 2;

function buildTrackLayout(): TrackLayout[] {
  let y = 0;
  const tracks: TrackLayout[] = [];
  const add = (h: number, label: string) => {
    tracks.push({ y, h, label });
    y += h + TRACK_GAP;
  };
  add(ZOOM_TRACK_HEIGHT, 'zoom');
  add(SPEED_TRACK_HEIGHT, 'speed');
  add(DEVICE_AUDIO_TRACK_HEIGHT, 'deviceAudio');
  add(MIC_AUDIO_TRACK_HEIGHT, 'micAudio');
  add(WEBCAM_TRACK_HEIGHT, 'webcam');
  add(TEXT_TRACK_HEIGHT, 'text');
  add(KEYSTROKE_TRACK_HEIGHT, 'keystroke');
  add(POINTER_TRACK_HEIGHT, 'pointer');
  add(TRIM_TRACK_HEIGHT, 'trim');
  add(RULER_HEIGHT, 'ruler');
  return tracks;
}

export const TRACK_LAYOUTS = buildTrackLayout();
export const TOTAL_CANVAS_HEIGHT = TRACK_LAYOUTS.reduce(
  (max, t) => Math.max(max, t.y + t.h),
  0,
);

// ─── Draw state ────────────────────────────────────────────────────────────

export interface TimelineDrawState {
  segment: VideoSegment | null;
  duration: number;
  currentTime: number;
  zoom: number;
  scrollLeft: number;
  viewportWidth: number;
  canvasWidthPx: number;
  rulerTicks: TimelineRulerTick[];
  thumbnails: string[];
  thumbnailImages?: ImageBitmap[];
  isDark: boolean;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
  dpr: number;
}

// ─── Drawing helpers ───────────────────────────────────────────────────────

function timeToX(time: number, duration: number, canvasWidthPx: number): number {
  if (duration <= 0) return 0;
  return (time / duration) * canvasWidthPx;
}

function clampVolume(v: number): number {
  return Math.max(0, Math.min(1, Number.isFinite(v) ? v : 1));
}

function cosineInterpolate(points: Array<{ time: number; volume?: number; speed?: number }>, time: number, field: 'volume' | 'speed'): number {
  if (points.length === 0) return field === 'volume' ? 1 : 1;
  if (points.length === 1) return (points[0] as any)[field] ?? 1;
  if (time <= points[0].time) return (points[0] as any)[field] ?? 1;
  if (time >= points[points.length - 1].time) return (points[points.length - 1] as any)[field] ?? 1;

  let i = 0;
  while (i < points.length - 1 && points[i + 1].time < time) i++;
  const a = points[i];
  const b = points[i + 1];
  const span = b.time - a.time;
  const t = span > 0 ? (time - a.time) / span : 0;
  const ft = (1 - Math.cos(t * Math.PI)) / 2;
  const va = (a as any)[field] ?? 1;
  const vb = (b as any)[field] ?? 1;
  return va + (vb - va) * ft;
}

// ─── Track draw functions ──────────────────────────────────────────────────

function drawAudioCurve(
  ctx: CanvasRenderingContext2D,
  points: Array<{ time: number; volume: number }>,
  duration: number,
  canvasW: number,
  trackY: number,
  trackH: number,
  color: string,
  visibleLeft: number,
  visibleRight: number,
) {
  if (points.length < 2 || duration <= 0) return;

  const padTop = 5;
  const padBot = 5;
  const range = trackH - padTop - padBot;
  const step = Math.max(1, Math.floor((visibleRight - visibleLeft) / 800));

  // Fill area
  ctx.beginPath();
  let started = false;
  for (let px = Math.floor(visibleLeft); px <= Math.ceil(visibleRight); px += step) {
    const t = (px / canvasW) * duration;
    const vol = clampVolume(cosineInterpolate(points, t, 'volume'));
    const y = trackY + padTop + (1 - vol) * range;
    if (!started) {
      ctx.moveTo(px, trackY + trackH - padBot);
      ctx.lineTo(px, y);
      started = true;
    } else {
      ctx.lineTo(px, y);
    }
  }
  ctx.lineTo(Math.ceil(visibleRight), trackY + trackH - padBot);
  ctx.closePath();
  ctx.fillStyle = color.replace(')', ', 0.12)').replace('rgb(', 'rgba(');
  ctx.fill();

  // Stroke line
  ctx.beginPath();
  started = false;
  for (let px = Math.floor(visibleLeft); px <= Math.ceil(visibleRight); px += step) {
    const t = (px / canvasW) * duration;
    const vol = clampVolume(cosineInterpolate(points, t, 'volume'));
    const y = trackY + padTop + (1 - vol) * range;
    if (!started) {
      ctx.moveTo(px, y);
      started = true;
    } else {
      ctx.lineTo(px, y);
    }
  }
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.stroke();

  // Baseline
  ctx.strokeStyle = color.replace(')', ', 0.18)').replace('rgb(', 'rgba(');
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(visibleLeft, trackY + padTop);
  ctx.lineTo(visibleRight, trackY + padTop);
  ctx.moveTo(visibleLeft, trackY + trackH - padBot);
  ctx.lineTo(visibleRight, trackY + trackH - padBot);
  ctx.stroke();

  // Control point dots (only for actual user points, not interpolated)
  for (const pt of points) {
    const px = timeToX(pt.time, duration, canvasW);
    if (px < visibleLeft - 10 || px > visibleRight + 10) continue;
    const vol = clampVolume(pt.volume);
    const y = trackY + padTop + (1 - vol) * range;
    ctx.beginPath();
    ctx.arc(px, y, 4, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.strokeStyle = 'rgba(0,0,0,0.5)';
    ctx.lineWidth = 1;
    ctx.stroke();
  }
}

function drawSegmentBlocks(
  ctx: CanvasRenderingContext2D,
  segments: Array<{ startTime: number; endTime: number; id?: string }>,
  duration: number,
  canvasW: number,
  trackY: number,
  trackH: number,
  color: string,
  visibleLeft: number,
  visibleRight: number,
) {
  for (const seg of segments) {
    const left = timeToX(seg.startTime, duration, canvasW);
    const right = timeToX(seg.endTime, duration, canvasW);
    if (right < visibleLeft || left > visibleRight) continue;
    const x = Math.max(left, visibleLeft - 1);
    const w = Math.min(right, visibleRight + 1) - x;
    ctx.fillStyle = color;
    const r = Math.min(4, w / 2);
    ctx.beginPath();
    ctx.roundRect(x, trackY + 2, w, trackH - 4, r);
    ctx.fill();
  }
}

function drawTrimTrack(
  ctx: CanvasRenderingContext2D,
  segment: VideoSegment,
  duration: number,
  canvasW: number,
  trackY: number,
  trackH: number,
  theme: TimelineTheme,
  visibleLeft: number,
  visibleRight: number,
  thumbnailImages?: ImageBitmap[],
) {
  const trimSegments = getTrimSegments(segment, duration);

  // Draw thumbnail background (dimmed)
  if (thumbnailImages && thumbnailImages.length > 0) {
    ctx.globalAlpha = 0.06;
    const thumbW = canvasW / thumbnailImages.length;
    for (let i = 0; i < thumbnailImages.length; i++) {
      const x = i * thumbW;
      if (x + thumbW < visibleLeft || x > visibleRight) continue;
      ctx.drawImage(thumbnailImages[i], x, trackY, thumbW, trackH);
    }
    ctx.globalAlpha = 1;
  }

  // Draw active regions (brighter thumbnails)
  for (const seg of trimSegments) {
    const left = timeToX(seg.startTime, duration, canvasW);
    const right = timeToX(seg.endTime, duration, canvasW);
    if (right < visibleLeft || left > visibleRight) continue;
    ctx.save();
    ctx.beginPath();
    ctx.rect(left, trackY, right - left, trackH);
    ctx.clip();
    if (thumbnailImages && thumbnailImages.length > 0) {
      const thumbW = canvasW / thumbnailImages.length;
      for (let i = 0; i < thumbnailImages.length; i++) {
        const x = i * thumbW;
        if (x + thumbW < left || x > right) continue;
        ctx.drawImage(thumbnailImages[i], x, trackY, thumbW, trackH);
      }
    } else {
      ctx.fillStyle = theme.trimActiveColor;
      ctx.fillRect(left, trackY, right - left, trackH);
    }
    ctx.restore();
  }

  // Draw excluded gaps
  const gaps: Array<{ start: number; end: number }> = [];
  let cursor = 0;
  for (const seg of trimSegments) {
    if (seg.startTime > cursor) gaps.push({ start: cursor, end: seg.startTime });
    cursor = seg.endTime;
  }
  if (cursor < duration) gaps.push({ start: cursor, end: duration });

  for (const gap of gaps) {
    const left = timeToX(gap.start, duration, canvasW);
    const right = timeToX(gap.end, duration, canvasW);
    if (right < visibleLeft || left > visibleRight) continue;
    ctx.fillStyle = theme.trimGapColor;
    ctx.fillRect(left, trackY, right - left, trackH);
  }

  // Draw trim segment borders
  ctx.strokeStyle = theme.zoomColor;
  ctx.lineWidth = 2;
  for (const seg of trimSegments) {
    const left = timeToX(seg.startTime, duration, canvasW);
    const right = timeToX(seg.endTime, duration, canvasW);
    if (right < visibleLeft || left > visibleRight) continue;
    ctx.strokeRect(left, trackY, right - left, trackH);
  }
}

function drawRuler(
  ctx: CanvasRenderingContext2D,
  ticks: TimelineRulerTick[],
  canvasW: number,
  trackY: number,
  _trackH: number,
  theme: TimelineTheme,
  visibleLeft: number,
  visibleRight: number,
) {
  ctx.font = theme.rulerLabelFont;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'top';

  for (const tick of ticks) {
    const x = (tick.leftPct / 100) * canvasW;
    if (x < visibleLeft - 30 || x > visibleRight + 30) continue;

    // Tick mark
    ctx.strokeStyle = theme.rulerTickColor;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, trackY);
    ctx.lineTo(x, trackY + 6);
    ctx.stroke();

    // Label
    ctx.fillStyle = theme.rulerLabelColor;
    ctx.fillText(tick.label, x, trackY + 7);
  }
}

function drawPlayhead(
  ctx: CanvasRenderingContext2D,
  currentTime: number,
  duration: number,
  canvasW: number,
  totalHeight: number,
  color: string,
  visibleLeft: number,
  visibleRight: number,
) {
  const x = timeToX(currentTime, duration, canvasW);
  if (x < visibleLeft - 2 || x > visibleRight + 2) return;

  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(x, 0);
  ctx.lineTo(x, totalHeight);
  ctx.stroke();

  // Playhead dot
  ctx.beginPath();
  ctx.arc(x, 0, 4, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();
}

function drawTrackBackground(
  ctx: CanvasRenderingContext2D,
  trackY: number,
  trackH: number,
  theme: TimelineTheme,
  visibleLeft: number,
  visibleRight: number,
) {
  ctx.fillStyle = theme.surfaceColor;
  const r = 6;
  ctx.beginPath();
  ctx.roundRect(visibleLeft, trackY, visibleRight - visibleLeft, trackH, r);
  ctx.fill();
}

// ─── Main draw function ────────────────────────────────────────────────────

export function drawTimeline(
  ctx: CanvasRenderingContext2D,
  state: TimelineDrawState,
) {
  const { segment, duration, currentTime, canvasWidthPx, rulerTicks } = state;

  const canvasW = canvasWidthPx;
  const visibleLeft = state.scrollLeft;
  const visibleRight = state.scrollLeft + state.viewportWidth;
  const theme = DARK_THEME; // TODO: support light theme

  // Clear visible region only (faster than clearing entire wide canvas)
  ctx.clearRect(visibleLeft - 1, 0, visibleRight - visibleLeft + 2, TOTAL_CANVAS_HEIGHT);

  if (!segment || duration <= 0) return;

  const tracks = TRACK_LAYOUTS;
  const deviceAudioPoints = segment.deviceAudioPoints ?? [
    { time: 0, volume: 1 },
    { time: duration, volume: 1 },
  ];
  const micAudioPoints = segment.micAudioPoints ?? [];

  // Draw track backgrounds
  for (const track of tracks) {
    if (track.label === 'ruler') continue;
    drawTrackBackground(ctx, track.y, track.h, theme, visibleLeft, visibleRight);
  }

  // Draw each track
  for (const track of tracks) {
    switch (track.label) {
      case 'deviceAudio':
        if (state.isDeviceAudioAvailable) {
          drawAudioCurve(ctx, deviceAudioPoints, duration, canvasW, track.y, track.h, theme.deviceAudioColor, visibleLeft, visibleRight);
        }
        break;
      case 'micAudio':
        if (state.isMicAudioAvailable && micAudioPoints.length >= 2) {
          drawAudioCurve(ctx, micAudioPoints as any, duration, canvasW, track.y, track.h, theme.micAudioColor, visibleLeft, visibleRight);
        }
        break;
      case 'text':
        drawSegmentBlocks(ctx, segment.textSegments ?? [], duration, canvasW, track.y, track.h, theme.textSegmentColor + '40', visibleLeft, visibleRight);
        break;
      case 'keystroke': {
        const ksSegs = segment.keyboardVisibilitySegments ?? [];
        drawSegmentBlocks(ctx, ksSegs, duration, canvasW, track.y, track.h, theme.keystrokeSegmentColor + '40', visibleLeft, visibleRight);
        break;
      }
      case 'pointer': {
        const ptrSegs = segment.cursorVisibilitySegments ?? [];
        drawSegmentBlocks(ctx, ptrSegs, duration, canvasW, track.y, track.h, theme.pointerSegmentColor + '40', visibleLeft, visibleRight);
        break;
      }
      case 'webcam':
        if (state.isWebcamAvailable) {
          const wcSegs = segment.webcamVisibilitySegments ?? [];
          drawSegmentBlocks(ctx, wcSegs, duration, canvasW, track.y, track.h, theme.webcamSegmentColor + '40', visibleLeft, visibleRight);
        }
        break;
      case 'trim':
        drawTrimTrack(ctx, segment, duration, canvasW, track.y, track.h, theme, visibleLeft, visibleRight, state.thumbnailImages);
        break;
      case 'ruler':
        drawRuler(ctx, rulerTicks, canvasW, track.y, track.h, theme, visibleLeft, visibleRight);
        break;
      // speed and zoom curves will be added in next iteration
    }
  }

  // Draw playhead last (on top)
  drawPlayhead(ctx, currentTime, duration, canvasW, TOTAL_CANVAS_HEIGHT, theme.playheadColor, visibleLeft, visibleRight);
}

// ─── Hit testing ───────────────────────────────────────────────────────────

export interface HitTestResult {
  track: string;
  type: 'background' | 'segment' | 'handle' | 'point' | 'playhead';
  index?: number;
  segmentId?: string;
  handleSide?: 'start' | 'end';
  time: number;
}

export function hitTest(
  x: number,
  y: number,
  state: TimelineDrawState,
): HitTestResult | null {
  if (!state.segment || state.duration <= 0) return null;

  const time = (x / state.canvasWidthPx) * state.duration;

  for (const track of TRACK_LAYOUTS) {
    if (y >= track.y && y < track.y + track.h) {
      return {
        track: track.label,
        type: 'background',
        time,
      };
    }
  }

  return null;
}
