export const POPULAR_ASPECT_RATIO_PRESETS = [
  { id: "landscape-16-9", label: "16:9", width: 16, height: 9 },
  { id: "portrait-9-16", label: "9:16", width: 9, height: 16 },
  { id: "square-1-1", label: "1:1", width: 1, height: 1 },
  { id: "portrait-4-5", label: "4:5", width: 4, height: 5 },
  { id: "cinema-21-9", label: "21:9", width: 21, height: 9 },
] as const;

export type AspectRatioPreset = (typeof POPULAR_ASPECT_RATIO_PRESETS)[number];

export const CROP_ASPECT_RATIO_PRESETS = [
  { id: "landscape-16-9", label: "16:9", width: 16, height: 9, orientation: "landscape" },
  { id: "landscape-16-10", label: "16:10", width: 16, height: 10, orientation: "landscape" },
  { id: "landscape-3-2", label: "3:2", width: 3, height: 2, orientation: "landscape" },
  { id: "landscape-7-5", label: "7:5", width: 7, height: 5, orientation: "landscape" },
  { id: "landscape-4-3", label: "4:3", width: 4, height: 3, orientation: "landscape" },
  { id: "landscape-5-4", label: "5:4", width: 5, height: 4, orientation: "landscape" },
  { id: "cinema-185-100", label: "1.85:1", width: 185, height: 100, orientation: "landscape" },
  { id: "landscape-2-1", label: "2:1", width: 2, height: 1, orientation: "landscape" },
  { id: "cinema-21-9", label: "21:9", width: 21, height: 9, orientation: "landscape" },
  { id: "cinema-239-100", label: "2.39:1", width: 239, height: 100, orientation: "landscape" },
  { id: "square-1-1", label: "1:1", width: 1, height: 1, orientation: "square" },
  { id: "portrait-4-5", label: "4:5", width: 4, height: 5, orientation: "portrait" },
  { id: "portrait-3-4", label: "3:4", width: 3, height: 4, orientation: "portrait" },
  { id: "portrait-5-7", label: "5:7", width: 5, height: 7, orientation: "portrait" },
  { id: "portrait-2-3", label: "2:3", width: 2, height: 3, orientation: "portrait" },
  { id: "portrait-10-16", label: "10:16", width: 10, height: 16, orientation: "portrait" },
  { id: "portrait-9-16", label: "9:16", width: 9, height: 16, orientation: "portrait" },
  { id: "portrait-1-2", label: "1:2", width: 1, height: 2, orientation: "portrait" },
  { id: "portrait-9-21", label: "9:21", width: 9, height: 21, orientation: "portrait" },
] as const;

export const CROP_ASPECT_RATIO_ORIENTATIONS = [
  "landscape",
  "square",
  "portrait",
] as const;

export const COMMON_CROP_ASPECT_RATIO_PRESET_IDS = [
  "landscape-16-9",
  "portrait-9-16",
  "landscape-4-3",
  "portrait-3-4",
  "square-1-1",
  "portrait-4-5",
] as const;

export type CropAspectRatioPreset = (typeof CROP_ASPECT_RATIO_PRESETS)[number];
export type AspectRatioPresetId = CropAspectRatioPreset["id"];
export type CropAspectRatioOrientation = CropAspectRatioPreset["orientation"];
