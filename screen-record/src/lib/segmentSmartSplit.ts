import type { TextSegment } from '@/types/video';

export interface TextChunk {
  text: string;
  unitCount: number;
}

const CJK_PATTERN = /[\u4e00-\u9fff\u3400-\u4dbf\u3040-\u309f\u30a0-\u30ff\uac00-\ud7af]/;
const STRONG_PUNCTUATION = /[.!?。！？]+["'”’)]*$/u;
const MEDIUM_PUNCTUATION = /[,;:，、；：]+["'”’)]*$/u;
const BOUNDARY_PUNCTUATION = /[.!?。！？,;:，、；：]+["'”’)]*$/u;

function countUnits(text: string) {
  const cleanText = text.trim().replace(/\s+/g, ' ');
  if (!cleanText) return 0;
  if (CJK_PATTERN.test(cleanText)) {
    return cleanText.replace(/[\s\p{P}]/gu, '').length;
  }
  return cleanText.split(/\s+/).filter(Boolean).length;
}

function splitLatinTextByChunkCount(text: string, chunkCount: number): TextChunk[] {
  const words = text.trim().split(/\s+/).filter(Boolean);
  const chunks: TextChunk[] = [];
  let cursor = 0;
  for (let index = 0; index < chunkCount; index += 1) {
    const remainingWords = words.length - cursor;
    const remainingChunks = chunkCount - index;
    const idealTake = Math.max(1, Math.ceil(remainingWords / remainingChunks));
    const take = index === chunkCount - 1
      ? remainingWords
      : chooseLatinChunkSize(words, cursor, idealTake, remainingChunks);
    const part = words.slice(cursor, cursor + take);
    cursor += take;
    chunks.push({ text: part.join(' '), unitCount: part.length });
  }
  return chunks.filter((chunk) => chunk.text.length > 0);
}

function splitLatinTextByPunctuation(text: string): TextChunk[] {
  const words = text.trim().split(/\s+/).filter(Boolean);
  const chunks: TextChunk[] = [];
  let current: string[] = [];
  for (const word of words) {
    current.push(word);
    if (BOUNDARY_PUNCTUATION.test(word)) {
      chunks.push({ text: current.join(' '), unitCount: current.length });
      current = [];
    }
  }
  if (current.length > 0) chunks.push({ text: current.join(' '), unitCount: current.length });
  return chunks.filter((chunk) => chunk.text.length > 0);
}

function splitCjkTextByPunctuation(text: string): TextChunk[] {
  const cleanText = text.replace(/\s+/g, ' ').trim();
  const chunks: TextChunk[] = [];
  let start = 0;
  for (let index = 0; index < cleanText.length; index += 1) {
    const char = cleanText[index];
    if (!char || !BOUNDARY_PUNCTUATION.test(char)) continue;
    const chunkText = cleanText.slice(start, index + 1).trim();
    if (chunkText) chunks.push({ text: chunkText, unitCount: countUnits(chunkText) });
    start = index + 1;
  }
  const tail = cleanText.slice(start).trim();
  if (tail) chunks.push({ text: tail, unitCount: countUnits(tail) });
  return chunks.filter((chunk) => chunk.text.length > 0);
}

function splitTextByPunctuation(text: string): TextChunk[] {
  return CJK_PATTERN.test(text)
    ? splitCjkTextByPunctuation(text)
    : splitLatinTextByPunctuation(text);
}

function refinePunctuationChunks(chunks: readonly TextChunk[], maxUnits: number): TextChunk[] {
  const refined: TextChunk[] = [];
  for (const chunk of chunks) {
    if (chunk.unitCount <= maxUnits) {
      refined.push(chunk);
      continue;
    }
    refined.push(...splitTextIntoChunkCount(chunk.text, Math.ceil(chunk.unitCount / maxUnits)));
  }
  return refined;
}

function isUsefulPunctuationSplit(chunks: readonly TextChunk[], totalUnits: number, maxUnits: number) {
  if (chunks.length < 2) return false;
  const tinyThreshold = Math.max(2, Math.floor(maxUnits * 0.25));
  const hugeThreshold = Math.max(maxUnits * 2.75, totalUnits * 0.75);
  const tinyChunks = chunks.filter((chunk) => chunk.unitCount < tinyThreshold).length;
  const hasHugeChunk = chunks.some((chunk) => chunk.unitCount > hugeThreshold);
  return tinyChunks <= Math.max(1, Math.floor(chunks.length * 0.35)) && !hasHugeChunk;
}

function punctuationWeight(text: string) {
  if (STRONG_PUNCTUATION.test(text)) return 12;
  if (MEDIUM_PUNCTUATION.test(text)) return 7;
  return 0;
}

function chooseLatinChunkSize(
  words: readonly string[],
  cursor: number,
  idealTake: number,
  remainingChunks: number,
) {
  const remainingWords = words.length - cursor;
  if (remainingChunks <= 1) return remainingWords;

  const minTake = Math.max(1, Math.floor(idealTake * 0.55));
  const maxTake = Math.min(
    remainingWords - (remainingChunks - 1),
    Math.max(minTake, Math.ceil(idealTake * 1.45)),
  );
  let bestTake = Math.min(Math.max(idealTake, minTake), maxTake);
  let bestScore = Number.POSITIVE_INFINITY;

  for (let take = minTake; take <= maxTake; take += 1) {
    const boundaryWord = words[cursor + take - 1] ?? '';
    const nextWord = words[cursor + take] ?? '';
    const distance = Math.abs(take - idealTake);
    const noPunctuationPenalty = punctuationWeight(boundaryWord) > 0 ? 0 : 2.5;
    const startsNewClauseBonus = /^[(["'“‘]?[A-ZÀ-Ỵ]/u.test(nextWord) ? 0.6 : 0;
    const score = distance * 2.2 + noPunctuationPenalty - punctuationWeight(boundaryWord) - startsNewClauseBonus;
    if (score < bestScore || (score === bestScore && Math.abs(take - idealTake) < Math.abs(bestTake - idealTake))) {
      bestScore = score;
      bestTake = take;
    }
  }

  return bestTake;
}

function splitCjkTextByChunkCount(text: string, chunkCount: number): TextChunk[] {
  const cleanText = text.replace(/\s+/g, ' ').trim();
  const totalUnits = countUnits(cleanText);
  const chunks: TextChunk[] = [];
  let cursor = 0;
  let currentUnitOffset = 0;

  for (let index = 0; index < chunkCount; index += 1) {
    const remainingUnits = totalUnits - currentUnitOffset;
    const remainingChunks = chunkCount - index;
    const targetUnits = Math.max(1, Math.ceil(remainingUnits / remainingChunks));
    let unitCount = 0;
    let end = cursor;

    while (end < cleanText.length && unitCount < targetUnits) {
      const char = cleanText[end];
      end += 1;
      if (char && !/[\s\p{P}]/u.test(char)) unitCount += 1;
    }

    if (index < chunkCount - 1) {
      end = chooseCjkBoundary(cleanText, cursor, end, totalUnits, currentUnitOffset, remainingChunks);
    } else {
      end = cleanText.length;
    }

    const chunkText = cleanText.slice(cursor, end).trim();
    if (chunkText) {
      const chunkUnits = countUnits(chunkText);
      chunks.push({ text: chunkText, unitCount: chunkUnits });
      currentUnitOffset += chunkUnits;
    }
    cursor = end;
  }

  return chunks.length > 0 ? chunks : [{ text, unitCount: totalUnits }];
}

function countCjkUnitsBetween(text: string, start: number, end: number) {
  return countUnits(text.slice(start, end));
}

function chooseCjkBoundary(
  text: string,
  cursor: number,
  idealEnd: number,
  totalUnits: number,
  consumedUnits: number,
  remainingChunks: number,
) {
  if (remainingChunks <= 1) return text.length;
  const remainingUnits = totalUnits - consumedUnits;
  const idealUnits = Math.max(1, Math.ceil(remainingUnits / remainingChunks));
  const minUnits = Math.max(1, Math.floor(idealUnits * 0.55));
  const maxUnits = Math.max(minUnits, Math.ceil(idealUnits * 1.45));
  const searchStart = Math.max(cursor + 1, idealEnd - 14);
  const searchEnd = Math.min(text.length, idealEnd + 18);
  let bestEnd = idealEnd;
  let bestScore = Number.POSITIVE_INFINITY;

  for (let end = searchStart; end <= searchEnd; end += 1) {
    const units = countCjkUnitsBetween(text, cursor, end);
    const remainingAfter = totalUnits - consumedUnits - units;
    if (units < minUnits || units > maxUnits || remainingAfter < remainingChunks - 1) continue;
    const boundaryText = text.slice(Math.max(cursor, end - 2), end);
    const distance = Math.abs(units - idealUnits);
    const score = distance * 2.2 - punctuationWeight(boundaryText) + (/[\s]/u.test(text[end - 1] ?? '') ? -1 : 0);
    if (score < bestScore || (score === bestScore && Math.abs(end - idealEnd) < Math.abs(bestEnd - idealEnd))) {
      bestScore = score;
      bestEnd = end;
    }
  }

  return bestEnd;
}

export function smartSplitText(text: string, maxUnits: number): TextChunk[] {
  const totalUnits = countUnits(text);
  const safeMaxUnits = Math.max(1, maxUnits);
  if (totalUnits <= safeMaxUnits) return [{ text, unitCount: totalUnits }];
  const punctuationChunks = splitTextByPunctuation(text);
  if (isUsefulPunctuationSplit(punctuationChunks, totalUnits, safeMaxUnits)) {
    return refinePunctuationChunks(punctuationChunks, safeMaxUnits);
  }
  return splitTextIntoChunkCount(text, Math.ceil(totalUnits / safeMaxUnits));
}

export function splitTextIntoChunkCount(text: string, chunkCount: number): TextChunk[] {
  const totalUnits = countUnits(text);
  const safeChunkCount = Math.max(1, Math.min(Math.floor(chunkCount), totalUnits || 1));
  if (safeChunkCount <= 1) return [{ text, unitCount: totalUnits }];
  return CJK_PATTERN.test(text)
    ? splitCjkTextByChunkCount(text, safeChunkCount)
    : splitLatinTextByChunkCount(text, safeChunkCount);
}

export function splitTimingByChunks(
  startTime: number,
  endTime: number,
  chunks: readonly TextChunk[],
) {
  if (chunks.length <= 1) return [{ startTime, endTime }];
  const totalUnits = chunks.reduce((sum, chunk) => sum + Math.max(1, chunk.unitCount), 0);
  const duration = Math.max(0, endTime - startTime);
  let cursor = startTime;
  return chunks.map((chunk, index) => {
    const nextEnd = index === chunks.length - 1
      ? endTime
      : cursor + duration * (Math.max(1, chunk.unitCount) / totalUnits);
    const timing = { startTime: cursor, endTime: nextEnd };
    cursor = nextEnd;
    return timing;
  });
}

export function splitTextSegmentByMaxUnits<T extends TextSegment>(
  segment: T,
  maxUnits: number,
  createId: () => string,
): T[] {
  const chunks = smartSplitText(segment.text, maxUnits);
  if (chunks.length <= 1) return [segment];
  const timings = splitTimingByChunks(segment.startTime, segment.endTime, chunks);
  return chunks.map((chunk, index) => ({
    ...segment,
    id: index === 0 ? segment.id : createId(),
    text: chunk.text,
    startTime: timings[index]?.startTime ?? segment.startTime,
    endTime: timings[index]?.endTime ?? segment.endTime,
  }));
}
