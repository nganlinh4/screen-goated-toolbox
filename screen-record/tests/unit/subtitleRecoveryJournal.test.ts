import { afterEach, expect, it, vi } from 'vitest';
import type { SubtitleRecoveryRecord } from '@/hooks/subtitleRecoveryJournal';

const record: SubtitleRecoveryRecord = {
  version: 1, jobId: 'recover-job', projectId: 'project-a', context: null,
  results: [{ clipId: 'clip', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'recovered' }] }],
};

afterEach(async () => {
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  await journal.clearSubtitleRecovery('recover-job').catch(() => {});
  await journal.clearSubtitleRecovery('other-job').catch(() => {});
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it('survives module restart and never clears another job', async () => {
  const first = await import('@/hooks/subtitleRecoveryJournal');
  await first.retainSubtitleRecovery(record);
  expect(first.getSubtitleRecovery().record).toEqual(record);
  vi.resetModules();
  const reopened = await import('@/hooks/subtitleRecoveryJournal');
  await reopened.loadSubtitleRecovery();
  expect(reopened.getSubtitleRecovery().record).toEqual(record);
  expect(await reopened.clearSubtitleRecovery('other-job')).toBe(false);
  expect(reopened.getSubtitleRecovery().record).toEqual(record);
  await expect(reopened.requireSubtitleRecoveryClear('unavailable', 'pending')).rejects.toThrow('pending');
  expect(await reopened.clearSubtitleRecovery('recover-job')).toBe(true);
  await reopened.requireSubtitleRecoveryClear('unavailable', 'pending');
});

it('durably fences a confirmed clear across a module restart and retires its replay ticket', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  const ticketStore = await import('@/hooks/subtitleStartTicket');
  const jobId = 'ticket-0123456789abcdef0123456789abcdef-transcription-1';
  const recovery = { ...record, jobId, context: { projectKey: 'project-a' } as SubtitleRecoveryRecord['context'] };
  ticketStore.saveSubtitleStartTicket({ jobId, projectKey: 'project-a', projectEpoch: 0, jobContext: recovery.context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  await journal.retainSubtitleRecovery(recovery);
  expect(await journal.clearSubtitleRecovery(jobId, recovery)).toBe(true);
  expect(ticketStore.readSubtitleStartTicket()).toBeNull();
  vi.resetModules();
  const restarted = await import('@/hooks/subtitleRecoveryJournal');
  expect(await restarted.wasSubtitleRecoveryAcknowledged(jobId)).toBe(true);
  expect(await restarted.wasSubtitleRecoveryAcknowledged('other-job')).toBe(false);
  await restarted.loadSubtitleRecovery();
  expect(restarted.getSubtitleRecovery().record).toBeNull();
});

it('retains an unacknowledged payload in memory and retries a quota failure', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  await journal.loadSubtitleRecovery();
  const put = IDBObjectStore.prototype.put;
  let failed = false;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    if (!failed) { failed = true; throw new DOMException('quota', 'QuotaExceededError'); }
    return key === undefined ? put.call(this, value) : put.call(this, value, key);
  });
  const durable = journal.retainSubtitleRecovery(record);
  await vi.waitFor(() => expect(journal.getSubtitleRecovery().error).toContain('quota'));
  expect(journal.getSubtitleRecovery().pending).toEqual(record);
  expect(journal.getSubtitleRecovery().record).toBeNull();
  spy.mockRestore();
  journal.retrySubtitleRecovery();
  await durable;
  expect(journal.getSubtitleRecovery().record).toEqual(record);
  expect(journal.getSubtitleRecovery().pending).toBeNull();
});

it('never replaces another job recovery and proceeds only after its explicit removal', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  await journal.retainSubtitleRecovery(record);
  const next = { ...record, jobId: 'other-job', projectId: 'project-b' };
  const waiting = journal.retainSubtitleRecovery(next);
  await vi.waitFor(() => expect(journal.getSubtitleRecovery().error).toContain('awaiting attention'));
  expect(journal.getSubtitleRecovery().record).toEqual(record);
  expect(journal.getSubtitleRecovery().pending).toEqual(next);
  expect(await journal.clearSubtitleRecovery('recover-job')).toBe(true);
  journal.retrySubtitleRecovery();
  await waiting;
  expect(journal.getSubtitleRecovery().record).toEqual(next);
});

