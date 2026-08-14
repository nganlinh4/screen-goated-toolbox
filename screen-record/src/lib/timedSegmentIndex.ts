export interface TimedSegment {
  startTime: number;
  endTime: number;
}

interface IndexedTimedSegment<T extends TimedSegment> {
  segment: T;
  originalIndex: number;
}

interface TimedSegmentIndex<T extends TimedSegment> {
  sorted: Array<IndexedTimedSegment<T>>;
  prefixMaxEnd: number[];
}

const indexCache = new WeakMap<
  readonly TimedSegment[],
  TimedSegmentIndex<TimedSegment>
>();

function buildIndex<T extends TimedSegment>(
  segments: readonly T[],
): TimedSegmentIndex<T> {
  const sorted = segments
    .map((segment, originalIndex) => ({ segment, originalIndex }))
    .sort((left, right) => left.segment.startTime - right.segment.startTime);
  const prefixMaxEnd: number[] = [];
  let maxEnd = Number.NEGATIVE_INFINITY;
  for (const entry of sorted) {
    maxEnd = Math.max(maxEnd, entry.segment.endTime);
    prefixMaxEnd.push(maxEnd);
  }
  return { sorted, prefixMaxEnd };
}

function getIndex<T extends TimedSegment>(
  segments: readonly T[],
): TimedSegmentIndex<T> {
  const cached = indexCache.get(segments) as TimedSegmentIndex<T> | undefined;
  if (cached) return cached;
  const index = buildIndex(segments);
  indexCache.set(segments, index as TimedSegmentIndex<TimedSegment>);
  return index;
}

export function getActiveTimedSegments<T extends TimedSegment>(
  segments: readonly T[] | null | undefined,
  time: number,
): T[] {
  if (!segments?.length || !Number.isFinite(time)) return [];
  const { sorted, prefixMaxEnd } = getIndex(segments);
  let low = 0;
  let high = sorted.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (sorted[middle].segment.startTime <= time) low = middle + 1;
    else high = middle;
  }

  const active: Array<IndexedTimedSegment<T>> = [];
  for (let index = low - 1; index >= 0; index -= 1) {
    if (prefixMaxEnd[index] < time) break;
    const entry = sorted[index];
    if (entry.segment.endTime >= time) active.push(entry);
  }
  active.sort((left, right) => left.originalIndex - right.originalIndex);
  return active.map((entry) => entry.segment);
}
