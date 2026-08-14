import { invoke } from "@/lib/ipc";
import type { BakedOverlayPayload } from "@/types/video";
import { throwIfExportCancelled } from "@/lib/exportCancellation";

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
  frames: readonly T[],
  dataType: string,
  extraPayload?: Record<string, unknown>,
  isCancelled?: () => boolean,
): Promise<void> {
  for (let i = 0; i < frames.length; i += EXPORT_FRAME_CHUNK_SIZE) {
    throwIfExportCancelled(isCancelled);
    await invoke("stage_export_data", {
      ...extraPayload,
      dataType,
      data: frames.slice(i, i + EXPORT_FRAME_CHUNK_SIZE),
    });
  }
}

export async function stageFrameIterableInChunks<T>(
  frames: Iterable<T>,
  dataType: string,
  extraPayload?: Record<string, unknown>,
  isCancelled?: () => boolean,
): Promise<void> {
  let chunk: T[] = [];
  for (const frame of frames) {
    chunk.push(frame);
    if (chunk.length < EXPORT_FRAME_CHUNK_SIZE) continue;
    throwIfExportCancelled(isCancelled);
    await invoke("stage_export_data", {
      ...extraPayload,
      dataType,
      data: chunk,
    });
    chunk = [];
  }
  if (chunk.length > 0) {
    throwIfExportCancelled(isCancelled);
    await invoke("stage_export_data", {
      ...extraPayload,
      dataType,
      data: chunk,
    });
  }
}

export async function stageOverlayPayload(
  payload: BakedOverlayPayload | null | undefined,
  extraPayload?: Record<string, unknown>,
  isCancelled?: () => boolean,
): Promise<void> {
  if (!payload) return;
  if (payload.atlasBase64) {
    throwIfExportCancelled(isCancelled);
    await invoke("stage_export_data", {
      ...extraPayload,
      dataType: "atlas",
      base64: payload.atlasBase64,
      width: payload.atlasWidth,
      height: payload.atlasHeight,
    });
  }
  if (payload.atlasMetadata) {
    throwIfExportCancelled(isCancelled);
    await invoke("stage_export_data", {
      ...extraPayload,
      dataType: "overlay_atlas_metadata",
      data: payload.atlasMetadata,
    });
    return;
  }
  await stageFramesInChunks(
    payload.frames,
    "overlay_frames_chunk",
    extraPayload,
    isCancelled,
  );
}
