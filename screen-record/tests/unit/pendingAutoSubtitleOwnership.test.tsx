import { act, renderHook, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { invoke } from '@/lib/ipc';
import { activateNarrationProject, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { importedProjectOwner, rememberImportedProjectOwner } from '@/hooks/importProjectOwnership';
import { useAppDropActions, usePendingAutoSubtitleGeneration, type PendingAutoSubtitleRequest } from '@/hooks/useAppDropActions';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => []), logToHost: vi.fn() }));
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

it.each(['unchanged', 'foreign', 'reopened'] as const)(
  'checks the exact dropped-video activation before the delayed generation: %s', (change) => {
    vi.useFakeTimers();
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'p');
    const isCurrent = importedProjectOwner(owner, 'p');
    if (!isCurrent) throw new Error('expected current owner');
    const request: PendingAutoSubtitleRequest = { projectId: 'p', isCurrent };
    const generate = vi.fn();
    const hook = renderHook(() => {
      const [pending, setPending] = useState<PendingAutoSubtitleRequest | null>(request);
      const [armed, setArmed] = useState(false);
      usePendingAutoSubtitleGeneration({ currentProjectId: 'p', currentRawVideoPath: 'video.mp4',
        pendingAutoSubtitleArmed: armed, pendingAutoSubtitleProjectId: pending, setPendingAutoSubtitleProjectId: setPending,
        setPendingAutoSubtitleArmed: setArmed, setActivePanel: vi.fn(), setSubtitleSource: vi.fn(),
        subtitleSource: 'video', isGeneratingSubtitles: false, handleGenerateSubtitles: generate });
    });
    if (change === 'foreign') setNarrationProjectId(owner, 'other');
    if (change === 'reopened') activateNarrationProject(owner, 'p');
    act(() => vi.advanceTimersByTime(200));
    expect(generate).toHaveBeenCalledTimes(change === 'unchanged' ? 1 : 0);
    hook.unmount(); setNarrationProjectId(owner, null);
  },
);

it('does not queue a video drop whose imported owner changed before its continuation', async () => {
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, 'p');
  const project = { id: 'p' };
  const isCurrent = importedProjectOwner(owner, 'p');
  if (!isCurrent) throw new Error('expected current owner');
  rememberImportedProjectOwner(project, isCurrent);
  let resolveImport!: (value: typeof project) => void;
  const imported = new Promise<typeof project>((resolve) => { resolveImport = resolve; });
  const setPending = vi.fn();
  vi.mocked(invoke).mockImplementation(async (command) => command === 'take_pending_video_drop_actions'
    ? [{ path: 'clip.mp4', action: 'generate-subtitles' }] : []);
  const importVideoPath = vi.fn(() => imported);
  const hook = renderHook(() => useAppDropActions({ getCurrentProjectId: () => owner.current,
    importAudioPaths: vi.fn(), importSubtitlePayload: vi.fn(), importVideoPath, setPendingAutoSubtitleProjectId: setPending }));
  await waitFor(() => expect(importVideoPath).toHaveBeenCalledOnce());
  activateNarrationProject(owner, 'p');
  await act(async () => { resolveImport(project); await imported; });
  expect(setPending).not.toHaveBeenCalled();
  hook.unmount(); setNarrationProjectId(owner, null);
});