it('keeps omitted results recoverable even after the current project save succeeds', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  const queue = await import('@/hooks/subtitleCompletedPersistence');
  await journal.retainSubtitleRecovery(record);
  const owner = { current: 'project-a' as string | null };
  const unregister = queue.registerSubtitleSaveEditor(owner);
  const accepted = queue.requestSubtitleCompletedSave('recover-job', {
    projectId: 'project-a', composition: null, segment: null, loadedClipId: null,
  }, () => 'ready', async () => true, true);
  expect(accepted).toBe(true);
  await vi.waitFor(() => expect(queue.hasPendingSubtitleCompletedSave()).toBe(false));
  expect(journal.getSubtitleRecovery().record).toEqual(record);
  unregister();
});

it('explicitly retires a quota-blocked pending record and resolves its completion barrier', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  await journal.loadSubtitleRecovery();
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(() => { throw new DOMException('quota', 'QuotaExceededError'); });
  const pendingRecord = { ...record, jobId: 'pending-quota' };
  const barrier = journal.retainSubtitleRecovery(pendingRecord);
  await vi.waitFor(() => expect(journal.getSubtitleRecovery().error).toContain('quota'));
  expect(await journal.discardPendingSubtitleRecovery({ ...pendingRecord })).toBe(false);
  expect(await journal.discardPendingSubtitleRecovery(pendingRecord)).toBe(true);
  expect(await barrier).toBe(false);
  expect(journal.getSubtitleRecovery().pending).toBeNull();
  const calls = spy.mock.calls.length;
  journal.retrySubtitleRecovery();
  await Promise.resolve();
  expect(spy).toHaveBeenCalledTimes(calls);
});

it('removes the replay ticket before discarding a quota-blocked recovery', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  const tickets = await import('@/hooks/subtitleStartTicket');
  await journal.loadSubtitleRecovery();
  const jobId = 'ticket-0123456789abcdef0123456789abcdef-transcription-2';
  const pending = { ...record, jobId, context: { projectKey: 'project-a' } as SubtitleRecoveryRecord['context'] };
  tickets.saveSubtitleStartTicket({ jobId, projectKey: 'project-a', projectEpoch: 0, jobContext: pending.context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(() => { throw new DOMException('quota', 'QuotaExceededError'); });
  const barrier = journal.retainSubtitleRecovery(pending);
  await vi.waitFor(() => expect(journal.getSubtitleRecovery().error).toContain('quota'));
  expect(await journal.discardPendingSubtitleRecovery(pending)).toBe(true);
  expect(await barrier).toBe(false);
  expect(tickets.readSubtitleStartTicket()).toBeNull();
  vi.resetModules();
  const reopened = await import('@/hooks/subtitleStartTicket');
  expect(reopened.readSubtitleStartTicket()).toBeNull();
});

it('aborts and reconciles an in-flight journal put before reporting pending discard', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  await journal.loadSubtitleRecovery();
  const put = IDBObjectStore.prototype.put;
  const pendingRecord = { ...record, jobId: 'pending-inflight' };
  let discard: Promise<boolean> | undefined;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    const request = key === undefined ? put.call(this, value) : put.call(this, value, key);
    if (key === 'subtitle-completed-recovery-v1') discard = journal.discardPendingSubtitleRecovery(pendingRecord);
    return request;
  });
  const barrier = journal.retainSubtitleRecovery(pendingRecord);
  await vi.waitFor(() => expect(discard).toBeDefined());
  expect(await discard).toBe(true);
  expect(await barrier).toBe(false);
  await journal.loadSubtitleRecovery(true);
  expect(journal.getSubtitleRecovery().record).toBeNull();
  expect(journal.getSubtitleRecovery().pending).toBeNull();
  spy.mockRestore();
});

it('allows explicit discard of an oversized in-memory recovery', async () => {
  vi.resetModules();
  const journal = await import('@/hooks/subtitleRecoveryJournal');
  const oversized = { ...record, jobId: 'oversized', results: [{ ...record.results[0], segments: [
    { ...record.results[0].segments[0], text: 'x'.repeat(16 * 1024 * 1024) },
  ] }] };
  const barrier = journal.retainSubtitleRecovery(oversized);
  await vi.waitFor(() => expect(journal.getSubtitleRecovery().error).toContain('16 MiB'));
  expect(await journal.discardPendingSubtitleRecovery(oversized)).toBe(true);
  expect(await barrier).toBe(false);
});
