import type { HistoryEntry } from "./models";
import type { Copy } from "./i18n";
import { confirmDestructive } from "../../ui-shared/destructive-confirmation.ts";

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function renameSavedResult(
  invoke: Invoke,
  entry: HistoryEntry,
  requestedName: string,
): Promise<boolean> {
  const value = requestedName.trim();
  if (!value || value === entry.outputName) return true;
  try {
    await invoke("rename_history_result", { id: entry.id, newName: value });
    return true;
  } catch {
    return false;
  }
}

export async function deleteSavedResults(invoke: Invoke, id?: string): Promise<boolean> {
  try {
    await invoke(id ? "delete_history_result" : "delete_all_history_results", id ? { id } : {});
    return true;
  } catch {
    return false;
  }
}

export function confirmDeleteAll(copy: Copy): Promise<boolean> {
  return confirmDestructive({
    message: copy.deleteAllConfirm,
    confirmLabel: copy.deleteAll,
    cancelLabel: copy.dismiss,
  });
}
