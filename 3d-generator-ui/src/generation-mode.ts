export type GenerationMode = "quality" | "fast";
export type MeshTopology = "triangle" | "quad";

export type GenerationSettings = {
  mode: GenerationMode;
  polycount: number;
  minimumPolycount: number;
  maximumPolycount: number;
  autoSegment: boolean;
  showAutoSegment: boolean;
  topology: MeshTopology;
};

const LIMITS = { minimum: 500, maximum: 20_000 };

export function generationSettings(
  mode: GenerationMode,
  polycount: number,
  requestedAutoSegment: boolean,
  topology?: MeshTopology,
): GenerationSettings {
  const finitePolycount = Number.isFinite(polycount) ? Math.round(polycount) : 5_000;
  const legacy = mode === "fast";
  const limits = legacy ? { minimum: 100, maximum: 15_000 } : LIMITS;
  const autoSegment = !legacy && requestedAutoSegment;
  return {
    mode,
    polycount: Math.min(limits.maximum, Math.max(limits.minimum, finitePolycount)),
    minimumPolycount: limits.minimum,
    maximumPolycount: limits.maximum,
    autoSegment,
    showAutoSegment: !legacy,
    topology: legacy || autoSegment ? "triangle" : topology || "quad",
  };
}
