import { invoke } from "@/lib/ipc";

interface MediaServerInfo {
  port: number;
  token: string;
}

let mediaServerInfoPromise: Promise<MediaServerInfo> | null = null;

export function isNativeMediaUrl(url: string | null | undefined): boolean {
  if (!url) return false;
  return /^https?:\/\/(127\.0\.0\.1|localhost):\d+\/\?path=/.test(url);
}

/**
 * Resolves the local media-server port plus the per-process secret gate token.
 * The token is delivered over the secure custom-IPC bridge (never HTTP) and is
 * required on every server request: as an `X-SGT-Token` header on POST/fetch and
 * as a `&token=` query param on GET media URLs (since <video>/<audio> `src`
 * cannot send headers).
 */
export async function getMediaServerInfo(): Promise<MediaServerInfo> {
  if (!mediaServerInfoPromise) {
    const pending = invoke<MediaServerInfo>("get_media_server_port");
    mediaServerInfoPromise = pending;
    // A transient IPC failure must not be cached: the next call retries.
    pending.catch(() => {
      if (mediaServerInfoPromise === pending) mediaServerInfoPromise = null;
    });
  }
  const info = await mediaServerInfoPromise;
  if (!info || !info.port) {
    // Reset so a later call can retry instead of caching the failure.
    mediaServerInfoPromise = null;
    throw new Error("Media server unavailable");
  }
  return info;
}

/** Header object carrying the gate token for POST/fetch calls. */
async function mediaServerAuthHeaders(): Promise<Record<string, string>> {
  const { token } = await getMediaServerInfo();
  return token ? { "X-SGT-Token": token } : {};
}

export async function getMediaServerUrl(path: string): Promise<string> {
  const trimmedPath = path.trim();
  if (!trimmedPath) {
    throw new Error("Media path is empty");
  }
  const { port, token } = await getMediaServerInfo();
  const base = `http://127.0.0.1:${port}/?path=${encodeURIComponent(trimmedPath)}`;
  return token ? `${base}&token=${encodeURIComponent(token)}` : base;
}

export async function writeBlobToTempMediaFile(blob: Blob): Promise<string> {
  const { port } = await getMediaServerInfo();
  const response = await fetch(`http://127.0.0.1:${port}/write-temp`, {
    method: "POST",
    headers: await mediaServerAuthHeaders(),
    body: blob,
  });
  if (!response.ok) {
    throw new Error(`Failed to write temp media file (${response.status})`);
  }
  const data = (await response.json()) as { path?: string };
  if (!data.path) {
    throw new Error("Temp media file path missing");
  }
  return data.path;
}

export async function importVideoToManagedMediaFile(
  blob: Blob,
  fileName?: string,
  traceId?: string,
): Promise<{ path: string; hasAudio: boolean }> {
  const { port } = await getMediaServerInfo();
  const params = new URLSearchParams();
  if (fileName) {
    params.set("filename", fileName);
  }
  if (traceId) {
    params.set("traceId", traceId);
  }
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  const response = await fetch(`http://127.0.0.1:${port}/import-video${suffix}`, {
    method: "POST",
    headers: await mediaServerAuthHeaders(),
    body: blob,
  });
  if (!response.ok) {
    const message = await response.text().catch(() => "");
    throw new Error(message || `Failed to import video (${response.status})`);
  }
  const data = (await response.json()) as { path?: string; hasAudio?: boolean };
  if (!data.path) {
    throw new Error("Imported video path missing");
  }
  return { path: data.path, hasAudio: data.hasAudio !== false };
}

export async function importVideoPathToManagedMediaFile(
  path: string,
  traceId?: string,
): Promise<{ path: string; hasAudio: boolean }> {
  const data = await invoke<{ path?: string; hasAudio?: boolean }>("import_video_path", {
    path,
    traceId,
  });
  if (!data.path) {
    throw new Error("Imported video path missing");
  }
  return { path: data.path, hasAudio: data.hasAudio !== false };
}

export async function importAudioToManagedMediaFile(
  blob: Blob,
  fileName?: string,
  traceId?: string,
): Promise<{ path: string; duration: number }> {
  const { port } = await getMediaServerInfo();
  const params = new URLSearchParams();
  if (fileName) {
    params.set("filename", fileName);
  }
  if (traceId) {
    params.set("traceId", traceId);
  }
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  const response = await fetch(`http://127.0.0.1:${port}/import-audio${suffix}`, {
    method: "POST",
    headers: await mediaServerAuthHeaders(),
    body: blob,
  });
  if (!response.ok) {
    const message = await response.text().catch(() => "");
    throw new Error(message || `Failed to import audio (${response.status})`);
  }
  const data = (await response.json()) as { path?: string; duration?: number };
  if (!data.path) {
    throw new Error("Imported audio path missing");
  }
  return { path: data.path, duration: data.duration ?? 0 };
}

export async function importAudioPathToManagedMediaFile(
  path: string,
  traceId?: string,
): Promise<{ path: string; duration: number }> {
  const data = await invoke<{ path?: string; duration?: number }>("import_audio_path", {
    path,
    traceId,
  });
  if (!data.path) {
    throw new Error("Imported audio path missing");
  }
  return { path: data.path, duration: data.duration ?? 0 };
}

export async function createAudioPlaceholderVideo(
  duration: number,
  traceId?: string,
): Promise<{ path: string }> {
  const data = await invoke<{ path?: string }>("create_audio_placeholder_video", {
    duration,
    traceId,
  });
  if (!data.path) {
    throw new Error("Audio placeholder video path missing");
  }
  return { path: data.path };
}

export function isManagedImportedVideoPath(
  path: string | null | undefined,
): boolean {
  if (!path) return false;
  const normalizedPath = path.replace(/\\/g, "/").toLowerCase();
  return (
    normalizedPath.includes("/screen-goated-toolbox/recordings/imported-") ||
    normalizedPath.includes(
      "/screen-goated-toolbox/recordings/imported-audio-placeholder-",
    )
  );
}

export function isManagedCompositionSnapshotPath(
  path: string | null | undefined,
): boolean {
  if (!path) return false;
  const normalizedPath = path.replace(/\\/g, "/").toLowerCase();
  return normalizedPath.includes(
    "/screen-goated-toolbox/composition-snapshots/",
  );
}

/** Every per-clip media path a composition snapshot copies and later owns. */
export const SNAPSHOT_CLIP_MEDIA_PATH_KEYS = [
  "rawVideoPath",
  "rawMicAudioPath",
  "rawWebcamVideoPath",
] as const;

export type SnapshotClipMediaPaths = Partial<
  Record<(typeof SNAPSHOT_CLIP_MEDIA_PATH_KEYS)[number], string>
>;

/** Managed snapshot copies a clip owns, excluding paths the project root still uses. */
export function ownedSnapshotClipMediaPaths(
  clip: SnapshotClipMediaPaths,
  rootPaths: ReadonlyArray<string | null | undefined>,
): string[] {
  const roots = new Set(rootPaths.filter(Boolean));
  return SNAPSHOT_CLIP_MEDIA_PATH_KEYS.map((key) => clip[key]).filter(
    (path): path is string =>
      Boolean(path) && !roots.has(path) && isManagedCompositionSnapshotPath(path),
  );
}
