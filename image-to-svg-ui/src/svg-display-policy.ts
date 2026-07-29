import type { Stage } from "./types";

export type SvgSurfaceIntent = "preview" | "edit";

export function shouldConstructEditableSurface(
  intent: SvgSurfaceIntent,
  stage: Stage,
  outputPath: string | undefined,
): boolean {
  return intent === "edit" && stage === "done" && Boolean(outputPath);
}
