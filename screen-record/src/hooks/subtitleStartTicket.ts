import type { ResumableSubtitleGenerationRun } from './subtitleGenerationTypes';

const KEY = 'sgt:subtitle-generation-start-v1';
const MAX_LENGTH = 1024 * 1024;

export function readSubtitleStartTicket(): ResumableSubtitleGenerationRun | null {
  const value = localStorage.getItem(KEY);
  if (value === null) return null;
  return parseTicket(value);
}

function parseTicket(value: string): ResumableSubtitleGenerationRun {
  if (value.length > MAX_LENGTH) throw new Error('Invalid subtitle start ticket');
  const run: unknown = JSON.parse(value);
  if (!run || typeof run !== 'object') throw new Error('Invalid subtitle start ticket');
  const ticket = run as ResumableSubtitleGenerationRun;
  if (!/^ticket-[0-9a-f]{32}-transcription-[1-9][0-9]*$/.test(ticket.jobId)
    || (ticket.projectKey !== null && typeof ticket.projectKey !== 'string')
    || !Number.isSafeInteger(ticket.projectEpoch)
    || !ticket.jobContext || ticket.jobContext.projectKey !== ticket.projectKey) throw new Error('Invalid subtitle start ticket');
  return ticket;
}

export function saveSubtitleStartTicket(run: ResumableSubtitleGenerationRun) {
  if (readSubtitleStartTicket()) throw new Error('A subtitle start is awaiting recovery');
  const value = JSON.stringify(run);
  if (value.length > MAX_LENGTH) throw new Error('Subtitle start ticket exceeds its storage limit');
  localStorage.setItem(KEY, value);
  if (localStorage.getItem(KEY) !== value) throw new Error('Subtitle start ticket could not be saved');
}

export function clearSubtitleStartTicket(jobId: string) {
  if (readSubtitleStartTicket()?.jobId === jobId) localStorage.removeItem(KEY);
}

/** Return only an unreadable stored value; never authorize removal of a valid ticket. */
export function readInvalidSubtitleStartTicket(): string | null {
  const value = localStorage.getItem(KEY);
  if (value === null) return null;
  try { parseTicket(value); return null; } catch { return value; }
}

export function discardInvalidSubtitleStartTicket(expected: string): boolean {
  if (localStorage.getItem(KEY) !== expected || readInvalidSubtitleStartTicket() !== expected) return false;
  localStorage.removeItem(KEY);
  return true;
}
