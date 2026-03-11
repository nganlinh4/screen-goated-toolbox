import type { BackgroundConfig } from "@/types/video";

export function cloneBackgroundConfig(
  backgroundConfig: BackgroundConfig,
): BackgroundConfig {
  return { ...backgroundConfig };
}

export function equalBackgroundConfig(
  left: BackgroundConfig,
  right: BackgroundConfig,
): boolean {
  const leftEntries = Object.entries(left) as Array<
    [keyof BackgroundConfig, BackgroundConfig[keyof BackgroundConfig]]
  >;
  const rightEntries = Object.entries(right) as Array<
    [keyof BackgroundConfig, BackgroundConfig[keyof BackgroundConfig]]
  >;
  if (leftEntries.length !== rightEntries.length) return false;
  return leftEntries.every(([key, value]) => Object.is(value, right[key]));
}
