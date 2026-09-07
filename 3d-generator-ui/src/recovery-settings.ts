import { generationSettings, type GenerationMode } from "./generation-mode.ts";

export type FrozenGenerationSettings = {
  generationMode: GenerationMode;
  polycount: number;
  autoSegment: boolean;
  segmentationLevel: "simple" | "balanced" | "detailed";
  topology?: "triangle" | "quad";
  instruction?: string;
  outputDir: string;
};

type FrozenSettingsSource = {
  generationMode?: GenerationMode | null;
  polycount?: number | null;
  autoSegment?: boolean | null;
  segmentationLevel?: "simple" | "balanced" | "detailed" | null;
  topology?: "triangle" | "quad" | null;
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
    || (source.segmentationLevel != null
      && !["simple", "balanced", "detailed"].includes(source.segmentationLevel))
    || typeof source.outputDir !== "string"
    || !source.outputDir.trim()
    || (source.instruction != null && typeof source.instruction !== "string")
    || (source.topology != null && source.topology !== "triangle" && source.topology !== "quad")
  ) return undefined;

  const normalized = generationSettings(
    source.generationMode,
    source.polycount,
    source.autoSegment,
    source.topology || undefined,
  );
  if (
    normalized.mode !== source.generationMode
    || normalized.polycount !== source.polycount
    || normalized.autoSegment !== source.autoSegment
    || (source.topology != null && normalized.topology !== source.topology)
  ) return undefined;

  return {
    generationMode: source.generationMode,
    polycount: source.polycount,
    autoSegment: source.autoSegment,
    segmentationLevel: source.segmentationLevel || "detailed",
    ...(source.topology ? { topology: source.topology } : {}),
    instruction: source.instruction || undefined,
    outputDir: source.outputDir,
  };
}
