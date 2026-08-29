import type { AssetPayload, QueueItem } from "./types";

type Invoke = <T = unknown>(command: string, args?: unknown) => Promise<T>;

const MAX_CACHE_CHARACTERS = 12 * 1024 * 1024;
const PROJECT_THUMBNAIL_EDGE = 128;
const MAX_PROJECT_THUMBNAIL_URL_CHARACTERS = 16_500;

export function normalizedProjectThumbnail(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length > MAX_PROJECT_THUMBNAIL_URL_CHARACTERS) {
    return undefined;
  }
  return /^data:image\/jpeg;base64,[A-Za-z0-9+/]+={0,2}$/.test(value) ? value : undefined;
}

export class ImagePreviewCache {
  private readonly invoke: Invoke;
  private readonly values = new Map<string, AssetPayload>();
  private readonly pending = new Map<string, Promise<AssetPayload>>();
  private cachedCharacters = 0;

  constructor(invoke: Invoke) {
    this.invoke = invoke;
  }

  load(path: string, maxEdge: number): Promise<AssetPayload> {
    const key = `${path}\n${maxEdge}`;
    const cached = this.values.get(key);
    if (cached) {
      this.values.delete(key);
      this.values.set(key, cached);
      return Promise.resolve(cached);
    }
    const active = this.pending.get(key);
    if (active) return active;
    const request = this.invoke<AssetPayload>("read_image_preview", { path, maxEdge })
      .then((value) => {
        this.pending.delete(key);
        this.remember(key, value);
        return value;
      }, (error) => {
        this.pending.delete(key);
        throw error;
      });
    this.pending.set(key, request);
    return request;
  }

  ensureProjectThumbnail(item: QueueItem, onReady: () => void) {
    if (item.thumbnailUrl || !item.path) return;
    void this.load(item.path, PROJECT_THUMBNAIL_EDGE).then((preview) => {
      if (item.thumbnailUrl) return;
      item.thumbnailUrl = preview.dataUrl;
      onReady();
    }).catch(() => undefined);
  }

  clear() {
    this.values.clear();
    this.cachedCharacters = 0;
  }

  private remember(key: string, value: AssetPayload) {
    const characters = value.dataUrl.length;
    if (characters > MAX_CACHE_CHARACTERS) return;
    this.values.set(key, value);
    this.cachedCharacters += characters;
    while (this.cachedCharacters > MAX_CACHE_CHARACTERS) {
      const oldest = this.values.entries().next().value as [string, AssetPayload] | undefined;
      if (!oldest) break;
      this.values.delete(oldest[0]);
      this.cachedCharacters -= oldest[1].dataUrl.length;
    }
  }
}
