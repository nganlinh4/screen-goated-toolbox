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

export function isManagedCompositionSnapshotPath(
  path: string | null | undefined,
): boolean {
  if (!path) return false;
  const normalizedPath = path.replace(/\\/g, "/").toLowerCase();
  return normalizedPath.includes(
    "/screen-goated-toolbox/composition-snapshots/",
  );
}
