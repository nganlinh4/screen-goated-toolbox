export type GenerationMode = "fast" | "quality";

export type GenerationSettings = {
  mode: GenerationMode;
  polycount: number;
  minimumPolycount: number;
  maximumPolycount: number;
  autoSegment: boolean;
  showAutoSegment: boolean;
};

const LIMITS: Record<GenerationMode, { minimum: number; maximum: number }> = {
  fast: { minimum: 100, maximum: 15_000 },
  quality: { minimum: 500, maximum: 20_000 },
};

export function generationSettings(
  _mode: GenerationMode,
  polycount: number,
  requestedAutoSegment: boolean,
): GenerationSettings {
  const selectedMode = "quality";
  const limits = LIMITS[selectedMode];
  const finitePolycount = Number.isFinite(polycount) ? Math.round(polycount) : 5_000;
  return {
    mode: selectedMode,
    polycount: Math.min(limits.maximum, Math.max(limits.minimum, finitePolycount)),
    minimumPolycount: limits.minimum,
    maximumPolycount: limits.maximum,
    autoSegment: selectedMode === "quality" && requestedAutoSegment,
    showAutoSegment: selectedMode === "quality",
  };
}
