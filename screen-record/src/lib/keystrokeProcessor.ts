import { KeystrokeEvent, RawInputEvent, InputModifiers } from '@/types/video';

const DEFAULT_DISPLAY_DURATION_SEC = 1.2;

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

  for (const raw of sorted) {
    const mods = normalizeModifiers(raw.modifiers);

    if (raw.type === 'keyboard' && isModifierKey(raw.key)) continue;

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
      modifiers: mods,
      key: raw.key,
      btn: raw.btn,
      direction: raw.direction,
    };

    out.push(candidate);
  }

  return out;
}
