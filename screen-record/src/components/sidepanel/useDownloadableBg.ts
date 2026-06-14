import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { BackgroundConfig } from '@/types/video';
import { DEFAULT_BUILT_IN_BACKGROUND_ID } from '@/lib/backgroundPresets';

export interface DownloadableBg {
  id: string;
  preview: string;
  downloadUrl: string;
}

export type BgDlState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'downloading'; progress: number }
  | { status: 'prewarming' }
  | { status: 'done'; ext: string; version: number }
  | { status: 'error'; message: string };

type DownloadableBgNativeState = {
  downloaded?: boolean;
  ext?: string | null;
  version?: number | null;
  progress?: unknown;
};

export const buildDownloadedBgUrl = (id: string, ext: string, version: number): string =>
  `/bg-downloaded/${id}.${ext}?v=${version}`;

export const nativeBgStateToUiState = (nativeState?: DownloadableBgNativeState): BgDlState => {
  if (!nativeState) return { status: 'idle' };
  if (typeof nativeState.progress === 'object' && nativeState.progress !== null) {
    if ('Downloading' in nativeState.progress) {
      const progress = (nativeState.progress as { Downloading?: { progress?: number } }).Downloading?.progress ?? 0;
      return { status: 'downloading', progress };
    }
    if ('Error' in nativeState.progress) {
      return { status: 'error', message: String((nativeState.progress as { Error?: unknown }).Error ?? '') };
    }
  }
  if (nativeState.downloaded && nativeState.ext) {
    return { status: 'done', ext: nativeState.ext, version: nativeState.version ?? 0 };
  }
  return { status: 'idle' };
};

