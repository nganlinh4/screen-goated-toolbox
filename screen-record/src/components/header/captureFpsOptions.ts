import { MonitorInfo } from "@/hooks/useAppHooks";

/** Returns exact integer divisors of Hz that are at least 30fps. */
export function getPerfectFpsOptions(hz: number): number[] {
  if (hz <= 0) return [];
  const options: number[] = [];
  for (let n = 1; n <= 4; n++) {
    const fps = hz / n;
    if (Number.isInteger(fps) && fps >= 30) options.push(fps);
  }
  return options;
}

export function getCombinedFpsOptions(monitors: MonitorInfo[]): number[] {
  const set = new Set<number>();
  for (const monitor of monitors) {
    for (const fps of getPerfectFpsOptions(monitor.hz)) {
      set.add(fps);
    }
  }
  return Array.from(set).sort((a, b) => a - b);
}
