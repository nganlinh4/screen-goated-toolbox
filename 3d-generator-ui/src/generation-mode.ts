export type GenerationMode = "quality";
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
  return {
    mode,
    polycount: Math.min(LIMITS.maximum, Math.max(LIMITS.minimum, finitePolycount)),
    minimumPolycount: LIMITS.minimum,
    maximumPolycount: LIMITS.maximum,
    autoSegment: requestedAutoSegment,
    showAutoSegment: true,
    topology: requestedAutoSegment ? "triangle" : topology || "quad",
  };
}
