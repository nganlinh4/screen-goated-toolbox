import { useEffect, useState } from 'react';
import { useSettings } from '@/hooks/useSettings';
import { discardSubtitleCompletedSave, useSubtitleCompletedSaveState } from '@/hooks/subtitleCompletedPersistence';
import { clearSubtitleRecovery, discardPendingSubtitleRecovery, retrySubtitleRecovery, useSubtitleRecovery } from '@/hooks/subtitleRecoveryJournal';
import type { SubtitleRecoveryRecord } from '@/hooks/subtitleRecoveryJournal';
import { releaseRetiringSubtitleGeneration, retireSubtitleGeneration, useRetiringSubtitleGenerations, type SubtitleRetirement } from '@/hooks/retiredSubtitleGeneration';
import { discardInvalidSubtitleStartTicket, readInvalidSubtitleStartTicket, readSubtitleStartTicket } from '@/hooks/subtitleStartTicket';
import { hasResumableStart, readResumableRun } from '@/hooks/resumableJobRegistry';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { saveSubtitleSrt } from '@/lib/subtitleSrt';
import { useSubtitleShutdownBlocked } from '@/hooks/subtitleShutdown';
import { SubtitleShutdownChoice } from './SubtitleShutdownChoice';

function RecoveryItem({ record, pending }: { record: SubtitleRecoveryRecord; pending: boolean }) {
  const { t } = useSettings();
  const [index, setIndex] = useState('0');
  const [confirmation, setConfirmation] = useState<{ record: SubtitleRecoveryRecord; serialized: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const selected = record.results[Number(index)] ?? record.results[0];
  const confirmed = confirmation?.record === record && confirmation.serialized === JSON.stringify(record);
  const exportSrt = async () => {
    if (!selected || busy) return;
    setBusy(true); setMessage('');
    try {
      const style = defaultSubtitleStyle();
      const subtitles = selected.segments.map((item, i) => ({ ...item, id: `recovered-${i}`, style }));
      const savedPath = await saveSubtitleSrt(subtitles, null, `recovered-${selected.clipId}`, t.subtitleSrtSavedTo);
      if (savedPath) setMessage(t.subtitleRecoveryExported);
    } catch { setMessage(t.subtitleRecoveryUnavailable); }
    finally { setBusy(false); }
  };
  const discard = async () => {
    if (!confirmed || busy) return;
    setBusy(true); setMessage('');
    try {
      const removed = pending ? await discardPendingSubtitleRecovery(record) : await clearSubtitleRecovery(record.jobId, record);
      if (removed && !pending) discardSubtitleCompletedSave(record.jobId);
      setConfirmation(null);
    } catch { setMessage(t.subtitleRecoveryUnavailable); }
    finally { setBusy(false); }
  };
  return <div className="subtitle-recovery-item mt-1">
    <p className="subtitle-recovery-description">{pending ? t.subtitleRecoveryMemoryOnly : t.subtitleRecoveryDescription}</p>
    {record.results.length > 0 && <>
      <select className="subtitle-recovery-source ui-input mt-2 max-w-full" aria-label={t.subtitleRecoveryTitle}
        value={String(Math.min(Number(index) || 0, record.results.length - 1))} onChange={(event) => setIndex(event.target.value)}>
        {record.results.map((item, i) => <option key={`${item.clipId}-${i}`} value={String(i)}>{item.clipId}</option>)}
      </select>
      <button className="subtitle-recovery-export ui-action-button ml-2 px-3 py-1" disabled={busy} onClick={() => void exportSrt()}>{t.subtitleRecoveryExport}</button>
    </>}
    {!confirmed
      ? <button className="subtitle-recovery-discard ui-action-button ml-2 px-3 py-1" disabled={busy} onClick={() => setConfirmation({ record, serialized: JSON.stringify(record) })}>{t.subtitleRecoveryDiscard}</button>
      : <div className="subtitle-recovery-confirm mt-2">
        <p className="subtitle-recovery-confirm-text">{t.subtitleRecoveryConfirm}</p>
        <button className="subtitle-recovery-confirm-discard ui-action-button mr-2 px-3 py-1" disabled={busy} onClick={() => void discard()}>{t.subtitleRecoveryConfirmDiscard}</button>
        <button className="subtitle-recovery-confirm-cancel ui-action-button px-3 py-1" disabled={busy} onClick={() => setConfirmation(null)}>{t.subtitleRecoveryCancel}</button>
      </div>}
    {message && <p className="subtitle-recovery-result mt-2">{message}</p>}
  </div>;
}

function RetirementItem({ item }: { item: SubtitleRetirement }) {
  const { t } = useSettings();
  const [confirmedItem, setConfirmedItem] = useState<SubtitleRetirement | null>(null);
  const [failed, setFailed] = useState(false);
  return <div className="subtitle-retirement-item mt-2">
    <p className="subtitle-retirement-description">{t.subtitleRetirementUnconfirmed}</p>
    {confirmedItem !== item
      ? <button className="subtitle-retirement-release ui-action-button mt-1 px-3 py-1" onClick={() => setConfirmedItem(item)}>{t.subtitleRetirementRelease}</button>
      : <div className="subtitle-retirement-confirm mt-1">
        <p className="subtitle-retirement-confirm-text">{t.subtitleRetirementConfirm}</p>
        <button className="subtitle-retirement-confirm-release ui-action-button mr-2 px-3 py-1" onClick={() => setFailed(!releaseRetiringSubtitleGeneration(item))}>{t.subtitleRetirementConfirmRelease}</button>
        <button className="subtitle-retirement-confirm-cancel ui-action-button px-3 py-1" onClick={() => setConfirmedItem(null)}>{t.subtitleRecoveryCancel}</button>
      </div>}
    {failed && <p className="subtitle-retirement-error">{t.subtitleRecoveryUnavailable}</p>}
  </div>;
}

export function SubtitleRecoveryNotice() {
  const shutdownBlocked = useSubtitleShutdownBlocked();
  const [ticketError, setTicketError] = useState(false);
  const [invalidTicket, setInvalidTicket] = useState<string | null>(null);
  const [confirmInvalidTicket, setConfirmInvalidTicket] = useState<string | null>(null);
  const [ticketAttempt, setTicketAttempt] = useState(0);
  useEffect(() => {
    try {
      const ticket = readSubtitleStartTicket();
      setTicketError(false);
      setInvalidTicket(null);
      if (ticket && !hasResumableStart('subtitle-generation') && !readResumableRun('subtitle-generation')) void retireSubtitleGeneration(ticket);
    }
    catch {
      setTicketError(true);
      try { setInvalidTicket(readInvalidSubtitleStartTicket()); } catch { setInvalidTicket(null); }
    }
  }, [ticketAttempt]);
  const { t } = useSettings();
  const recovery = useSubtitleRecovery();
  const saveState = useSubtitleCompletedSaveState();
  const retiring = useRetiringSubtitleGenerations();
  const showRecord = saveState === 'none' || saveState === 'unconfirmed' || saveState === 'superseded' || !!recovery.error;
  if (!shutdownBlocked && !(showRecord && recovery.record) && !recovery.pending && !recovery.error && !ticketError && retiring.length === 0) return null;
  return (
    <section className="subtitle-recovery-notice fixed bottom-4 left-4 z-[290] max-w-md rounded-xl border border-[var(--tertiary-color)] bg-[var(--ui-surface-3)] p-4 text-sm text-[var(--on-surface)] shadow-xl" role="status">
      <p className="subtitle-recovery-title font-semibold">{t.subtitleRecoveryTitle}</p>
      {shutdownBlocked && <SubtitleShutdownChoice />}
      {showRecord && recovery.record && <RecoveryItem record={recovery.record} pending={false} />}
      {recovery.pending && <RecoveryItem record={recovery.pending} pending />}
      {retiring.map((item) => <RetirementItem key={item.jobId} item={item} />)}
      {ticketError && <div className="subtitle-ticket-error mt-2">
        <p className="subtitle-ticket-error-message">{t.subtitleRecoveryUnavailable}</p>
        <button className="subtitle-ticket-retry ui-action-button mt-1 px-3 py-1" onClick={() => setTicketAttempt((value) => value + 1)}>{t.subtitleRecoveryRetry}</button>
        {invalidTicket !== null && (confirmInvalidTicket !== invalidTicket
          ? <button className="subtitle-ticket-release ui-action-button ml-2 px-3 py-1" onClick={() => setConfirmInvalidTicket(invalidTicket)}>{t.subtitleRetirementRelease}</button>
          : <div className="subtitle-ticket-confirm mt-1">
            <p className="subtitle-ticket-confirm-text">{t.subtitleRetirementConfirm}</p>
            <button className="subtitle-ticket-confirm-release ui-action-button mr-2 px-3 py-1" onClick={() => {
              try {
                if (discardInvalidSubtitleStartTicket(invalidTicket)) setTicketAttempt((value) => value + 1);
              } catch { setTicketError(true); }
              setConfirmInvalidTicket(null);
            }}>{t.subtitleRetirementConfirmRelease}</button>
            <button className="subtitle-ticket-confirm-cancel ui-action-button px-3 py-1" onClick={() => setConfirmInvalidTicket(null)}>{t.subtitleRecoveryCancel}</button>
          </div>)}
      </div>}
      {recovery.error && <div className="subtitle-recovery-error mt-2">
        <p className="subtitle-recovery-error-message">{t.subtitleRecoveryUnavailable}</p>
        <button className="subtitle-recovery-retry ui-action-button mt-1 px-3 py-1" onClick={retrySubtitleRecovery}>{t.subtitleRecoveryRetry}</button>
      </div>}
    </section>
  );
}
