import { projectManager } from "@/lib/projectManager";
import { Project } from "@/types/video";
import { writeBlobToTempMediaFile } from "@/lib/mediaServer";

/**
 * Restore a raw media path from a stored blob by writing it back to disk via
 * the media server, then persisting the updated project record so this
 * migration only happens once.
 *
 * Unifies the near-duplicate blob -> disk -> updateProject blocks in the
 * project load path (mic audio, webcam video). Returns the restored path, or
 * the empty string if nothing was written (or on error).
 *
 * @param blob       The stored media blob to restore.
 * @param errorLabel Human-readable label used in the failure log message.
 * @param projectId  The project being migrated.
 * @param project    The full loaded project record (spread into the update).
 * @param buildPatch Builds the updateProject patch from the restored path.
 *                   This lets each caller accumulate previously restored
 *                   paths (e.g. webcam includes rawVideoPath + rawMicAudioPath).
 */
export async function restoreRawPath(
  blob: Blob,
  errorLabel: string,
  projectId: string,
  project: Project,
  buildPatch: (
    restoredPath: string,
  ) => Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
): Promise<string> {
  try {
    const restoredPath = await writeBlobToTempMediaFile(blob);
    if (restoredPath) {
      await projectManager.updateProject(projectId, {
        ...project,
        ...buildPatch(restoredPath),
      });
    }
    return restoredPath;
  } catch (e) {
    console.error(`[ProjectLoad] Failed to restore ${errorLabel}:`, e);
    return "";
  }
}
