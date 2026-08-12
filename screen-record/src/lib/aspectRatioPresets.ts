export const POPULAR_ASPECT_RATIO_PRESETS = [
  { id: "landscape-16-9", label: "16:9", width: 16, height: 9 },
  { id: "portrait-9-16", label: "9:16", width: 9, height: 16 },
  { id: "square-1-1", label: "1:1", width: 1, height: 1 },
  { id: "portrait-4-5", label: "4:5", width: 4, height: 5 },
  { id: "cinema-21-9", label: "21:9", width: 21, height: 9 },
] as const;

export type AspectRatioPreset = (typeof POPULAR_ASPECT_RATIO_PRESETS)[number];
export type AspectRatioPresetId = AspectRatioPreset["id"];
