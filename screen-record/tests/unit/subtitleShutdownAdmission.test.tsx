import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import * as projectStorage from '@/lib/projectStorage';
import { SubtitleShutdownChoice } from '@/components/SubtitleShutdownChoice';
import { invoke } from '@/lib/ipc';
import { requestSubtitleCompletedSave, discardSubtitleCompletedSave } from '@/hooks/subtitleCompletedPersistence';
import { getRetiringSubtitleGenerations, releaseRetiringSubtitleGeneration, retireSubtitleGeneration } from '@/hooks/retiredSubtitleGeneration';
import { clearSubtitleRecovery, retainSubtitleRecovery } from '@/hooks/subtitleRecoveryJournal';
import { cancelSubtitleShutdownOverride, clearSubtitleShutdownBlocked, hasPendingSubtitleShutdownWork,
  showSubtitleShutdownBlocked, subtitleShutdownOverrideArmed } from '@/hooks/subtitleShutdown';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn() }));
vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: {
  subtitleShutdownBlocked: 'Subtitle work is unfinished', subtitleShutdownCloseAnyway: 'Close without waiting',
  subtitleShutdownConfirm: 'Unfinished results may be lost', subtitleShutdownConfirmClose: 'Yes, close',
  subtitleShutdownKeepOpen: 'Keep window open',
} }) }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => { localStorage.removeItem('sgt:subtitle-generation-start-v1'); vi.mocked(invoke).mockResolvedValue(null); });
afterEach(() => { cancelSubtitleShutdownOverride(); clearSubtitleShutdownBlocked(); vi.restoreAllMocks(); });

it('blocks for an unacknowledged completed save and an uncertain retirement', async () => {
  expect(requestSubtitleCompletedSave('job-save', { projectId: 'p', composition: null, segment: null, loadedClipId: null },
    () => 'waiting', async () => false)).toBe(true);
  expect(hasPendingSubtitleShutdownWork()).toBe(true);
  discardSubtitleCompletedSave('job-save');
  const response = deferred<null>();
  vi.mocked(invoke).mockReturnValue(response.promise);
  const result = retireSubtitleGeneration({ jobId: 'job-retire', projectKey: null, projectEpoch: 0, jobContext: null,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  expect(hasPendingSubtitleShutdownWork()).toBe(true);
  expect(releaseRetiringSubtitleGeneration(getRetiringSubtitleGenerations()[0])).toBe(true);
  await result;
  response.resolve(null);
  expect(hasPendingSubtitleShutdownWork()).toBe(false);
});

it('blocks an in-flight journal write but permits close once its record is durable', async () => {
  const db = await projectStorage.openProjectDB();
  const opening = deferred<IDBDatabase>();
  const spy = vi.spyOn(projectStorage, 'openProjectDB').mockReturnValue(opening.promise);
  const record = { version: 1 as const, jobId: 'shutdown-journal', projectId: null, context: null,
    results: [{ clipId: 'clip', segments: [{ startTime: 0, endTime: 1, text: 'retained' }] }] };
  const retained = retainSubtitleRecovery(record);
  expect(hasPendingSubtitleShutdownWork()).toBe(true);
  opening.resolve(db);
  expect(await retained).toBe(true);
  spy.mockRestore();
  expect(hasPendingSubtitleShutdownWork()).toBe(false);
  await clearSubtitleRecovery(record.jobId);
});

it('requires a separate confirmation before arming close without waiting', async () => {
  showSubtitleShutdownBlocked();
  render(<SubtitleShutdownChoice />);
  fireEvent.click(screen.getByText('Close without waiting'));
  expect(invoke).not.toHaveBeenCalled();
  expect(screen.getByText('Unfinished results may be lost')).toBeTruthy();
  fireEvent.click(screen.getByText('Keep window open'));
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Close without waiting'));
  fireEvent.click(screen.getByText('Yes, close'));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('close_window', { subtitleOverrideId: expect.any(Number) }));
  expect(subtitleShutdownOverrideArmed()).toBe(true);
});
