import { generationSettings } from "./generation-mode";
import type { QueueItem } from "./types";

export function normalizeGenerationSettings(item: QueueItem) {
  const settings = generationSettings(item.generationMode, item.polycount, item.autoSegment, item.topology);
  item.generationMode = settings.mode;
  item.polycount = settings.polycount;
  item.autoSegment = settings.autoSegment;
  item.topology = settings.topology;
  return settings;
}
