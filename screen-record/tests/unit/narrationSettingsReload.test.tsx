import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@/lib/ipc';
import { useNarrationSettings } from '@/hooks/useNarrationSettings';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn() }));

type NarrationTtsMetadata = NonNullable<ReturnType<typeof useNarrationSettings>['metadata']>;

function metadataReply(edgeVoiceState: 'loading' | 'ready'): NarrationTtsMetadata {
  return {
    defaults: { method: 'GeminiLive', edgeVoice: 'en-US-AriaNeural', edgePitch: 0 },
    edgeVoiceState,
  } as unknown as NarrationTtsMetadata;
}

beforeEach(() => {
  vi.useFakeTimers();
  window.localStorage.clear();
});

afterEach(() => {
  vi.useRealTimers();
  vi.mocked(invoke).mockReset();
});

it('keeps a setting changed before the TTS metadata arrives', async () => {
  let reply!: (meta: NarrationTtsMetadata) => void;
  vi.mocked(invoke).mockImplementation(() => new Promise((resolve) => { reply = resolve as typeof reply; }));
  const { result } = renderHook(() => useNarrationSettings());

  act(() => result.current.update('method', 'EdgeTTS'));
  await act(async () => reply(metadataReply('ready')));

  expect(result.current.settings.method).toBe('EdgeTTS');
  expect(result.current.settings.edgeVoice).toBe('en-US-AriaNeural');
});

it('keeps a setting changed while the Edge voice list is still loading', async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce(metadataReply('loading'))
    .mockResolvedValueOnce(metadataReply('ready'));
  const { result } = renderHook(() => useNarrationSettings());
  await act(async () => {});
  expect(result.current.settings.method).toBe('GeminiLive');

  act(() => result.current.update('edgePitch', 14));
  await act(async () => { await vi.advanceTimersByTimeAsync(1200); });

  expect(invoke).toHaveBeenCalledTimes(2);
  expect(result.current.settings.edgePitch).toBe(14);
});

it('reset still returns to the host defaults', async () => {
  vi.mocked(invoke).mockResolvedValue(metadataReply('ready'));
  const { result } = renderHook(() => useNarrationSettings());
  await act(async () => {});

  act(() => result.current.update('method', 'EdgeTTS'));
  act(() => result.current.resetToDefaults());

  expect(result.current.settings.method).toBe('GeminiLive');
});

it.each(['StepAudioEditX', 'Supertonic'])(
  'falls back to the host default when the saved provider %s no longer exists',
  async (retired) => {
    window.localStorage.setItem(
      'screen-record-narration-tts-v3',
      JSON.stringify({ method: retired, edgePitch: 12 }),
    );
    vi.mocked(invoke).mockResolvedValue(metadataReply('ready'));
    const { result } = renderHook(() => useNarrationSettings());
    await act(async () => {});

    expect(result.current.settings.method).toBe('GeminiLive');
    expect(result.current.settings.edgePitch).toBe(12);
  },
);
