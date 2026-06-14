import type { Translations } from '@/i18n';

interface MonitorLike {
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  hz: number;
  is_primary: boolean;
}

function applyTemplate(
  template: string,
  values: Record<string, string | number>
): string {
  return Object.entries(values).reduce(
    (result, [key, value]) => result.split(`{${key}}`).join(String(value)),
    template
  );
}

export function formatMonitorName(
  index: number,
  isPrimary: boolean,
  t: Pick<Translations, 'monitorDisplayName' | 'monitorDisplayNamePrimary'>
): string {
  return applyTemplate(
    isPrimary ? t.monitorDisplayNamePrimary : t.monitorDisplayName,
    { index: index + 1 }
  );
}

export function formatMonitorSummary(
  monitor: Pick<MonitorLike, 'width' | 'height' | 'hz' | 'is_primary'>,
  t: Pick<Translations, 'monitorPrimaryShort'>
): string {
  const base = `${monitor.width}×${monitor.height} · ${monitor.hz}Hz`;
  return monitor.is_primary ? `${base} ${t.monitorPrimaryShort}` : base;
}

export function formatMonitorDialogSummary(
  monitor: Pick<MonitorLike, 'width' | 'height' | 'hz' | 'x' | 'y'>
): string {
  return `${monitor.width}×${monitor.height} · ${monitor.hz}Hz · ${monitor.x}, ${monitor.y}`;
}

export const sortMonitorsByPosition = <T extends MonitorLike>(
  monitors: T[],
  t: Pick<Translations, 'monitorDisplayName' | 'monitorDisplayNamePrimary'>
) => {
  return [...monitors]
    .sort((a, b) => a.x - b.x)
    .map((monitor, index) => ({
      ...monitor,
      name: formatMonitorName(index, monitor.is_primary, t)
    }));
};

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}
