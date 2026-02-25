import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { Trash2, Download, Loader2 } from 'lucide-react';
import { BackgroundConfig } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';
import downloadableBackgrounds from '@/config/downloadable-backgrounds.json';

/** Inline style for slider active track fill */
const sv = (v: number, min: number, max: number): React.CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as React.CSSProperties);

export const GRADIENT_PRESETS: Record<string, { className?: string; style?: React.CSSProperties }> = {
  gradient1: { className: 'bg-gradient-to-r from-[#4f7fd9] to-[#8a72d8]' },
  gradient2: { className: 'bg-gradient-to-r from-rose-400 to-orange-300' },
  gradient3: { className: 'bg-gradient-to-r from-emerald-500 to-teal-400' },
  gradient4: {
    style: {
      backgroundImage: [
        'radial-gradient(circle at 18% 78%, rgba(8,85,170,0.18) 0%, rgba(8,85,170,0) 78%)',
        'radial-gradient(circle at 86% 22%, rgba(249,115,22,0.14) 0%, rgba(249,115,22,0) 80%)',
        'linear-gradient(45deg, #061a40 0%, #0353a4 55%, #f97316 100%)',
      ].join(','),
    },
  },
  gradient5: {
    style: {
      backgroundImage: [
        'radial-gradient(circle at 82% 26%, rgba(239,71,111,0.18) 0%, rgba(239,71,111,0) 70%)',
        'radial-gradient(circle at 22% 86%, rgba(36,123,160,0.18) 0%, rgba(36,123,160,0) 72%)',
        'linear-gradient(52deg, #0d1b4c 0%, #4b4c99 52%, #ef476f 100%)',
      ].join(','),
    },
  },
  gradient6: {
    style: {
      backgroundImage: [
        'radial-gradient(circle at 22% 80%, rgba(0,212,255,0.22) 0%, rgba(0,212,255,0) 72%)',
        'radial-gradient(circle at 78% 22%, rgba(255,228,94,0.26) 0%, rgba(255,228,94,0) 66%)',
        'linear-gradient(48deg, #00d4ff 0%, #ffe45e 50%, #ff3d81 100%)',
      ].join(','),
    },
  },
  gradient7: {
    style: {
      backgroundImage: [
        'radial-gradient(circle at 24% 78%, rgba(63,167,214,0.16) 0%, rgba(63,167,214,0) 72%)',
        'radial-gradient(circle at 78% 26%, rgba(242,158,109,0.16) 0%, rgba(242,158,109,0) 70%)',
        'linear-gradient(42deg, #3fa7d6 0%, #8d7ae6 52%, #f29e6d 100%)',
      ].join(','),
    },
  },
};

export interface DownloadableBg {
  id: string;
  preview: string;
  downloadUrl: string;
}

export const DOWNLOADABLE_BACKGROUNDS: ReadonlyArray<DownloadableBg> = downloadableBackgrounds;

export type BgDlState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'downloading'; progress: number }
  | { status: 'prewarming' }
  | { status: 'done'; ext: string; version: number }
  | { status: 'error'; message: string };

export const buildDownloadedBgUrl = (id: string, ext: string, version: number): string =>
  `/bg-downloaded/${id}.${ext}?v=${version}`;

export function useDownloadableBg(bg: DownloadableBg, setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>) {
  const [state, setState] = useState<BgDlState>({ status: 'checking' });
  const syncInFlightRef = useRef(false);
  const prewarmedUrlSetRef = useRef<Set<string>>(new Set());
  const prewarmInFlightUrlSetRef = useRef<Set<string>>(new Set());
  const pendingPostDownloadPrewarmRef = useRef(false);

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
              return { ...prev, backgroundType: 'gradient2', customBackground: undefined };
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
                })
                .catch((e) => {
                  pendingPostDownloadPrewarmRef.current = false;
                  console.warn('Failed to prewarm downloaded background after download:', e);
                });
            }
            const needsPrewarm =
              pendingPostDownloadPrewarmRef.current &&
              (!prewarmedUrlSetRef.current.has(syncedUrl) || prewarmInFlightUrlSetRef.current.has(syncedUrl));
            next = needsPrewarm
              ? { status: 'prewarming' }
              : { status: 'done', ext, version };
          } else {
            next = { status: 'idle' };
          }
        }
      }

      setState(prev => {
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
    setState({ status: 'downloading', progress: 0 });
    invoke('start_bg_download', { id: bg.id, url: bg.downloadUrl });
  }, [bg.id, bg.downloadUrl, state.status]);

  const selectBg = useCallback(async () => {
    if (state.status !== 'done') return;
    // Use protocol URL — served by the custom protocol handler from local app data
    const url = buildDownloadedBgUrl(bg.id, state.ext, state.version);
    if (!prewarmedUrlSetRef.current.has(url)) {
      setState({ status: 'prewarming' });
      try {
        await ensurePrewarmed(url);
      } catch (e) {
        console.warn('Failed to prewarm selected downloaded background:', e);
      } finally {
        setState({ status: 'done', ext: state.ext, version: state.version });
      }
    }
    setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: url }));
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
          return { ...prev, backgroundType: 'gradient2', customBackground: undefined };
        }
        return prev;
      });
    } catch (e) {
      console.error('Failed to delete downloaded background:', e);
    }
  }, [bg.id, setBackgroundConfig]);

  // Keep tile state synced even when downloads/deletes happen outside this panel
  useEffect(() => {
    let cancelled = false;
    const run = async () => {
      if (cancelled) return;
      await syncState();
    };

    run();
    const interval = setInterval(run, 500);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [syncState]);

  return { state, startDownload, selectBg, deleteBg };
}

