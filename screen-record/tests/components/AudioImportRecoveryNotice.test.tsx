import { useCallback, useEffect, useRef, useState } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { AudioImportRecoveryNotice } from '@/components/AudioImportRecoveryNotice';
import { beginAudioImportWrite, finishAudioImportWrite, markAudioImportCommitted } from '@/lib/audioImportWriteFence';
import { clearAudioImportRecovery, retainAudioImportRecovery } from '@/hooks/audioImportRecovery';

vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: {
  audioRecoveryTitle: 'Unsaved audio edits', audioRecoveryDescription: 'Retry saving',
  audioRecoveryUnavailable: 'Original clip unavailable', audioRecoveryFailed: 'Retry failed',
  audioRecoveryRetry: 'Retry save', audioRecoveryRetrying: 'Saving',
} }) }));

it('updates retry availability as the original clip is left and restored', async () => {
  const fence = beginAudioImportWrite('clip-recovery-test');
  markAudioImportCommitted(fence.projectId, fence);
  const retry = vi.fn(async () => {});
  function Editor() {
    const [clip, setClip] = useState('root');
    const clipRef = useRef(clip);
    const select = (next: string) => { clipRef.current = next; setClip(next); };
    const isCurrent = useCallback(() => clipRef.current === 'root', []);
    return <><button onClick={() => select('other')}>Other clip</button>
      <button onClick={() => select('root')}>Root clip</button>
      <AudioImportRecoveryNotice ownerClipId={clip} />
      <RecoveryRegistration isCurrent={isCurrent} />
    </>;
  }
  function RecoveryRegistration({ isCurrent }: { isCurrent: () => boolean }) {
    useEffect(() => { retainAudioImportRecovery(fence, 'Project', isCurrent, retry); }, [isCurrent]);
    return null;
  }
  const view = render(<Editor />);
  expect(screen.getByRole('button', { name: 'Retry save' })).toBeEnabled();
  fireEvent.click(screen.getByRole('button', { name: 'Other clip' }));
  expect(screen.getByRole('button', { name: 'Retry save' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Root clip' }));
  expect(screen.getByRole('button', { name: 'Retry save' })).toBeEnabled();
  fireEvent.click(screen.getByRole('button', { name: 'Retry save' }));
  await waitFor(() => expect(retry).toHaveBeenCalledOnce());
  act(() => { clearAudioImportRecovery(fence); finishAudioImportWrite(fence, true); });
  view.unmount();
});
