import { useState } from 'react';
import { useSettings } from '@/hooks/useSettings';
import { invoke } from '@/lib/ipc';
import { armSubtitleShutdownOverride, cancelSubtitleShutdownOverride, currentSubtitleShutdownOverride } from '@/hooks/subtitleShutdown';

export function SubtitleShutdownChoice() {
  const { t } = useSettings();
  const [confirmClose, setConfirmClose] = useState(false);
  return <div className="subtitle-shutdown-choice mt-2">
    <p className="subtitle-shutdown-warning">{t.subtitleShutdownBlocked}</p>
    {!confirmClose
      ? <button className="subtitle-shutdown-request-close ui-action-button mt-1" onClick={() => setConfirmClose(true)}>{t.subtitleShutdownCloseAnyway}</button>
      : <div className="subtitle-shutdown-confirm mt-1">
        <p className="subtitle-shutdown-confirm-text">{t.subtitleShutdownConfirm}</p>
        <button className="subtitle-shutdown-confirm-close ui-action-button mr-2 px-3 py-1" onClick={() => {
          if (!armSubtitleShutdownOverride()) return;
          const overrideId = currentSubtitleShutdownOverride();
          void invoke('close_window', { subtitleOverrideId: overrideId }).catch(() => { cancelSubtitleShutdownOverride(overrideId); setConfirmClose(false); });
        }}>{t.subtitleShutdownConfirmClose}</button>
        <button className="subtitle-shutdown-confirm-cancel ui-action-button px-3 py-1" onClick={() => setConfirmClose(false)}>{t.subtitleShutdownKeepOpen}</button>
      </div>}
  </div>;
}
