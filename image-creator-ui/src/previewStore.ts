interface AssetResult {
  url: string;
}

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

class LruCache {
  private readonly values = new Map<string, string>();
  private readonly capacity: number;
  private readonly maxBytes: number;
  private bytes = 0;

  constructor(capacity: number, maxBytes: number) {
    this.capacity = capacity;
    this.maxBytes = maxBytes;
  }

  get(key: string): string | undefined {
    const value = this.values.get(key);
    if (!value) return undefined;
    this.values.delete(key);
    this.values.set(key, value);
    return value;
  }

  set(key: string, value: string) {
    const previous = this.values.get(key);
    if (previous) this.bytes -= this.weight(previous);
    this.values.delete(key);
    this.values.set(key, value);
    this.bytes += this.weight(value);
    while (this.values.size > this.capacity || this.bytes > this.maxBytes) {
      const oldest = this.values.keys().next().value;
      if (oldest === undefined) break;
      this.bytes -= this.weight(this.values.get(oldest) || "");
      this.values.delete(oldest);
    }
  }

  private weight(value: string) {
    return value.length;
  }
}

export class PreviewStore {
  private readonly stages = new LruCache(24, 24 * 1024 * 1024);
  private readonly pending = new Map<string, Promise<string>>();
  private readonly invoke: Invoke;

  constructor(invoke: Invoke) {
    this.invoke = invoke;
  }

  stage(path: string, maxEdge: number): Promise<string> {
    return this.load(this.stages, path, maxEdge);
  }

  private async load(cache: LruCache, path: string, maxEdge: number) {
    const key = `${maxEdge}:${path}`;
    const cached = cache.get(key);
    if (cached) return cached;
    const pendingKey = key;
    const current = this.pending.get(pendingKey);
    if (current) return current;
    const request = this.invoke<AssetResult>("image_asset_url", { path })
      .then((result) => {
        cache.set(key, result.url);
        return result.url;
      })
      .finally(() => this.pending.delete(pendingKey));
    this.pending.set(pendingKey, request);
    return request;
  }
}
