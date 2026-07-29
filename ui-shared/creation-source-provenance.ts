export type CreationSourceProvenance = "surface-import" | "presentation" | "none";

export function canSubmitCreationSource(
  provenance: CreationSourceProvenance,
  sourceCount: number,
  allowEmpty: boolean,
): boolean {
  if (sourceCount === 0) return allowEmpty && provenance === "none";
  return provenance === "surface-import";
}
