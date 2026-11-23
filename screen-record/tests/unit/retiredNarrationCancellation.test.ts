import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
  vi.mocked(invoke).mockReset();
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); localStorage.clear(); });

describe('retired narration cancellation', () => {
  it('preserves a legacy worker response without cancelling the unused client ID', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ jobId: 'ticket-test-s2s-1' });
    const { recordNarrationStart, recordNarrationResponse } = await import('@/hooks/retiredNarrationCancellation');
    const requested = await recordNarrationStart('cancel_s2s_narration');
    recordNarrationResponse('cancel_s2s_narration', requested, 'legacy-job');
    expect(localStorage.length).toBe(1);
    expect([...Array(localStorage.length).keys()].map((i) => localStorage.getItem(localStorage.key(i)!))).toEqual([
      JSON.stringify(['cancel_s2s_narration', 'legacy-job']),
    ]);
    expect(invoke).not.toHaveBeenCalledWith('cancel_s2s_narration', expect.anything());
  });
  it('reconciles a persisted start after reload before its IPC reply', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ jobId: 'ticket-test-s2s-1' });
    const first = await import('@/hooks/retiredNarrationCancellation');
    const id = await first.recordNarrationStart('cancel_s2s_narration');
    expect(id).toBe('ticket-test-s2s-1');
    expect(localStorage.length).toBe(1);
    vi.mocked(invoke).mockClear();
    vi.resetModules();
    vi.mocked(invoke).mockResolvedValue({ outcome: 'missing' });
    const reloaded = await import('@/hooks/retiredNarrationCancellation');
    reloaded.resumeRetiredNarrationCancellations();
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledWith('cancel_s2s_narration', { jobId: id });
    expect(localStorage.length).toBe(0);
  });

  it('fails a start when ownership cannot be persisted', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ jobId: 'ticket-test-subtitle-1' });
    const { recordNarrationStart } = await import('@/hooks/retiredNarrationCancellation');
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('storage full'); });
    await expect(recordNarrationStart('cancel_subtitle_narration')).rejects.toThrow('storage full');
    expect(invoke).not.toHaveBeenCalledWith('start_subtitle_narration', expect.anything());
    setItem.mockRestore();
  });
  it('replays a failed cancellation after a WebView reload and stops on an evicted job', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('IPC unavailable')).mockResolvedValue({ outcome: 'missing' });
    const first = await import('@/hooks/retiredNarrationCancellation');
    first.retireNarrationJob('cancel_subtitle_narration', 'retired-job');
    await vi.advanceTimersByTimeAsync(0);
    expect(localStorage.length).toBe(1);
    // A reload discards all JS state and timers but retains the browser profile.
    vi.clearAllTimers();
    vi.resetModules();
    const reloaded = await import('@/hooks/retiredNarrationCancellation');
    reloaded.resumeRetiredNarrationCancellations();
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenLastCalledWith('cancel_subtitle_narration', { jobId: 'retired-job' });
    expect(localStorage.length).toBe(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('stops retrying when the worker reports a terminal job', async () => {
    vi.mocked(invoke).mockResolvedValue({ outcome: 'terminal' });
    const { retireNarrationJob } = await import('@/hooks/retiredNarrationCancellation');
    retireNarrationJob('cancel_s2s_narration', 'finished-job');
    await vi.advanceTimersByTimeAsync(60_000);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(localStorage.length).toBe(0);
  });
});
