import type { Item } from "./types";

/**
 * Drops a deleted history item and picks its neighbour when it was selected.
 * A history refresh can replace the list while the delete is in flight, so the
 * item may already be gone; then nothing else may be removed.
 */
export function removeDeletedHistoryItem(
  items: Item[],
  deleted: Item,
  selectedId: string,
): { selectedId: string; selectionChanged: boolean } {
  const index = items.findIndex((value) => value.id === deleted.id);
  if (index < 0) return { selectedId, selectionChanged: false };
  items.splice(index, 1);
  if (selectedId !== deleted.id) return { selectedId, selectionChanged: false };
  return {
    selectedId: items[Math.min(index, items.length - 1)]?.id || "",
    selectionChanged: true,
  };
}

/**
 * Applies a history entry's current output location to its item. A preview URL
 * is issued for one file path, so it is dropped when a rename moved the file.
 */
export function applyHistoryOutput(
  item: Item,
  entry: { outputPath: string; outputName: string },
): void {
  if (item.outputPath !== entry.outputPath) item.svgPreviewUrl = undefined;
  item.outputPath = entry.outputPath;
  item.outputName = entry.outputName;
}
