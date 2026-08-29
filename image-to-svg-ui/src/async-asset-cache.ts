type Loader = () => Promise<string>;

export class AsyncAssetCache {
  private readonly values = new Map<string, string>();
  private readonly pending = new Map<string, Promise<string>>();
  private readonly capacity: number;
  private readonly maxWeight: number;
  private weight = 0;

  constructor(capacity: number, maxWeight: number) {
    this.capacity = capacity;
    this.maxWeight = maxWeight;
  }

  get(key: string): string | undefined {
    const value = this.values.get(key);
    if (value === undefined) return undefined;
    this.values.delete(key);
    this.values.set(key, value);
    return value;
  }

  set(key: string, value: string) {
    const previous = this.values.get(key);
    if (previous !== undefined) this.weight -= previous.length;
    this.values.delete(key);
    this.values.set(key, value);
    this.weight += value.length;
    while (this.values.size > this.capacity || this.weight > this.maxWeight) {
      const oldest = this.values.keys().next().value;
      if (oldest === undefined) break;
      this.weight -= this.values.get(oldest)?.length ?? 0;
      this.values.delete(oldest);
    }
  }

  delete(key: string) {
    const previous = this.values.get(key);
    if (previous !== undefined) this.weight -= previous.length;
    this.values.delete(key);
    this.pending.delete(key);
  }

  load(key: string, loader: Loader): Promise<string> {
    const cached = this.get(key);
    if (cached !== undefined) return Promise.resolve(cached);
    const current = this.pending.get(key);
    if (current) return current;
    const request = loader()
      .then((value) => {
        this.set(key, value);
        return value;
      })
      .finally(() => this.pending.delete(key));
    this.pending.set(key, request);
    return request;
  }
}
