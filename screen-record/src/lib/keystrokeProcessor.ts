import { KeystrokeEvent, RawInputEvent, InputModifiers } from '@/types/video';

const DEFAULT_DISPLAY_DURATION_SEC = 1.2;
const HOLD_MIN_DURATION_SEC = 0.2;
const MIN_RELEASE_DISPLAY_SEC = 0.34;

function createEventId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function normalizeModifiers(mods?: InputModifiers): Required<InputModifiers> {
  return {
    ctrl: Boolean(mods?.ctrl),
    alt: Boolean(mods?.alt),
    shift: Boolean(mods?.shift),
    win: Boolean(mods?.win),
  };
}

function isModifierKey(key?: string): boolean {
  return ['Shift', 'Ctrl', 'Alt', 'Win'].includes(key || '');
}

function modifierPrefix(mods: InputModifiers): string {
  const parts: string[] = [];
  if (mods.ctrl) parts.push('Ctrl');
  if (mods.alt) parts.push('Alt');
  if (mods.shift) parts.push('Shift');
  if (mods.win) parts.push('Win');
  return parts.join(' + ');
}

function formatLabel(event: RawInputEvent, mods: InputModifiers): string {
  const prefix = modifierPrefix(mods);

  if (event.type === 'wheel') {
    const arrow = event.direction === 'up' ? '↑' : event.direction === 'down' ? '↓' : '';
    const wheel = `${arrow} Scroll`.trim();
    return prefix ? `${prefix} + ${wheel}` : wheel;
  }

  if (event.type === 'mousedown') {
    const btn = event.btn ? `${event.btn.charAt(0).toUpperCase()}${event.btn.slice(1)}` : 'Mouse';
    const clickText = `${btn} Click`;
    return prefix ? `${prefix} + ${clickText}` : clickText;
  }

  const key = event.key || `VK_${event.vk ?? 0}`;
  return prefix ? `${prefix} + ${key}` : key;
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

function computePercentile(values: number[], percentile: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.max(0, Math.min(sorted.length - 1, Math.floor((sorted.length - 1) * percentile)));
  return sorted[idx];
}

function keyboardToken(event: RawInputEvent): string {
  if (typeof event.vk === 'number') return `vk:${event.vk}`;
  return `key:${event.key ?? ''}`;
}

function mouseToken(event: RawInputEvent): string {
  return `btn:${event.btn ?? 'mouse'}`;
}

export function buildKeystrokeEvents(
  rawEvents: RawInputEvent[],
  duration: number,
  displayDurationSec: number = DEFAULT_DISPLAY_DURATION_SEC
): KeystrokeEvent[] {
  if (!rawEvents.length) return [];

  const out: KeystrokeEvent[] = [];
  const maxTime = duration > 0 ? duration : Number.MAX_SAFE_INTEGER;

  const sorted = [...rawEvents]
    .filter((event) => typeof event.timestamp === 'number' && Number.isFinite(event.timestamp))
    .sort((a, b) => a.timestamp - b.timestamp);
  const activeKeyboardEvents = new Map<string, number>();
  const activeMouseEvents = new Map<string, number>();

  for (const raw of sorted) {
    const mods = normalizeModifiers(raw.modifiers);

    if (raw.type === 'keyboard') {
      if (isModifierKey(raw.key)) continue;
      const token = keyboardToken(raw);
      const keyDirection = raw.direction ?? 'down';
      const timestamp = clamp(raw.timestamp, 0, maxTime);

      if (keyDirection === 'up') {
        const activeIndex = activeKeyboardEvents.get(token);
        if (activeIndex !== undefined && out[activeIndex]) {
          const active = out[activeIndex];
          const physicalDuration = Math.max(0, timestamp - active.startTime);
          active.endTime = clamp(Math.max(active.startTime + MIN_RELEASE_DISPLAY_SEC, timestamp), 0, maxTime);
          active.isHold = physicalDuration >= HOLD_MIN_DURATION_SEC;
          activeKeyboardEvents.delete(token);
        }
        continue;
      }

      const endTime = clamp(timestamp + displayDurationSec, 0, maxTime);
      if (endTime - timestamp <= 0.001) continue;
      const label = formatLabel(raw, mods);
      const candidate: KeystrokeEvent = {
        id: createEventId(),
        type: raw.type,
        startTime: timestamp,
        endTime,
        label,
        count: 1,
        isHold: false,
        modifiers: mods,
        key: raw.key,
        btn: raw.btn,
        direction: 'down',
      };
      out.push(candidate);
      activeKeyboardEvents.set(token, out.length - 1);
      continue;
    }

    if (raw.type === 'mousedown') {
      const token = mouseToken(raw);
      const buttonDirection = raw.direction ?? 'down';
      const timestamp = clamp(raw.timestamp, 0, maxTime);

      if (buttonDirection === 'up') {
        const activeIndex = activeMouseEvents.get(token);
        if (activeIndex !== undefined && out[activeIndex]) {
          const active = out[activeIndex];
          const physicalDuration = Math.max(0, timestamp - active.startTime);
          active.endTime = clamp(Math.max(active.startTime + MIN_RELEASE_DISPLAY_SEC, timestamp), 0, maxTime);
          active.isHold = physicalDuration >= HOLD_MIN_DURATION_SEC;
          activeMouseEvents.delete(token);
        }
        continue;
      }

      const endTime = clamp(timestamp + displayDurationSec, 0, maxTime);
      if (endTime - timestamp <= 0.001) continue;
      const label = formatLabel(raw, mods);
      const candidate: KeystrokeEvent = {
        id: createEventId(),
        type: raw.type,
        startTime: timestamp,
        endTime,
        label,
        count: 1,
        isHold: false,
        modifiers: mods,
        key: raw.key,
        btn: raw.btn,
        direction: 'down',
      };
      out.push(candidate);
      activeMouseEvents.set(token, out.length - 1);
      continue;
    }

    const startTime = clamp(raw.timestamp, 0, maxTime);
    const endTime = clamp(startTime + displayDurationSec, 0, maxTime);
    if (endTime - startTime <= 0.001) continue;

    const label = formatLabel(raw, mods);
    const candidate: KeystrokeEvent = {
      id: createEventId(),
      type: raw.type,
      startTime,
      endTime,
      label,
      count: 1,
      isHold: false,
      modifiers: mods,
      key: raw.key,
      btn: raw.btn,
      direction: raw.direction,
    };

    out.push(candidate);
  }

  const durations = out.map((event) => Math.max(0, event.endTime - event.startTime));
  const min = durations.length ? Math.min(...durations) : 0;
  const max = durations.length ? Math.max(...durations) : 0;
  const p50 = computePercentile(durations, 0.5);
  const p90 = computePercentile(durations, 0.9);
  const short = out
    .filter((event) => event.endTime - event.startTime <= 0.12)
    .slice(0, 12)
    .map((event) => ({
      type: event.type,
      label: event.label,
      startTime: event.startTime,
      endTime: event.endTime,
      duration: event.endTime - event.startTime,
      hold: Boolean(event.isHold),
    }));
  console.info('[KeystrokeDebug][Processor]', {
    rawEvents: sorted.length,
    builtEvents: out.length,
    min,
    p50,
    p90,
    max,
    shortLE120ms: short.length,
    sampleShort: short,
  });

  return out;
}
