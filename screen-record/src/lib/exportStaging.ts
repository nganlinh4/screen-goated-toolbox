import { invoke } from "@/lib/ipc";

export const EXPORT_FRAME_CHUNK_SIZE = 1500;

/**
 * Stage baked frame arrays to the Rust export staging store in fixed-size
 * chunks (avoids V8 JSON.stringify limits on very large frame arrays).
 *
 * `extraPayload` is spread into every chunk invocation so callers can attach
 * shape-specific fields (e.g. `sessionId`/`jobId` for the composition pipeline);
 * the Rust consumer accepts both the bare and the session-scoped payload shapes.
 */
export async function stageFramesInChunks<T>(
  frames: T[],
  dataType: string,
  extraPayload?: Record<string, unknown>,
): Promise<void> {
  for (let i = 0; i < frames.length; i += EXPORT_FRAME_CHUNK_SIZE) {
    await invoke("stage_export_data", {
      ...extraPayload,
      dataType,
      data: frames.slice(i, i + EXPORT_FRAME_CHUNK_SIZE),
    });
  }
}
