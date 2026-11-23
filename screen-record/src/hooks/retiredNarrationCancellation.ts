import { invoke } from '@/lib/ipc';

const storagePrefix = 'sgt:retired-narration:';
const commands = new Set(['cancel_subtitle_narration', 'cancel_s2s_narration', 'cancel_gemini_translate_narration']);
type Cancellation = { command: string; jobId: string; attempts: number };
const pending = new Map<string, Cancellation>();

function keyFor(command: string, jobId: string): string {
  return storagePrefix + encodeURIComponent(JSON.stringify([command, jobId]));
}

function start(entry: Cancellation): void {
  const key = keyFor(entry.command, entry.jobId);
  if (pending.has(key)) return;
  pending.set(key, entry);
  const attempt = () => {
    void invoke<{ outcome: string } | null>(entry.command, { jobId: entry.jobId }).then((result) => {
      // null is the legacy success response; newer workers distinguish missing and terminal jobs.
      if (result != null && !['cancelled', 'terminal', 'missing'].includes(result.outcome)) {
        throw new Error('Unexpected narration cancellation response');
      }
      if (pending.get(key) !== entry) return;
      pending.delete(key);
      try { localStorage.removeItem(key); } catch { /* Retry on the next startup if removal failed. */ }
    }).catch(() => {
      if (pending.get(key) !== entry) return;
      const delay = Math.min(30_000, 250 * 2 ** Math.min(entry.attempts++, 7));
      window.setTimeout(attempt, delay);
    });
  };
  attempt();
}

/** Replay cancellation intents even when the previous WebView disappeared before acknowledgement. */
export function resumeRetiredNarrationCancellations(): void {
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (!key?.startsWith(storagePrefix)) continue;
      let value: unknown;
      try { value = JSON.parse(localStorage.getItem(key) ?? 'null'); } catch { continue; }
      if (!Array.isArray(value) || value.length !== 2) continue;
      const [command, jobId]: unknown[] = value;
      if (typeof command !== 'string' || !commands.has(command) || typeof jobId !== 'string' || !jobId || key !== keyFor(command, jobId)) continue;
      start({ command, jobId, attempts: 0 });
    }
  } catch {
    // Storage can be unavailable; live cancellation still retries in memory.
  }
}

export function retireNarrationJob(command: string, jobId: string): void {
  if (!commands.has(command) || !jobId) return;
  const key = keyFor(command, jobId);
  try { localStorage.setItem(key, JSON.stringify([command, jobId])); } catch { /* Keep retrying while this WebView lives. */ }
  start({ command, jobId, attempts: 0 });
}

/** Record a start before IPC so a reloaded WebView can retire it even without a reply. */
export async function recordNarrationStart(command: string): Promise<string> {
  const kind = command === 'cancel_subtitle_narration' ? 'subtitle'
    : command === 'cancel_s2s_narration' ? 's2s'
      : command === 'cancel_gemini_translate_narration' ? 'gemini-translate' : null;
  if (!kind) throw new Error('Unknown narration cancellation command');
  const { jobId } = await invoke<{ jobId: string }>('allocate_narration_job_id', { kind });
  if (typeof jobId !== 'string' || !jobId) throw new Error('Invalid narration job identity');
  localStorage.setItem(keyFor(command, jobId), JSON.stringify([command, jobId]));
  return jobId;
}

/** Preserve the actual identity if an older worker assigns its own ID. */
export function recordNarrationResponse(command: string, requested: string, actual: string): void {
  if (requested === actual) return;
  try { localStorage.setItem(keyFor(command, actual), JSON.stringify([command, actual])); } catch (error) {
    retireNarrationJob(command, actual);
    throw error;
  }
  acknowledgeNarrationStart(command, requested);
}

export function acknowledgeNarrationStart(command: string, jobId: string): void {
  try { localStorage.removeItem(keyFor(command, jobId)); } catch { /* A later replay safely cancels this ID. */ }
}
