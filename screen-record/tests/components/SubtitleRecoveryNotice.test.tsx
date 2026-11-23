import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { SubtitleRecoveryNotice } from '@/components/SubtitleRecoveryNotice';
import { claimResumableStart, clearResumableRun, releaseResumableStart, saveResumableRun } from '@/hooks/resumableJobRegistry';
import { clearSubtitleStartTicket, saveSubtitleStartTicket } from '@/hooks/subtitleStartTicket';

const mocks = vi.hoisted(() => ({
  save: vi.fn(), clear: vi.fn(), discard: vi.fn(), discardPending: vi.fn(), retry: vi.fn(), release: vi.fn(), retire: vi.fn(),
  retiring: [] as { jobId: string; projectId: string }[],
  pending: null as null | { version: number; jobId: string; projectId: string; context: null; results: { clipId: string; isPartial: boolean; segments: { startTime: number; endTime: number; text: string }[] }[] },
  record: { version: 1, jobId: 'job', projectId: 'p', context: null,
    results: [{ clipId: 'clip', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'hello' }] }] },
}));
vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: {
  subtitleRecoveryTitle: 'Recovery', subtitleRecoveryDescription: 'Retained', subtitleRecoveryUnavailable: 'Unavailable',
  subtitleRecoveryRetry: 'Retry', subtitleRecoveryExport: 'Export SRT', subtitleRecoveryExported: 'SRT saved.',
  subtitleRecoveryDiscard: 'Clear recovery', subtitleRecoveryConfirm: 'Really clear?', subtitleRecoveryConfirmDiscard: 'Yes, clear',
  subtitleRecoveryCancel: 'Keep', subtitleSrtSavedTo: 'Saved to',
  subtitleRecoveryMemoryOnly: 'Only in this window',
  subtitleRetirementUnconfirmed: 'Still waiting', subtitleRetirementRelease: 'Stop waiting',
  subtitleRetirementConfirm: 'Results may be lost', subtitleRetirementConfirmRelease: 'Yes, stop waiting',
} }) }));
vi.mock('@/hooks/subtitleCompletedPersistence', () => ({ useSubtitleCompletedSaveState: () => 'superseded', discardSubtitleCompletedSave: mocks.discard }));
vi.mock('@/hooks/subtitleRecoveryJournal', () => ({ useSubtitleRecovery: () => ({ record: mocks.record, pending: mocks.pending, error: mocks.pending ? 'quota' : null }),
  clearSubtitleRecovery: mocks.clear, discardPendingSubtitleRecovery: mocks.discardPending, retrySubtitleRecovery: mocks.retry }));
vi.mock('@/lib/subtitleSrt', () => ({ saveSubtitleSrt: mocks.save }));
vi.mock('@/hooks/retiredSubtitleGeneration', () => ({ useRetiringSubtitleGenerations: () => mocks.retiring,
  releaseRetiringSubtitleGeneration: mocks.release, retireSubtitleGeneration: mocks.retire }));

it('acknowledges export only after an actual save and requires a separate clear confirmation', async () => {
  mocks.save.mockResolvedValueOnce(null).mockResolvedValueOnce('C:/exports/recovered.srt');
  mocks.clear.mockResolvedValue(true);
  render(<SubtitleRecoveryNotice />);
  fireEvent.click(screen.getByRole('button', { name: 'Export SRT' }));
  await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(1));
  expect(screen.queryByText('SRT saved.')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Export SRT' }));
  expect(await screen.findByText('SRT saved.')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Clear recovery' }));
  expect(screen.getByText('Really clear?')).toBeInTheDocument();
  expect(mocks.clear).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Keep' }));
  expect(mocks.clear).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Clear recovery' }));
  fireEvent.click(screen.getByRole('button', { name: 'Yes, clear' }));
  await waitFor(() => expect(mocks.clear).toHaveBeenCalledWith('job', mocks.record));
  expect(mocks.discard).toHaveBeenCalledWith('job');
});

it('separately exports and confirms the exact pending recovery when another record is stored', async () => {
  mocks.pending = { ...mocks.record, jobId: 'pending-job', results: [{ ...mocks.record.results[0], clipId: 'pending-clip' }] };
  mocks.discardPending.mockResolvedValue(true);
  mocks.save.mockResolvedValue('C:/exports/pending.srt');
  render(<SubtitleRecoveryNotice />);
  const item = screen.getByText('Only in this window').parentElement!;
  fireEvent.click(within(item).getByRole('button', { name: 'Export SRT' }));
  expect(await within(item).findByText('SRT saved.')).toBeInTheDocument();
  fireEvent.click(within(item).getByRole('button', { name: 'Clear recovery' }));
  expect(mocks.discardPending).not.toHaveBeenCalled();
  fireEvent.click(within(item).getByRole('button', { name: 'Yes, clear' }));
  await waitFor(() => expect(mocks.discardPending).toHaveBeenCalledWith(mocks.pending));
  expect(mocks.clear).not.toHaveBeenCalledWith('pending-job', expect.anything());
  mocks.pending = null;
});

it('requires an informed separate confirmation before stopping retirement retries', () => {
  const item = { jobId: 'waiting', projectId: 'p' };
  mocks.retiring = [item];
  render(<SubtitleRecoveryNotice />);
  fireEvent.click(screen.getByRole('button', { name: 'Stop waiting' }));
  expect(screen.getByText('Results may be lost')).toBeInTheDocument();
  expect(mocks.release).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Yes, stop waiting' }));
  expect(mocks.release).toHaveBeenCalledWith(item);
  mocks.retiring = [];
});

it.each(['start', 'run'] as const)('does not retire a live %s on notice remount', (stage) => {
  const jobId = 'ticket-0123456789abcdef0123456789abcdef-transcription-3';
  saveSubtitleStartTicket({ jobId, projectKey: 'p', projectEpoch: 0, jobContext: { projectKey: 'p' } as never,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const claim = stage === 'start' ? claimResumableStart('subtitle-generation', {}) : null;
  if (stage === 'run') saveResumableRun('subtitle-generation', { jobId });
  render(<SubtitleRecoveryNotice />);
  expect(mocks.retire).not.toHaveBeenCalled();
  if (claim) releaseResumableStart('subtitle-generation', claim);
  clearResumableRun('subtitle-generation');
  clearSubtitleStartTicket(jobId);
});

it('reports an invalid ticket and requires confirmation before removing that exact value', () => {
  localStorage.setItem('sgt:subtitle-generation-start-v1', '{invalid');
  render(<SubtitleRecoveryNotice />);
  expect(screen.getByText('Unavailable')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Stop waiting' }));
  expect(screen.getByText('Results may be lost')).toBeInTheDocument();
  expect(localStorage.getItem('sgt:subtitle-generation-start-v1')).toBe('{invalid');
  fireEvent.click(screen.getByRole('button', { name: 'Yes, stop waiting' }));
  expect(localStorage.getItem('sgt:subtitle-generation-start-v1')).toBeNull();
});

it('does not reuse a discard confirmation for a different recovery job', () => {
  const { rerender } = render(<SubtitleRecoveryNotice />);
  fireEvent.click(screen.getByRole('button', { name: 'Clear recovery' }));
  expect(screen.getByText('Really clear?')).toBeInTheDocument();
  mocks.record.jobId = 'different-job';
  rerender(<SubtitleRecoveryNotice />);
  expect(screen.queryByRole('button', { name: 'Yes, clear' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Clear recovery' })).toBeInTheDocument();
  mocks.record.jobId = 'job';
});
