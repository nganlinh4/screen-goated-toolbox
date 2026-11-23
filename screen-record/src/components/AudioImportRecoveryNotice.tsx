import { useSettings } from '@/hooks/useSettings';
import { retryAudioImportRecovery, useAudioImportRecovery } from '@/hooks/audioImportRecovery';

export function AudioImportRecoveryNotice({ ownerClipId }: { ownerClipId?: string | null }) {
  void ownerClipId;
  const { t } = useSettings();
  const recoveries = useAudioImportRecovery();
  if (!recoveries.length) return null;
  return <section className="audio-import-recovery-notice fixed bottom-4 right-4 z-[290] max-w-md rounded-xl border border-[var(--tertiary-color)] bg-[var(--ui-surface-3)] p-4 text-sm text-[var(--on-surface)] shadow-xl"
    role="alert" aria-live="assertive">
    <p className="audio-import-recovery-title font-semibold">{t.audioRecoveryTitle}</p>
    {recoveries.map((recovery) => <div className="audio-import-recovery-item mt-1" key={recovery.fence.projectId}>
      <p className="audio-import-recovery-project font-medium">{recovery.projectName}</p>
      <p className="audio-import-recovery-description">{t.audioRecoveryDescription}</p>
      {!recovery.isCurrent() && <p className="audio-import-recovery-unavailable mt-1">{t.audioRecoveryUnavailable}</p>}
      {recovery.failed && <p className="audio-import-recovery-failed mt-1">{t.audioRecoveryFailed}</p>}
      <button className="audio-import-recovery-retry ui-action-button mt-2 px-3 py-1" type="button"
        disabled={recovery.busy || !recovery.isCurrent()} onClick={() => void retryAudioImportRecovery(recovery.fence)}>
        {recovery.busy ? t.audioRecoveryRetrying : t.audioRecoveryRetry}
      </button>
    </div>)}
  </section>;
}