// ============================================================================
// DownloadableBgButton
// ============================================================================
function DownloadableBgButton({ bg, backgroundConfig, setBackgroundConfig }: {
  bg: DownloadableBg;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
}) {
  const { state, startDownload, selectBg, deleteBg } = useDownloadableBg(bg, setBackgroundConfig);

  const isDownloaded = state.status === 'done';
  const isDownloading = state.status === 'downloading';
  const isPrewarming = state.status === 'prewarming';
  const progress = isDownloading ? (state as { status: 'downloading'; progress: number }).progress : 0;

  // Overlay opacity: keep some visible cover while download reaches 100% so the
  // spinner/progress remains visible until post-download prewarm finishes.
  const overlayOpacity = isDownloaded ? 0 : isDownloading ? Math.max(0.4, 1 - (progress / 100)) : 1;

  const handleClick = () => {
    if (isDownloaded) {
      selectBg();
    } else if (state.status === 'idle' || state.status === 'error') {
      startDownload();
    }
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    deleteBg();
  };

  // Check if this downloaded bg is currently selected
  const isSelected = isDownloaded && backgroundConfig.backgroundType === 'custom'
    && backgroundConfig.customBackground?.includes(`/bg-downloaded/${bg.id}.`);

  return (
    <button
      onClick={handleClick}
      title={
        isDownloading ? `Downloading... ${Math.round(progress)}%`
        : isPrewarming ? 'Preparing image for export...'
        : isDownloaded ? bg.id
        : state.status === 'error' ? `Error: ${(state as { status: 'error'; message: string }).message}. Click to retry.`
        : 'Click to download'
      }
      className={`downloadable-bg-btn aspect-square h-10 rounded-lg transition-all duration-150 relative overflow-hidden group ${
        isSelected
          ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
          : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
      }`}
    >
      {/* Preview image */}
      <img
        src={bg.preview}
        alt={bg.id}
        className="absolute inset-0 w-full h-full object-cover"
        draggable={false}
      />

      {/* Delete button (top-right, shown on hover when downloaded) */}
      {isDownloaded && (
        <div
          onClick={handleDelete}
          className="downloadable-bg-delete absolute top-0.5 right-0.5 w-3.5 h-3.5 rounded-sm bg-black/50 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer hover:bg-red-500/80 z-10"
          title="Delete downloaded file"
        >
          <Trash2 className="w-2 h-2 text-white" />
        </div>
      )}

      {/* Download overlay: opacity decreases as download progresses */}
      {overlayOpacity > 0 && (
        <div
          className="downloadable-bg-overlay absolute inset-0 flex items-center justify-center transition-opacity duration-200"
          style={{
            backgroundColor: `rgba(0, 0, 0, ${0.18 * overlayOpacity})`,
            backdropFilter: overlayOpacity > 0.3 ? 'blur(1px)' : 'none',
          }}
        >
          {isDownloading ? (
            <div className="download-progress-ring relative w-5 h-5">
              <svg viewBox="0 0 20 20" className="w-full h-full -rotate-90">
                <circle cx="10" cy="10" r="8" fill="none" stroke="rgba(255,255,255,0.2)" strokeWidth="2" />
                <circle
                  cx="10" cy="10" r="8" fill="none" stroke="white" strokeWidth="2"
                  strokeDasharray={`${(progress / 100) * 50.3} 50.3`}
                  strokeLinecap="round"
                />
              </svg>
            </div>
          ) : isPrewarming ? (
            <Loader2 className="w-3.5 h-3.5 text-white/85 animate-spin drop-shadow-sm" />
          ) : (
            <Download className="w-3.5 h-3.5 text-white/80 drop-shadow-sm" />
          )}
        </div>
      )}
    </button>
  );
}

