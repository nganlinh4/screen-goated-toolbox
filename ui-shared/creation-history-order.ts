export function newestSessionsFirst<T extends {
  createdAtMs?: number;
}>(items: readonly T[]): T[] {
  return items
    .map((item, index) => ({ item, index }))
    .sort((left, right) =>
      (right.item.createdAtMs || 0) - (left.item.createdAtMs || 0)
      || left.index - right.index)
    .map(({ item }) => item);
}
