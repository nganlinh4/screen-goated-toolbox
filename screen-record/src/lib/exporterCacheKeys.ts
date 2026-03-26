import type { BackgroundConfig, MousePosition, VideoSegment } from '@/types/video';

// ---------------------------------------------------------------------------
// Cache key building helpers for VideoExporter prep cache
// ---------------------------------------------------------------------------

export function buildTimeSegmentStamp(
  segments: Array<{ startTime: number; endTime: number }> | undefined
): string {
  if (!segments || segments.length === 0) return '0:0';
  let hash = 2166136261 >>> 0;
  for (const seg of segments) {
    const startMs = Math.round(seg.startTime * 1000);
    const endMs = Math.round(seg.endTime * 1000);
    hash ^= startMs;
    hash = Math.imul(hash, 16777619) >>> 0;
    hash ^= endMs;
    hash = Math.imul(hash, 16777619) >>> 0;
  }
  return `${segments.length}:${hash.toString(16)}`;
}

export function buildJsonHash(value: unknown): string {
  let json = '';
  try {
    json = JSON.stringify(value) ?? '';
  } catch {
    json = '';
  }
  let hash = 2166136261 >>> 0;
  for (let i = 0; i < json.length; i++) {
    hash ^= json.charCodeAt(i);
    hash = Math.imul(hash, 16777619) >>> 0;
  }
  return `${json.length}:${hash.toString(16)}`;
}

export function buildMousePositionsStamp(positions: MousePosition[]): string {
  if (positions.length === 0) return '0:0';
  return buildJsonHash(positions.map((position) => ({
    x: Math.round(position.x * 1000) / 1000,
    y: Math.round(position.y * 1000) / 1000,
    timestamp: Math.round(position.timestamp * 1000) / 1000,
    isClicked: Boolean(position.isClicked),
    cursorType: position.cursor_type ?? '',
    rotation: Math.round((position.cursor_rotation ?? 0) * 10000) / 10000,
    captureWidth: Math.round((position.captureWidth ?? 0) * 1000) / 1000,
    captureHeight: Math.round((position.captureHeight ?? 0) * 1000) / 1000,
  })));
}

export function buildSegmentContentStamp(segment: VideoSegment): string {
  return buildJsonHash({
    trimStart: Math.round(segment.trimStart * 1000) / 1000,
    trimEnd: Math.round(segment.trimEnd * 1000) / 1000,
    trimSegments: (segment.trimSegments ?? []).map((trim) => ({
      start: Math.round(trim.startTime * 1000) / 1000,
      end: Math.round(trim.endTime * 1000) / 1000,
    })),
    zoomKeyframes: (segment.zoomKeyframes ?? []).map((frame) => ({
      time: Math.round(frame.time * 1000) / 1000,
      duration: Math.round(frame.duration * 1000) / 1000,
      zoomFactor: Math.round(frame.zoomFactor * 10000) / 10000,
      positionX: Math.round(frame.positionX * 10000) / 10000,
      positionY: Math.round(frame.positionY * 10000) / 10000,
      easingType: frame.easingType,
    })),
    speedPoints: (segment.speedPoints ?? []).map((point) => ({
      time: Math.round(point.time * 1000) / 1000,
      speed: Math.round(point.speed * 10000) / 10000,
    })),
    deviceAudioPoints: (segment.deviceAudioPoints ?? []).map((point) => ({
      time: Math.round(point.time * 1000) / 1000,
      volume: Math.round(point.volume * 10000) / 10000,
    })),
    micAudioPoints: (segment.micAudioPoints ?? []).map((point) => ({
      time: Math.round(point.time * 1000) / 1000,
      volume: Math.round(point.volume * 10000) / 10000,
    })),
    micAudioOffsetSec: Math.round((segment.micAudioOffsetSec ?? 0) * 10000) / 10000,
    webcamVisibilitySegments: (segment.webcamVisibilitySegments ?? []).map((range) => ({
      start: Math.round(range.startTime * 1000) / 1000,
      end: Math.round(range.endTime * 1000) / 1000,
    })),
    webcamOffsetSec: Math.round((segment.webcamOffsetSec ?? 0) * 10000) / 10000,
    textSegments: (segment.textSegments ?? []).map((text) => ({
      id: text.id,
      start: Math.round(text.startTime * 1000) / 1000,
      end: Math.round(text.endTime * 1000) / 1000,
      text: text.text,
      style: text.style,
    })),
    cursorVisibility: (segment.cursorVisibilitySegments ?? []).map((visibility) => ({
      start: Math.round(visibility.startTime * 1000) / 1000,
      end: Math.round(visibility.endTime * 1000) / 1000,
    })),
    crop: segment.crop ?? null,
    useCustomCursor: segment.useCustomCursor ?? true,
    keystrokeMode: segment.keystrokeMode ?? 'off',
    keystrokeLanguage: segment.keystrokeLanguage ?? 'en',
    keystrokeDelaySec: Math.round((segment.keystrokeDelaySec ?? 0) * 1000) / 1000,
    keystrokeOverlay: segment.keystrokeOverlay ?? null,
    keyboardVisibility: (segment.keyboardVisibilitySegments ?? []).map((visibility) => ({
      start: Math.round(visibility.startTime * 1000) / 1000,
      end: Math.round(visibility.endTime * 1000) / 1000,
    })),
    keyboardMouseVisibility: (segment.keyboardMouseVisibilitySegments ?? []).map((visibility) => ({
      start: Math.round(visibility.startTime * 1000) / 1000,
      end: Math.round(visibility.endTime * 1000) / 1000,
    })),
    keystrokeEvents: (segment.keystrokeEvents ?? []).map((event) => ({
      id: event.id,
      type: event.type,
      start: Math.round(event.startTime * 1000) / 1000,
      end: Math.round(event.endTime * 1000) / 1000,
      label: event.label,
      count: event.count,
      isHold: Boolean(event.isHold),
      key: event.key ?? '',
      btn: event.btn ?? '',
      direction: event.direction ?? '',
      modifiers: {
        ctrl: Boolean(event.modifiers?.ctrl),
        alt: Boolean(event.modifiers?.alt),
        shift: Boolean(event.modifiers?.shift),
        win: Boolean(event.modifiers?.win),
      },
    })),
  });
}

export function buildBackgroundStamp(backgroundConfig: BackgroundConfig | undefined): string {
  if (!backgroundConfig) return 'none';
  const customBackground = backgroundConfig.customBackground ?? '';
  return buildJsonHash({
    ...backgroundConfig,
    customBackground: customBackground
      ? `${customBackground.length}:${customBackground.slice(0, 64)}:${customBackground.slice(-64)}`
      : '',
  });
}