// ============================================================================
// BackgroundPanel
// ============================================================================
export interface BackgroundPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  recentUploads: string[];
  onRemoveRecentUpload: (imageUrl: string) => void;
  onBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  isBackgroundUploadProcessing: boolean;
}

export function BackgroundPanel({
  backgroundConfig,
  setBackgroundConfig,
  recentUploads,
  onRemoveRecentUpload,
  onBackgroundUpload,
  isBackgroundUploadProcessing
}: BackgroundPanelProps) {
  const { t } = useSettings();
  return (
    <div className="background-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="background-controls space-y-3.5">
        <div className="video-size-field flex items-center gap-3">
          <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.videoSize}</span>
          <input type="range" min="50" max="100" value={backgroundConfig.scale}
            style={sv(backgroundConfig.scale, 50, 100)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, scale: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{backgroundConfig.scale}%</span>
        </div>
        <div className="roundness-field flex items-center gap-3">
          <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.roundness}</span>
          <input type="range" min="0" max="64" value={backgroundConfig.borderRadius}
            style={sv(backgroundConfig.borderRadius, 0, 64)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, borderRadius: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{backgroundConfig.borderRadius}px</span>
        </div>
        <div className="shadow-field flex items-center gap-3">
          <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.shadow}</span>
          <input type="range" min="0" max="100" value={backgroundConfig.shadow || 0}
            style={sv(backgroundConfig.shadow || 0, 0, 100)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, shadow: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{backgroundConfig.shadow || 0}px</span>
        </div>
        <div className="background-style-field">
          <label className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-2 block">{t.backgroundStyle}</label>
          <div className="background-presets-grid grid grid-cols-6 gap-2">
            {/* Upload button */}
            <label className={`background-upload-btn aspect-square h-10 rounded-lg transition-all duration-150 cursor-pointer ring-1 ring-[var(--glass-border)] relative overflow-hidden group bg-[var(--glass-bg)] ${
              isBackgroundUploadProcessing
                ? 'opacity-80 cursor-wait'
                : 'hover:ring-[var(--primary-color)]/40 hover:scale-105'
            }`}>
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" disabled={isBackgroundUploadProcessing} />
              <div className="upload-icon absolute inset-0 flex items-center justify-center">
                {isBackgroundUploadProcessing ? (
                  <Loader2 className="w-4 h-4 text-[var(--primary-color)] animate-spin" />
                ) : (
                  <svg className="w-4 h-4 text-[var(--on-surface-variant)] group-hover:text-[var(--primary-color)] transition-colors" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
                )}
              </div>
            </label>

            {/* Black */}
            <button
              onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: 'solid' }))}
              className={`bg-preset-black aspect-square h-10 rounded-lg transition-all duration-150 bg-[#0a0a0a] ${
                backgroundConfig.backgroundType === 'solid'
                  ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                  : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
              }`}
            />

            {/* White */}
            <button
              onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: 'white' }))}
              className={`bg-preset-white aspect-square h-10 rounded-lg transition-all duration-150 bg-white ${
                backgroundConfig.backgroundType === 'white'
                  ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                  : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
              }`}
            />

            {/* Gradients */}
            {Object.entries(GRADIENT_PRESETS).map(([key, gradient]) => (
              <button
                key={key}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: key as BackgroundConfig['backgroundType'] }))}
                style={gradient.style}
                className={`aspect-square h-10 rounded-lg transition-all duration-150 ${gradient.className ?? ''} ${
                  backgroundConfig.backgroundType === key
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              />
            ))}

            {DOWNLOADABLE_BACKGROUNDS.map(bg => (
              <DownloadableBgButton
                key={bg.id}
                bg={bg}
                backgroundConfig={backgroundConfig}
                setBackgroundConfig={setBackgroundConfig}
              />
            ))}

            {recentUploads.map((imageUrl, index) => (
              <button
                key={index}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: imageUrl }))}
                className={`uploaded-bg-btn aspect-square h-10 rounded-lg transition-all duration-150 relative overflow-hidden group ${
                  backgroundConfig.backgroundType === 'custom' && backgroundConfig.customBackground === imageUrl
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              >
                <img src={imageUrl} alt={`Upload ${index + 1}`} className="absolute inset-0 w-full h-full object-cover" />
                <div
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onRemoveRecentUpload(imageUrl);
                  }}
                  className="uploaded-bg-delete absolute top-0.5 right-0.5 w-3.5 h-3.5 rounded-sm bg-black/50 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer hover:bg-red-500/80 z-10"
                  title="Remove uploaded background"
                  aria-label="Remove uploaded background"
                >
                  <Trash2 className="w-2.5 h-2.5 text-white" />
                </div>
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
