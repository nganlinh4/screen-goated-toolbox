export function getLruCacheValue<K, V>(
  cache: Map<K, V>,
  key: K,
): V | undefined {
  const value = cache.get(key);
  if (value === undefined) return undefined;
  cache.delete(key);
  cache.set(key, value);
  return value;
}

export function setLruCacheValue<K, V>(
  cache: Map<K, V>,
  key: K,
  value: V,
  maxEntries: number,
): void {
  cache.delete(key);
  cache.set(key, value);
  while (cache.size > Math.max(1, maxEntries)) {
    const oldestKey = cache.keys().next().value as K | undefined;
    if (oldestKey === undefined) break;
    cache.delete(oldestKey);
  }
}
