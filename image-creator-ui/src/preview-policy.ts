export const IMAGE_QUEUE_ROWS_DECODE_ARTWORK = false;
export const IMAGE_REFERENCE_LIST_DECODES_ARTWORK = false;

export function selectedImagePreviewPaths(references: string[], output?: string): string[] {
  if (!output) return references.slice(0, 1);
  if (references.length === 1) return [references[0], output];
  return [output];
}
