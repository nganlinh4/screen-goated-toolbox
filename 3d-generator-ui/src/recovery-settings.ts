import { generationSettings, type GenerationMode } from "./generation-mode.ts";

export type FrozenGenerationSettings = {
  generationMode: GenerationMode;
  polycount: number;
  autoSegment: boolean;
  instruction?: string;
  outputDir: string;
};

type FrozenSettingsSource = {
  generationMode?: GenerationMode | null;
  polycount?: number | null;
  autoSegment?: boolean | null;
  instruction?: string | null;
  outputDir?: string | null;
};

export function frozenGenerationSettings(
  source: FrozenSettingsSource,
): FrozenGenerationSettings | undefined {
  if (
    (source.generationMode !== "fast" && source.generationMode !== "quality")
    || typeof source.polycount !== "number"
    || !Number.isInteger(source.polycount)
    || typeof source.autoSegment !== "boolean"
    || typeof source.outputDir !== "string"
    || !source.outputDir.trim()
    || (source.instruction != null && typeof source.instruction !== "string")
  ) return undefined;

  const normalized = generationSettings(
    source.generationMode,
    source.polycount,
    source.autoSegment,
  );
  if (
    normalized.mode !== source.generationMode
    || normalized.polycount !== source.polycount
    || normalized.autoSegment !== source.autoSegment
  ) return undefined;

  return {
    generationMode: source.generationMode,
    polycount: source.polycount,
    autoSegment: source.autoSegment,
    instruction: source.instruction || undefined,
    outputDir: source.outputDir,
  };
}
