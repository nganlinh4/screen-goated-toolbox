import { invoke } from "@/lib/ipc";

let mediaServerPortPromise: Promise<number> | null = null;

export function isNativeMediaUrl(url: string | null | undefined): boolean {
  if (!url) return false;
  return /^https?:\/\/(127\.0\.0\.1|localhost):\d+\/\?path=/.test(url);
}

export async function getMediaServerPort(): Promise<number> {
  if (!mediaServerPortPromise) {
    mediaServerPortPromise = invoke<number>("get_media_server_port");
  }
  const port = await mediaServerPortPromise;
  if (!port) {
    throw new Error("Media server unavailable");
  }
  return port;
}

export async function getMediaServerUrl(path: string): Promise<string> {
  const trimmedPath = path.trim();
  if (!trimmedPath) {
    throw new Error("Media path is empty");
  }
  const port = await getMediaServerPort();
  return `http://127.0.0.1:${port}/?path=${encodeURIComponent(trimmedPath)}`;
}

export async function writeBlobToTempMediaFile(blob: Blob): Promise<string> {
  const port = await getMediaServerPort();
  const response = await fetch(`http://127.0.0.1:${port}/write-temp`, {
    method: "POST",
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
  const port = await getMediaServerPort();
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

export function isManagedImportedVideoPath(
  path: string | null | undefined,
): boolean {
  if (!path) return false;
  const normalizedPath = path.replace(/\\/g, "/").toLowerCase();
  return normalizedPath.includes("/screen-goated-toolbox/recordings/imported-");
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