export function useDownloadableBg(
  bg: DownloadableBg,
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>,
  syncedState?: BgDlState,
) {
  const [state, setState] = useState<BgDlState>(syncedState ?? { status: 'idle' });
  const syncInFlightRef = useRef(false);
  const prewarmedUrlSetRef = useRef<Set<string>>(new Set());
  const prewarmInFlightUrlSetRef = useRef<Set<string>>(new Set());
  const pendingPostDownloadPrewarmRef = useRef(false);
  const pendingAutoApplyRef = useRef(false);

  useEffect(() => {
    if (!syncedState) return;
    setState(prev => {
      if (prev.status === 'downloading' || prev.status === 'prewarming') return prev;
      return syncedState;
    });
  }, [syncedState]);

  const ensurePrewarmed = useCallback(async (url: string) => {
    if (prewarmedUrlSetRef.current.has(url)) return;
    if (prewarmInFlightUrlSetRef.current.has(url)) return;
    prewarmInFlightUrlSetRef.current.add(url);
    try {
      await invoke('prewarm_custom_background', { url });
      prewarmedUrlSetRef.current.add(url);
    } finally {
      prewarmInFlightUrlSetRef.current.delete(url);
    }
  }, []);

  const syncState = useCallback(async () => {
    if (syncInFlightRef.current) return;
    syncInFlightRef.current = true;

    try {
      const [infoRes, progressRes] = await Promise.allSettled([
        invoke<{ downloaded: boolean; ext: string | null; version?: number | null }>('check_bg_downloaded', { id: bg.id }),
        invoke<any>('get_bg_download_progress', { id: bg.id }),
      ]);

      let isDownloaded = false;
      let ext = '';
      let version = 0;

      if (infoRes.status === 'fulfilled') {
        const info = infoRes.value;
        isDownloaded = Boolean(info.downloaded && info.ext);
        ext = info.ext ?? '';
        version = info.version ?? 0;

        if (isDownloaded) {
          const syncedUrl = buildDownloadedBgUrl(bg.id, ext, version);
          setBackgroundConfig(prev => {
            if (
              prev.backgroundType === 'custom' &&
              typeof prev.customBackground === 'string' &&
              prev.customBackground.includes(`/bg-downloaded/${bg.id}.`) &&
              prev.customBackground !== syncedUrl
            ) {
              return { ...prev, customBackground: syncedUrl };
            }
            return prev;
          });
        } else {
          setBackgroundConfig(prev => {
            if (
              prev.backgroundType === 'custom' &&
              typeof prev.customBackground === 'string' &&
              prev.customBackground.includes(`/bg-downloaded/${bg.id}.`)
            ) {
              return { ...prev, backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID, customBackground: undefined };
            }
            return prev;
          });
        }
      }

      let next: BgDlState = isDownloaded && ext
        ? { status: 'done', ext, version }
        : { status: 'idle' };

      if (progressRes.status === 'fulfilled') {
        const progress = progressRes.value;
        if (typeof progress === 'object' && progress !== null) {
          if ('Downloading' in progress) {
            next = { status: 'downloading', progress: progress.Downloading.progress };
          } else if ('Error' in progress) {
            next = { status: 'error', message: progress.Error };
          }
        } else if (progress === 'Done') {
          if (isDownloaded && ext) {
            const syncedUrl = buildDownloadedBgUrl(bg.id, ext, version);
            if (
              pendingPostDownloadPrewarmRef.current &&
              !prewarmedUrlSetRef.current.has(syncedUrl) &&
              !prewarmInFlightUrlSetRef.current.has(syncedUrl)
            ) {
              void ensurePrewarmed(syncedUrl)
                .then(() => {
                  pendingPostDownloadPrewarmRef.current = false;
                  if (pendingAutoApplyRef.current) {
                    pendingAutoApplyRef.current = false;
                    setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: syncedUrl }));
                  }
                  setState({ status: 'done', ext, version });
                })
                .catch((e) => {
                  pendingPostDownloadPrewarmRef.current = false;
                  pendingAutoApplyRef.current = false;
                  console.warn('Failed to prewarm downloaded background after download:', e);
                  setState({ status: 'done', ext, version });
                });
            }
            const needsPrewarm =
              pendingPostDownloadPrewarmRef.current &&
              (!prewarmedUrlSetRef.current.has(syncedUrl) || prewarmInFlightUrlSetRef.current.has(syncedUrl));
            if (!needsPrewarm && pendingAutoApplyRef.current) {
              pendingAutoApplyRef.current = false;
              setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: syncedUrl }));
            }
            next = needsPrewarm
              ? { status: 'prewarming' }
              : { status: 'done', ext, version };
          } else {
            next = { status: 'idle' };
          }
        }
      }

      setState(prev => {
        // Don't let polling auto-exit 'prewarming' while a prewarm is still in flight
        if (prev.status === 'prewarming' && next.status === 'done' && prewarmInFlightUrlSetRef.current.size > 0) return prev;
        if (prev.status !== next.status) return next;
        if (prev.status === 'downloading' && next.status === 'downloading' && prev.progress !== next.progress) return next;
        if (prev.status === 'done' && next.status === 'done' && (prev.ext !== next.ext || prev.version !== next.version)) return next;
        if (prev.status === 'error' && next.status === 'error' && prev.message !== next.message) return next;
        return prev;
      });
    } finally {
      syncInFlightRef.current = false;
    }
  }, [bg.id, ensurePrewarmed, setBackgroundConfig]);

  const startDownload = useCallback(() => {
    if (state.status === 'downloading') return;
    pendingPostDownloadPrewarmRef.current = true;
    pendingAutoApplyRef.current = true;
    setState({ status: 'downloading', progress: 0 });
    invoke('start_bg_download', { id: bg.id, url: bg.downloadUrl });
  }, [bg.id, bg.downloadUrl, state.status]);

  const selectBg = useCallback(async () => {
    if (state.status !== 'done') return;
    const url = buildDownloadedBgUrl(bg.id, state.ext, state.version);
    if (!prewarmedUrlSetRef.current.has(url)) {
      setState({ status: 'prewarming' });
      try {
        await ensurePrewarmed(url);
      } catch (e) {
        console.warn('Failed to prewarm selected downloaded background:', e);
      }
    }
    // Apply background while spinner is still visible (state still 'prewarming')
    setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: url }));
    // Defer spinner dismissal to next tick so background renders before overlay drops
    const doneExt = state.ext, doneVersion = state.version;
    setTimeout(() => setState({ status: 'done', ext: doneExt, version: doneVersion }), 0);
  }, [bg.id, ensurePrewarmed, state, setBackgroundConfig]);

  const deleteBg = useCallback(async () => {
    try {
      await invoke('delete_bg_download', { id: bg.id });
      setState({ status: 'idle' });
      setBackgroundConfig(prev => {
        if (
          prev.backgroundType === 'custom' &&
          typeof prev.customBackground === 'string' &&
          prev.customBackground.includes(`/bg-downloaded/${bg.id}.`)
        ) {
          return { ...prev, backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID, customBackground: undefined };
        }
        return prev;
      });
    } catch (e) {
      console.error('Failed to delete downloaded background:', e);
    }
  }, [bg.id, setBackgroundConfig]);

  useEffect(() => {
    if (state.status !== 'downloading') return;
    const interval = setInterval(syncState, 500);
    return () => {
      clearInterval(interval);
    };
  }, [state.status, syncState]);

  return { state, startDownload, selectBg, deleteBg };
}
