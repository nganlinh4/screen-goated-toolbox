import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { Trash2, Download, Loader2 } from 'lucide-react';
import { BackgroundConfig } from '@/types/video';
import { Slider } from '@/components/ui/Slider';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';
import downloadableBackgrounds from '@/config/downloadable-backgrounds.json';

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
  const pendingAutoApplyRef = useRef(false);

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
          return { ...prev, backgroundType: 'gradient2', customBackground: undefined };
        }
        return prev;
      });
    } catch (e) {
      console.error('Failed to delete downloaded background:', e);
    }
  }, [bg.id, setBackgroundConfig]);

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

  const [isApplying, setIsApplying] = useState(false);
  const isDownloaded = state.status === 'done';
  const isDownloading = state.status === 'downloading';
  const isPrewarming = state.status === 'prewarming';
  const progress = isDownloading ? (state as { status: 'downloading'; progress: number }).progress : 0;
  const overlayOpacity = (isDownloaded && !isApplying) ? 0 : isDownloading ? Math.max(0.4, 1 - (progress / 100)) : 1;

  const handleClick = () => {
    if (isDownloaded) {
      setIsApplying(true);
      selectBg();
      setTimeout(() => setIsApplying(false), 0);
    } else if (state.status === 'idle' || state.status === 'error') {
      startDownload();
    }
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    deleteBg();
  };

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
      <img
        src={bg.preview}
        alt={bg.id}
        className="absolute inset-0 w-full h-full object-cover"
        draggable={false}
      />
      {isDownloaded && (
        <div
          onClick={handleDelete}
          className="downloadable-bg-delete absolute top-0.5 right-0.5 w-3.5 h-3.5 rounded-sm bg-black/50 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer hover:bg-red-500/80 z-10"
          title="Delete downloaded file"
        >
          <Trash2 className="w-2 h-2 text-white" />
        </div>
      )}
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
          ) : (isPrewarming || isApplying) ? (
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
  const [applyingKey, setApplyingKey] = useState<string | null>(null);
  const applyPreset = (key: string, update: Partial<BackgroundConfig>) => {
    setApplyingKey(key);
    setBackgroundConfig(prev => ({ ...prev, ...update }));
    setTimeout(() => setApplyingKey(null), 0);
  };
  return (
    <PanelCard className="background-panel">
      <div className="background-controls space-y-3.5">
        <SettingRow label={t.videoSize} valueDisplay={`${backgroundConfig.scale}%`} className="video-size-field">
          <Slider
            min={50} max={100} value={backgroundConfig.scale}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, scale: val }))}
          />
        </SettingRow>
        <SettingRow label={t.roundness} valueDisplay={`${backgroundConfig.borderRadius}px`} className="roundness-field">
          <Slider
            min={0} max={64} value={backgroundConfig.borderRadius}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, borderRadius: val }))}
          />
        </SettingRow>
        <SettingRow label={t.shadow} valueDisplay={`${backgroundConfig.shadow || 0}px`} className="shadow-field">
          <Slider
            min={0} max={100} value={backgroundConfig.shadow || 0}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, shadow: val }))}
          />
        </SettingRow>
        <div className="background-style-field">
          <label className="text-xs font-medium uppercase tracking-wide text-on-surface-variant mb-2 block">{t.backgroundStyle}</label>
          <div className="background-presets-grid grid grid-cols-7 gap-2">
            {/* Upload button */}
            <label className={`background-upload-btn aspect-square h-10 rounded-lg transition-all duration-150 cursor-pointer ring-1 ring-[var(--glass-border)] relative overflow-hidden group bg-glass-bg ${
              isBackgroundUploadProcessing
                ? 'opacity-80 cursor-wait'
                : 'hover:ring-[var(--primary-color)]/40 hover:scale-105'
            }`}>
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" disabled={isBackgroundUploadProcessing} />
              <div className="upload-icon absolute inset-0 flex items-center justify-center">
                {isBackgroundUploadProcessing ? (
                  <Loader2 className="w-4 h-4 text-[var(--primary-color)] animate-spin" />
                ) : (
                  <svg className="w-4 h-4 text-on-surface-variant group-hover:text-[var(--primary-color)] transition-colors" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
                )}
              </div>
            </label>

            {/* Black */}
            <button
              onClick={() => applyPreset('solid', { backgroundType: 'solid' })}
              className={`bg-preset-black aspect-square h-10 rounded-lg transition-all duration-150 bg-[#0a0a0a] relative overflow-hidden ${
                backgroundConfig.backgroundType === 'solid'
                  ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                  : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
              }`}
            >
              {applyingKey === 'solid' && <div className="absolute inset-0 flex items-center justify-center"><Loader2 className="w-3.5 h-3.5 text-white/85 animate-spin drop-shadow-sm" /></div>}
            </button>

            {/* White */}
            <button
              onClick={() => applyPreset('white', { backgroundType: 'white' })}
              className={`bg-preset-white aspect-square h-10 rounded-lg transition-all duration-150 bg-white relative overflow-hidden ${
                backgroundConfig.backgroundType === 'white'
                  ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                  : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
              }`}
            >
              {applyingKey === 'white' && <div className="absolute inset-0 flex items-center justify-center"><Loader2 className="w-3.5 h-3.5 text-gray-500/80 animate-spin drop-shadow-sm" /></div>}
            </button>

            {/* Gradients */}
            {Object.entries(GRADIENT_PRESETS).map(([key, gradient]) => (
              <button
                key={key}
                onClick={() => applyPreset(key, { backgroundType: key as BackgroundConfig['backgroundType'] })}
                style={gradient.style}
                className={`aspect-square h-10 rounded-lg transition-all duration-150 relative overflow-hidden ${gradient.className ?? ''} ${
                  backgroundConfig.backgroundType === key
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              >
                {applyingKey === key && <div className="absolute inset-0 flex items-center justify-center"><Loader2 className="w-3.5 h-3.5 text-white/85 animate-spin drop-shadow-sm" /></div>}
              </button>
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
                onClick={() => applyPreset(imageUrl, { backgroundType: 'custom', customBackground: imageUrl })}
                className={`uploaded-bg-btn aspect-square h-10 rounded-lg transition-all duration-150 relative overflow-hidden group ${
                  backgroundConfig.backgroundType === 'custom' && backgroundConfig.customBackground === imageUrl
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              >
                <img src={imageUrl} alt={`Upload ${index + 1}`} className="absolute inset-0 w-full h-full object-cover" />
                {applyingKey === imageUrl && <div className="absolute inset-0 flex items-center justify-center bg-black/20 z-20"><Loader2 className="w-3.5 h-3.5 text-white/85 animate-spin drop-shadow-sm" /></div>}
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
    </PanelCard>
  );
}
