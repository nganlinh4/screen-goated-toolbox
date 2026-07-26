interface AssetResult {
  dataUrl: string;
}

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

class LruCache {
  private readonly values = new Map<string, string>();

  constructor(private readonly capacity: number) {}

  get(key: string): string | undefined {
    const value = this.values.get(key);
    if (!value) return undefined;
    this.values.delete(key);
    this.values.set(key, value);
    return value;
  }

  set(key: string, value: string) {
    this.values.delete(key);
    this.values.set(key, value);
    while (this.values.size > this.capacity) {
      const oldest = this.values.keys().next().value;
      if (oldest === undefined) break;
      this.values.delete(oldest);
    }
  }

  retainPaths(paths: Set<string>) {
    for (const key of this.values.keys()) {
      const separator = key.indexOf(":");
      if (!paths.has(key.slice(separator + 1))) this.values.delete(key);
    }
  }
}

export class PreviewStore {
  private readonly thumbnails = new LruCache(256);
  private readonly stages = new LruCache(24);
  private readonly pending = new Map<string, Promise<string>>();

  constructor(private readonly invoke: Invoke) {}

  thumbnail(path: string): Promise<string> {
    return this.load(this.thumbnails, "thumb", path, 128);
  }

  stage(path: string, maxEdge: number): Promise<string> {
    return this.load(this.stages, "stage", path, maxEdge);
  }

  retainStagePaths(paths: string[]) {
    this.stages.retainPaths(new Set(paths));
  }

  private async load(cache: LruCache, group: string, path: string, maxEdge: number) {
    const key = `${maxEdge}:${path}`;
    const cached = cache.get(key);
    if (cached) return cached;
    const pendingKey = `${group}:${key}`;
    const current = this.pending.get(pendingKey);
    if (current) return current;
    const request = this.invoke<AssetResult>("read_image_preview", { path, maxEdge })
      .then((result) => {
        cache.set(key, result.dataUrl);
        return result.dataUrl;
      })
      .finally(() => this.pending.delete(pendingKey));
    this.pending.set(pendingKey, request);
    return request;
  }
}
