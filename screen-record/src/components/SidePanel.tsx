import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { Trash2, AlignLeft, AlignCenter, AlignRight, Download } from 'lucide-react';
import { VideoSegment, BackgroundConfig, TextSegment } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';

function buildFontVariationCSS(vars?: TextSegment['style']['fontVariations']): string | undefined {
  const parts: string[] = [];
  if (vars?.wdth !== undefined && vars.wdth !== 100) parts.push(`'wdth' ${vars.wdth}`);
  if (vars?.slnt !== undefined && vars.slnt !== 0) parts.push(`'slnt' ${vars.slnt}`);
  if (vars?.ROND !== undefined && vars.ROND !== 0) parts.push(`'ROND' ${vars.ROND}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

/** Inline style for slider active track fill */
const sv = (v: number, min: number, max: number): React.CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as React.CSSProperties);
const CURSOR_ASSET_VERSION = `cursor-variants-runtime-${Date.now()}`;
const CURSOR_VARIANT_ROW_HEIGHT = 58;
const CURSOR_VARIANT_VIEWPORT_HEIGHT = 280;

// ============================================================================
// Types
// ============================================================================
export type ActivePanel = 'zoom' | 'background' | 'cursor' | 'blur' | 'text';

const GRADIENT_PRESETS: Record<string, { className?: string; style?: React.CSSProperties }> = {
  gradient1: { className: 'bg-gradient-to-r from-blue-600 to-violet-600' },
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

interface DownloadableBg {
  id: string;
  preview: string;
  downloadUrl: string;
}

const DOWNLOADABLE_BACKGROUNDS: DownloadableBg[] = [
  {
    id: 'warm-abstract',
    preview: '/bg-warm-abstract.svg',
    downloadUrl: 'https://photos.google.com/share/AF1QipNNQyeVrqxBdNmBkq9ILswizuj-RYJFNt5GlxJZ90Y6hx0okrVSLKSnmFFbX7j5Mg/photo/AF1QipPN4cVT1Rngl_wMHjLy1uWx0aiSyENSm8GWW3Ez?key=RV8tSXVJVGdfS1RIQUI0Q3RZZVhlTmw0WmhFZ2V3',
  },
  {
    id: 'cool-abstract',
    preview: '/bg-cool-abstract.svg',
    downloadUrl: 'https://photos.google.com/share/AF1QipNNQyeVrqxBdNmBkq9ILswizuj-RYJFNt5GlxJZ90Y6hx0okrVSLKSnmFFbX7j5Mg/photo/AF1QipNUuKkC-kKZKGQjJ7ga59EJY1d4YwYp0HVeuJ0L?key=RV8tSXVJVGdfS1RIQUI0Q3RZZVhlTmw0WmhFZ2V3',
  },
  {
    id: 'deep-abstract',
    preview: '/bg-deep-abstract.svg',
    downloadUrl: 'https://photos.google.com/share/AF1QipNNQyeVrqxBdNmBkq9ILswizuj-RYJFNt5GlxJZ90Y6hx0okrVSLKSnmFFbX7j5Mg/photo/AF1QipPufDAGMvOMDpTHKG574-ERmZxQN-CtcUCYnzKF?key=RV8tSXVJVGdfS1RIQUI0Q3RZZVhlTmw0WmhFZ2V3',
  },
  {
    id: 'vivid-abstract',
    preview: '/bg-vivid-abstract.svg',
    downloadUrl: 'https://drive.google.com/file/d/1Eq9-T4smcInRljcvAjBMKi_BAqtXn1Er/view',
  },
];

type BgDlState = { status: 'idle' } | { status: 'checking' } | { status: 'downloading'; progress: number } | { status: 'done'; ext: string } | { status: 'error'; message: string };

function useDownloadableBg(bg: DownloadableBg, setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>) {
  const [state, setState] = useState<BgDlState>({ status: 'checking' });
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Check on mount if already downloaded
  useEffect(() => {
    invoke<{ downloaded: boolean; ext: string | null }>('check_bg_downloaded', { id: bg.id }).then(res => {
      setState(res.downloaded && res.ext ? { status: 'done', ext: res.ext } : { status: 'idle' });
    }).catch(() => setState({ status: 'idle' }));
  }, [bg.id]);

  const startDownload = useCallback(() => {
    if (state.status === 'downloading') return;
    setState({ status: 'downloading', progress: 0 });
    invoke('start_bg_download', { id: bg.id, url: bg.downloadUrl });

    pollRef.current = setInterval(async () => {
      try {
        const progress = await invoke<any>('get_bg_download_progress');
        if (progress === 'Idle') {
          setState({ status: 'idle' });
        } else if (progress === 'Done') {
          // Fetch the extension of the downloaded file
          const info = await invoke<{ downloaded: boolean; ext: string | null }>('check_bg_downloaded', { id: bg.id });
          setState({ status: 'done', ext: info.ext ?? 'png' });
          if (pollRef.current) clearInterval(pollRef.current);
        } else if (typeof progress === 'object') {
          if ('Downloading' in progress) {
            setState({ status: 'downloading', progress: progress.Downloading.progress });
          } else if ('Error' in progress) {
            setState({ status: 'error', message: progress.Error });
            if (pollRef.current) clearInterval(pollRef.current);
          }
        }
      } catch {
        // keep polling
      }
    }, 150);
  }, [bg.id, bg.downloadUrl, state.status]);

  const selectBg = useCallback(() => {
    if (state.status !== 'done') return;
    // Use protocol URL — served by the custom protocol handler from local app data
    const url = `/bg-downloaded/${bg.id}.${state.ext}`;
    setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: url }));
  }, [bg.id, state, setBackgroundConfig]);

  const deleteBg = useCallback(async () => {
    try {
      await invoke('delete_bg_download', { id: bg.id });
      setState({ status: 'idle' });
    } catch (e) {
      console.error('Failed to delete downloaded background:', e);
    }
  }, [bg.id]);

  // Cleanup interval on unmount
  useEffect(() => {
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, []);

  return { state, startDownload, selectBg, deleteBg };
}

// ============================================================================
// PanelTabs
// ============================================================================
interface PanelTabsProps {
  activePanel: ActivePanel;
  onPanelChange: (panel: ActivePanel) => void;
}

function PanelTabs({ activePanel, onPanelChange }: PanelTabsProps) {
  const { t } = useSettings();
  const tabs: { id: ActivePanel; label: string }[] = [
    { id: 'zoom', label: t.tabZoom },
    { id: 'background', label: t.tabBackground },
    { id: 'cursor', label: t.tabCursor },
    { id: 'blur', label: t.tabBlur },
    { id: 'text', label: t.tabText }
  ];

  return (
    <div className="panel-tabs flex border-b border-[var(--glass-border)]">
      {tabs.map(tab => (
        <button
          key={tab.id}
          onClick={() => onPanelChange(tab.id)}
          className={`flex-1 px-2 py-2 text-xs font-medium transition-colors relative ${
            activePanel === tab.id
              ? 'text-[var(--primary-color)]'
              : 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'
          }`}
        >
          {tab.label}
          {activePanel === tab.id && (
            <div className="tab-indicator absolute bottom-0 left-1/4 right-1/4 h-[2px] bg-[var(--primary-color)] rounded-full" />
          )}
        </button>
      ))}
    </div>
  );
}

// ============================================================================
// ZoomPanel
// ============================================================================
interface ZoomPanelProps {
  segment: VideoSegment | null;
  editingKeyframeId: number | null;
  zoomFactor: number;
  setZoomFactor: (value: number) => void;
  onDeleteKeyframe: () => void;
  onUpdateZoom: (updates: { zoomFactor?: number; positionX?: number; positionY?: number }) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

function ZoomPanel({
  segment,
  editingKeyframeId,
  zoomFactor,
  setZoomFactor,
  onDeleteKeyframe,
  onUpdateZoom,
  beginBatch,
  commitBatch
}: ZoomPanelProps) {
  const { t } = useSettings();
  if (editingKeyframeId !== null && segment) {
    const keyframe = segment.zoomKeyframes[editingKeyframeId];
    if (!keyframe) return null;

    return (
      <div className="zoom-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
        <div className="panel-header flex justify-between items-center mb-3">
          <h2 className="panel-title text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)]">{t.zoomConfiguration}</h2>
          <Button
            onClick={onDeleteKeyframe}
            variant="ghost"
            size="icon"
            className="text-[var(--on-surface-variant)] hover:text-[var(--tertiary-color)] hover:bg-[var(--tertiary-color)]/10 transition-colors"
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
        <div className="zoom-controls space-y-3">
          <div className="zoom-factor-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2">{t.zoomFactor}</label>
            <div className="space-y-2">
              <input
                type="range"
                min="1"
                max="3"
                step="0.1"
                value={zoomFactor}
                style={sv(zoomFactor, 1, 3)}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(e) => {
                  const newValue = Number(e.target.value);
                  setZoomFactor(newValue);
                  onUpdateZoom({ zoomFactor: newValue });
                }}
                className="w-full"
              />
              <div className="zoom-range-labels flex justify-between text-[10px] text-[var(--on-surface-variant)]">
                <span>1x</span>
                <span className="text-[var(--on-surface)]">{zoomFactor.toFixed(1)}x</span>
                <span>3x</span>
              </div>
            </div>
          </div>
          <div className="position-controls space-y-3">
            <div className="position-x-field">
              <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
                <span>{t.horizontalPosition}</span>
                <span>{Math.round((keyframe?.positionX ?? 0.5) * 100)}%</span>
              </label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={keyframe?.positionX ?? 0.5}
                style={sv(keyframe?.positionX ?? 0.5, 0, 1)}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(e) => onUpdateZoom({ positionX: Number(e.target.value) })}
                className="w-full"
              />
            </div>
            <div className="position-y-field">
              <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
                <span>{t.verticalPosition}</span>
                <span>{Math.round((keyframe?.positionY ?? 0.5) * 100)}%</span>
              </label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={keyframe?.positionY ?? 0.5}
                style={sv(keyframe?.positionY ?? 0.5, 0, 1)}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(e) => onUpdateZoom({ positionY: Number(e.target.value) })}
                className="w-full"
              />
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="zoom-panel-hint bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-4 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <p className="text-xs text-[var(--on-surface-variant)]">{t.zoomHint}</p>
    </div>
  );
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
  const progress = isDownloading ? (state as { status: 'downloading'; progress: number }).progress : 0;

  // Overlay opacity: 1 when not downloaded, wipes to 0 as download progresses
  const overlayOpacity = isDownloaded ? 0 : isDownloading ? 1 - (progress / 100) : 1;

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
interface BackgroundPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  recentUploads: string[];
  onRemoveRecentUpload: (imageUrl: string) => void;
  onBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
}

function BackgroundPanel({
  backgroundConfig,
  setBackgroundConfig,
  recentUploads,
  onRemoveRecentUpload,
  onBackgroundUpload,
  canvasRef
}: BackgroundPanelProps) {
  const { t } = useSettings();
  return (
    <div className="background-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="background-controls space-y-3">
        {/* Canvas Size */}
        <div className="canvas-size-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.canvasSize}</label>
          <div className="canvas-mode-toggle flex rounded-lg border border-[var(--glass-border)] overflow-hidden mb-2">
            {(['auto', 'custom'] as const).map(mode => {
              const isActive = (backgroundConfig.canvasMode ?? 'auto') === mode;
              return (
                <button
                  key={mode}
                  onClick={() => {
                    if (mode === 'custom') {
                      setBackgroundConfig(prev => {
                        const w = prev.canvasWidth ?? canvasRef.current?.width ?? 1920;
                        const h = prev.canvasHeight ?? canvasRef.current?.height ?? 1080;
                        return { ...prev, canvasMode: 'custom', canvasWidth: w, canvasHeight: h };
                      });
                    } else {
                      setBackgroundConfig(prev => ({ ...prev, canvasMode: 'auto' }));
                    }
                  }}
                  className={`canvas-mode-btn flex-1 px-2 py-1 text-[10px] font-medium transition-colors ${
                    isActive
                      ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]'
                      : 'bg-[var(--glass-bg)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'
                  }`}
                >
                  {mode === 'auto' ? t.canvasAuto : t.canvasCustom}
                </button>
              );
            })}
          </div>
          {(backgroundConfig.canvasMode ?? 'auto') === 'custom' && (
            <p className="canvas-dimensions-label text-[10px] text-[var(--on-surface-variant)] text-center tabular-nums">
              {backgroundConfig.canvasWidth ?? 1920} x {backgroundConfig.canvasHeight ?? 1080}
            </p>
          )}
        </div>

        <div className="video-size-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.videoSize}</span>
            <span>{backgroundConfig.scale}%</span>
          </label>
          <input type="range" min="50" max="100" value={backgroundConfig.scale}
            style={sv(backgroundConfig.scale, 50, 100)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, scale: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div className="roundness-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.roundness}</span>
            <span>{backgroundConfig.borderRadius}px</span>
          </label>
          <input type="range" min="0" max="64" value={backgroundConfig.borderRadius}
            style={sv(backgroundConfig.borderRadius, 0, 64)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, borderRadius: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div className="shadow-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.shadow}</span>
            <span>{backgroundConfig.shadow || 0}px</span>
          </label>
          <input type="range" min="0" max="100" value={backgroundConfig.shadow || 0}
            style={sv(backgroundConfig.shadow || 0, 0, 100)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, shadow: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div className="volume-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.volume}</span>
            <span>{Math.round((backgroundConfig.volume ?? 1) * 100)}%</span>
          </label>
          <input type="range" min="0" max="1" step="0.01" value={backgroundConfig.volume ?? 1}
            style={sv(backgroundConfig.volume ?? 1, 0, 1)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, volume: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div className="background-style-field">
          <label className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-2 block">{t.backgroundStyle}</label>
          <div className="background-presets-grid grid grid-cols-6 gap-2">
            {/* Upload button */}
            <label className="background-upload-btn aspect-square h-10 rounded-lg transition-all duration-150 cursor-pointer ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105 relative overflow-hidden group bg-[var(--glass-bg)]">
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" />
              <div className="upload-icon absolute inset-0 flex items-center justify-center">
                <svg className="w-4 h-4 text-[var(--on-surface-variant)] group-hover:text-[var(--primary-color)] transition-colors" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
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

// ============================================================================
// CursorPanel
// ============================================================================
interface CursorPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
}

interface CursorVariantButtonProps {
  isSelected: boolean;
  onClick: () => void;
  label: string;
  children: React.ReactNode;
}

type CursorVariant = 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';
type CursorVariantRow = {
  id: string;
  label: string;
  screenstudioSrc: string;
  macos26Src: string;
  sgtcuteSrc: string;
  sgtcoolSrc: string;
  sgtaiSrc: string;
  sgtpixelSrc: string;
  jepriwin11Src: string;
};

function CursorVariantButton({ isSelected, onClick, label, children }: CursorVariantButtonProps) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      className={`cursor-variant-button w-full min-w-0 h-10 rounded-[10px] border transition-all duration-150 flex items-center justify-center overflow-hidden ${
        isSelected
          ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/14 shadow-[0_0_0_1px_var(--primary-color)_inset,0_0_0_3px_rgba(59,130,246,0.16),0_6px_16px_rgba(59,130,246,0.2)]'
          : 'border-[var(--glass-border)] bg-[var(--glass-bg)] hover:border-[var(--primary-color)]/65 hover:bg-[var(--glass-bg-hover)]'
      }`}
    >
      {children}
    </button>
  );
}

function CursorPanel({ backgroundConfig, setBackgroundConfig }: CursorPanelProps) {
  const { t } = useSettings();
  const [variantScrollTop, setVariantScrollTop] = useState(0);
  const inferredPack: CursorVariant =
    backgroundConfig.cursorPack
    ?? backgroundConfig.cursorDefaultVariant
    ?? backgroundConfig.cursorTextVariant
    ?? backgroundConfig.cursorPointerVariant
    ?? backgroundConfig.cursorOpenHandVariant
    ?? 'screenstudio';
  const setCursorPack = (pack: CursorVariant) =>
    setBackgroundConfig(prev => ({
      ...prev,
      cursorPack: pack,
      cursorDefaultVariant: pack,
      cursorTextVariant: pack,
      cursorPointerVariant: pack,
      cursorOpenHandVariant: pack,
    }));
  const rows = useMemo<CursorVariantRow[]>(() => ([
    { id: 'default', label: t.cursorDefault, screenstudioSrc: '/cursor-default-screenstudio.svg', macos26Src: '/cursor-default-macos26.svg', sgtcuteSrc: '/cursor-default-sgtcute.svg', sgtcoolSrc: '/cursor-default-sgtcool.svg', sgtaiSrc: '/cursor-default-sgtai.svg', sgtpixelSrc: '/cursor-default-sgtpixel.svg', jepriwin11Src: '/cursor-default-jepriwin11.svg' },
    { id: 'text', label: t.cursorText, screenstudioSrc: '/cursor-text-screenstudio.svg', macos26Src: '/cursor-text-macos26.svg', sgtcuteSrc: '/cursor-text-sgtcute.svg', sgtcoolSrc: '/cursor-text-sgtcool.svg', sgtaiSrc: '/cursor-text-sgtai.svg', sgtpixelSrc: '/cursor-text-sgtpixel.svg', jepriwin11Src: '/cursor-text-jepriwin11.svg' },
    { id: 'pointer', label: t.cursorPointer, screenstudioSrc: '/cursor-pointer-screenstudio.svg', macos26Src: '/cursor-pointer-macos26.svg', sgtcuteSrc: '/cursor-pointer-sgtcute.svg', sgtcoolSrc: '/cursor-pointer-sgtcool.svg', sgtaiSrc: '/cursor-pointer-sgtai.svg', sgtpixelSrc: '/cursor-pointer-sgtpixel.svg', jepriwin11Src: '/cursor-pointer-jepriwin11.svg' },
    { id: 'openhand', label: t.cursorOpenHand, screenstudioSrc: '/cursor-openhand-screenstudio.svg', macos26Src: '/cursor-openhand-macos26.svg', sgtcuteSrc: '/cursor-openhand-sgtcute.svg', sgtcoolSrc: '/cursor-openhand-sgtcool.svg', sgtaiSrc: '/cursor-openhand-sgtai.svg', sgtpixelSrc: '/cursor-openhand-sgtpixel.svg', jepriwin11Src: '/cursor-openhand-jepriwin11.svg' },
    { id: 'closehand', label: 'Closed Hand', screenstudioSrc: '/cursor-closehand-screenstudio.svg', macos26Src: '/cursor-closehand-macos26.svg', sgtcuteSrc: '/cursor-closehand-sgtcute.svg', sgtcoolSrc: '/cursor-closehand-sgtcool.svg', sgtaiSrc: '/cursor-closehand-sgtai.svg', sgtpixelSrc: '/cursor-closehand-sgtpixel.svg', jepriwin11Src: '/cursor-closehand-jepriwin11.svg' },
    { id: 'wait', label: 'Wait', screenstudioSrc: '/cursor-wait-screenstudio.svg', macos26Src: '/cursor-wait-macos26.svg', sgtcuteSrc: '/cursor-wait-sgtcute.svg', sgtcoolSrc: '/cursor-wait-sgtcool.svg', sgtaiSrc: '/cursor-wait-sgtai.svg', sgtpixelSrc: '/cursor-wait-sgtpixel.svg', jepriwin11Src: '/cursor-wait-jepriwin11.svg' },
    { id: 'appstarting', label: 'App Starting', screenstudioSrc: '/cursor-appstarting-screenstudio.svg', macos26Src: '/cursor-appstarting-macos26.svg', sgtcuteSrc: '/cursor-appstarting-sgtcute.svg', sgtcoolSrc: '/cursor-appstarting-sgtcool.svg', sgtaiSrc: '/cursor-appstarting-sgtai.svg', sgtpixelSrc: '/cursor-appstarting-sgtpixel.svg', jepriwin11Src: '/cursor-appstarting-jepriwin11.svg' },
    { id: 'crosshair', label: 'Crosshair', screenstudioSrc: '/cursor-crosshair-screenstudio.svg', macos26Src: '/cursor-crosshair-macos26.svg', sgtcuteSrc: '/cursor-crosshair-sgtcute.svg', sgtcoolSrc: '/cursor-crosshair-sgtcool.svg', sgtaiSrc: '/cursor-crosshair-sgtai.svg', sgtpixelSrc: '/cursor-crosshair-sgtpixel.svg', jepriwin11Src: '/cursor-crosshair-jepriwin11.svg' },
    { id: 'resize_ns', label: 'Resize N-S', screenstudioSrc: '/cursor-resize-ns-screenstudio.svg', macos26Src: '/cursor-resize-ns-macos26.svg', sgtcuteSrc: '/cursor-resize-ns-sgtcute.svg', sgtcoolSrc: '/cursor-resize-ns-sgtcool.svg', sgtaiSrc: '/cursor-resize-ns-sgtai.svg', sgtpixelSrc: '/cursor-resize-ns-sgtpixel.svg', jepriwin11Src: '/cursor-resize-ns-jepriwin11.svg' },
    { id: 'resize_we', label: 'Resize W-E', screenstudioSrc: '/cursor-resize-we-screenstudio.svg', macos26Src: '/cursor-resize-we-macos26.svg', sgtcuteSrc: '/cursor-resize-we-sgtcute.svg', sgtcoolSrc: '/cursor-resize-we-sgtcool.svg', sgtaiSrc: '/cursor-resize-we-sgtai.svg', sgtpixelSrc: '/cursor-resize-we-sgtpixel.svg', jepriwin11Src: '/cursor-resize-we-jepriwin11.svg' },
    { id: 'resize_nwse', label: 'Resize NW-SE', screenstudioSrc: '/cursor-resize-nwse-screenstudio.svg', macos26Src: '/cursor-resize-nwse-macos26.svg', sgtcuteSrc: '/cursor-resize-nwse-sgtcute.svg', sgtcoolSrc: '/cursor-resize-nwse-sgtcool.svg', sgtaiSrc: '/cursor-resize-nwse-sgtai.svg', sgtpixelSrc: '/cursor-resize-nwse-sgtpixel.svg', jepriwin11Src: '/cursor-resize-nwse-jepriwin11.svg' },
    { id: 'resize_nesw', label: 'Resize NE-SW', screenstudioSrc: '/cursor-resize-nesw-screenstudio.svg', macos26Src: '/cursor-resize-nesw-macos26.svg', sgtcuteSrc: '/cursor-resize-nesw-sgtcute.svg', sgtcoolSrc: '/cursor-resize-nesw-sgtcool.svg', sgtaiSrc: '/cursor-resize-nesw-sgtai.svg', sgtpixelSrc: '/cursor-resize-nesw-sgtpixel.svg', jepriwin11Src: '/cursor-resize-nesw-jepriwin11.svg' },
  ]), [t.cursorDefault, t.cursorText, t.cursorPointer, t.cursorOpenHand]);
  const viewportHeight = CURSOR_VARIANT_VIEWPORT_HEIGHT;
  const totalHeight = rows.length * CURSOR_VARIANT_ROW_HEIGHT;
  const startIndex = Math.max(0, Math.floor(variantScrollTop / CURSOR_VARIANT_ROW_HEIGHT) - 2);
  const visibleCount = Math.ceil(viewportHeight / CURSOR_VARIANT_ROW_HEIGHT) + 4;
  const endIndex = Math.min(rows.length, startIndex + visibleCount);
  const visibleRows = rows.slice(startIndex, endIndex);
  return (
    <div className="cursor-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="cursor-controls space-y-2">
        <div className="cursor-size-field flex items-center gap-2">
          <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.cursorSize}</span>
          <input type="range" min="1" max="8" step="0.1" value={backgroundConfig.cursorScale ?? 2}
            style={sv(backgroundConfig.cursorScale ?? 2, 1, 8)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorScale: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-10 text-right flex-shrink-0">{(backgroundConfig.cursorScale ?? 2).toFixed(1)}x</span>
        </div>
        <div className="cursor-shadow-field flex items-center gap-2">
          <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">Shadow</span>
          <input type="range" min="0" max="200" step="1" value={backgroundConfig.cursorShadow ?? 35}
            style={sv(backgroundConfig.cursorShadow ?? 35, 0, 200)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorShadow: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-10 text-right flex-shrink-0">{Math.round(backgroundConfig.cursorShadow ?? 35)}%</span>
        </div>
        <div className="cursor-smoothness-field flex items-center gap-2">
          <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.movementSmoothing}</span>
          <input type="range" min="0" max="10" step="1" value={backgroundConfig.cursorSmoothness ?? 5}
            style={sv(backgroundConfig.cursorSmoothness ?? 5, 0, 10)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorSmoothness: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-10 text-right flex-shrink-0">{backgroundConfig.cursorSmoothness ?? 5}</span>
        </div>
        <div className="cursor-movement-delay-field flex items-center gap-2">
          <span className="cursor-movement-delay-label text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.pointerMovementDelay}</span>
          <input
            type="range"
            min="0"
            max="0.5"
            step="0.01"
            value={backgroundConfig.cursorMovementDelay ?? 0.03}
            style={sv(backgroundConfig.cursorMovementDelay ?? 0.03, 0, 0.5)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorMovementDelay: Number(e.target.value) }))}
            className="cursor-movement-delay-slider flex-1 min-w-0"
          />
          <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-10 text-right flex-shrink-0">{(backgroundConfig.cursorMovementDelay ?? 0.03).toFixed(2)}s</span>
        </div>
        <div className="cursor-wiggle-strength-field flex items-center gap-2">
          <span className="cursor-wiggle-strength-label text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.pointerWiggleStrength}</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={backgroundConfig.cursorWiggleStrength ?? 0.30}
            style={sv(backgroundConfig.cursorWiggleStrength ?? 0.30, 0, 1)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorWiggleStrength: Number(e.target.value) }))}
            className="cursor-wiggle-strength-slider flex-1 min-w-0"
          />
          <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-10 text-right flex-shrink-0">{Math.round((backgroundConfig.cursorWiggleStrength ?? 0.30) * 100)}%</span>
        </div>
        <div className="cursor-tilt-angle-field flex items-center gap-2">
          <span className="cursor-tilt-angle-label text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.cursorTilt}</span>
          <input
            type="range"
            min="-30"
            max="30"
            step="1"
            value={backgroundConfig.cursorTiltAngle ?? -10}
            style={sv(backgroundConfig.cursorTiltAngle ?? -10, -30, 30)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorTiltAngle: Number(e.target.value) }))}
            className="cursor-tilt-angle-slider flex-1 min-w-0"
          />
          <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-10 text-right flex-shrink-0">{backgroundConfig.cursorTiltAngle ?? -10}°</span>
        </div>
        <div className="cursor-variants-section space-y-2">
          <label className="cursor-variants-label text-xs text-[var(--on-surface-variant)] block">{t.cursorVariants}</label>
          <div
            className="cursor-variant-virtualized-list border border-[var(--glass-border)] rounded-lg overflow-hidden"
            style={{ height: `${viewportHeight}px` }}
          >
            <div
              className="cursor-variant-virtualized-scroll thin-scrollbar h-full overflow-y-auto"
              onScroll={(e) => setVariantScrollTop(e.currentTarget.scrollTop)}
            >
              <div className="cursor-variant-column-header sticky top-0 z-10 min-h-8 py-1 px-1.5 border-b border-[var(--glass-border)] grid grid-cols-7 gap-1.5 items-start bg-[var(--surface)]">
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  Mac OG
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  Mac Tahoe+
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  SGT Cute
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  SGT Cool
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  SGT AI
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  SGT Pixel
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 0" }}
                >
                  Jepri Win11
                </span>
              </div>
              <div className="cursor-variant-virtualized-inner relative" style={{ height: `${totalHeight}px` }}>
                {visibleRows.map((row, i) => {
                  const absoluteIndex = startIndex + i;
                  const tiltDeg = backgroundConfig.cursorTiltAngle ?? -10;
                  const hasTilt = (row.id === 'default' || row.id === 'pointer') && Math.abs(tiltDeg) > 0.5;
                  const tiltStyle = hasTilt ? { rotate: `${tiltDeg}deg` } as React.CSSProperties : undefined;
                  return (
                    <div
                      key={row.id}
                      className="cursor-variant-row absolute left-0 right-0 px-1.5 grid grid-cols-7 gap-1.5 items-center"
                      style={{ top: `${absoluteIndex * CURSOR_VARIANT_ROW_HEIGHT}px`, height: `${CURSOR_VARIANT_ROW_HEIGHT}px` }}
                    >
                      <CursorVariantButton
                        isSelected={inferredPack === 'screenstudio'}
                        onClick={() => setCursorPack('screenstudio')}
                        label={`${row.label} screen studio`}
                      >
                        <img src={`${row.screenstudioSrc}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                      <CursorVariantButton
                        isSelected={inferredPack === 'macos26'}
                        onClick={() => setCursorPack('macos26')}
                        label={`${row.label} macos26`}
                      >
                        <img src={`${row.macos26Src}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                      <CursorVariantButton
                        isSelected={inferredPack === 'sgtcute'}
                        onClick={() => setCursorPack('sgtcute')}
                        label={`${row.label} sgtcute`}
                      >
                        <img src={`${row.sgtcuteSrc}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                      <CursorVariantButton
                        isSelected={inferredPack === 'sgtcool'}
                        onClick={() => setCursorPack('sgtcool')}
                        label={`${row.label} sgtcool`}
                      >
                        <img src={`${row.sgtcoolSrc}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                      <CursorVariantButton
                        isSelected={inferredPack === 'sgtai'}
                        onClick={() => setCursorPack('sgtai')}
                        label={`${row.label} sgtai`}
                      >
                        <img src={`${row.sgtaiSrc}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                      <CursorVariantButton
                        isSelected={inferredPack === 'sgtpixel'}
                        onClick={() => setCursorPack('sgtpixel')}
                        label={`${row.label} sgtpixel`}
                      >
                        <img src={`${row.sgtpixelSrc}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                      <CursorVariantButton
                        isSelected={inferredPack === 'jepriwin11'}
                        onClick={() => setCursorPack('jepriwin11')}
                        label={`${row.label} jepriwin11`}
                      >
                        <img src={`${row.jepriwin11Src}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                      </CursorVariantButton>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// TextPanel
// ============================================================================
interface TextPanelProps {
  segment: VideoSegment | null;
  editingTextId: string | null;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

// ============================================================================
// BlurPanel
// ============================================================================
interface BlurPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
}

function BlurPanel({ backgroundConfig, setBackgroundConfig }: BlurPanelProps) {
  const { t } = useSettings();
  return (
    <div className="blur-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="blur-controls space-y-3">
        <div className="blur-sliders space-y-1.5">
          {([
            ['motionBlurCursor', t.motionBlurCursor, 25] as const,
            ['motionBlurZoom', t.motionBlurZoom, 10] as const,
            ['motionBlurPan', t.motionBlurPan, 10] as const,
          ]).map(([key, label, def]) => (
            <div key={key} className={`motion-blur-slider-${key} flex items-center gap-2`}>
              <span className="text-[10px] text-[var(--on-surface-variant)] w-24 flex-shrink-0">{label}</span>
              <input
                type="range" min="0" max="100" step="1"
                value={backgroundConfig[key] ?? def}
                style={sv(backgroundConfig[key] ?? def, 0, 100)}
                onChange={(e) => setBackgroundConfig(prev => ({ ...prev, [key]: Number(e.target.value) }))}
                className="motion-blur-range flex-1 min-w-0"
              />
              <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-8 text-right flex-shrink-0">{backgroundConfig[key] ?? def}%</span>
            </div>
          ))}
        </div>
        <p className="blur-gpu-note text-[10px] text-[var(--on-surface-variant)] leading-relaxed opacity-70">
          {t.motionBlurNote}
        </p>
      </div>
    </div>
  );
}

// ============================================================================
// TextPanel
// ============================================================================
function TextPanel({ segment, editingTextId, onUpdateSegment, beginBatch, commitBatch }: TextPanelProps) {
  const { t } = useSettings();
  const editingText = editingTextId ? segment?.textSegments?.find(ts => ts.id === editingTextId) : null;

  const updateStyle = (updates: Partial<TextSegment['style']>) => {
    if (!segment || !editingTextId) return;
    onUpdateSegment({
      ...segment,
      textSegments: segment.textSegments.map(ts =>
        ts.id === editingTextId ? { ...ts, style: { ...ts.style, ...updates } } : ts
      )
    });
  };

  return (
    <div className="text-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      {editingText && segment ? (
        <div className="text-controls space-y-2">
          <textarea
            value={editingText.text}
            onFocus={beginBatch}
            onBlur={commitBatch}
            onChange={(e) => {
              onUpdateSegment({
                ...segment,
                textSegments: segment.textSegments.map(ts =>
                  ts.id === editingTextId ? { ...ts, text: e.target.value } : ts
                )
              });
            }}
            className="w-full bg-[var(--glass-bg)] border border-[var(--glass-border)] rounded-lg px-3 py-2 text-[var(--on-surface)] text-sm focus:border-[var(--primary-color)]/50 focus:ring-1 focus:ring-[var(--primary-color)]/30 transition-colors thin-scrollbar subtle-resize"
            style={{
              fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
              fontWeight: editingText.style.fontVariations?.wght ?? 400,
              fontVariationSettings: buildFontVariationCSS(editingText.style.fontVariations),
            }}
            rows={2}
          />

          <p className="text-[10px] text-[var(--on-surface-variant)]">{t.dragTextHint}</p>

          {/* Font Size */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.fontSize}</span>
            <input
              type="range" min={12} max={200} step={1}
              value={editingText.style.fontSize}
              style={sv(editingText.style.fontSize, 12, 200)}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(e) => updateStyle({ fontSize: Number(e.target.value) })}
              className="flex-1 min-w-0"
            />
            <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-7 text-right flex-shrink-0">{editingText.style.fontSize}</span>
          </div>

          {/* Color */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.color}</span>
            <ColorPicker
              value={editingText.style.color}
              onChange={(color) => updateStyle({ color })}
              onOpen={beginBatch}
              onClose={commitBatch}
            />
          </div>

          {/* Font Variation Axes */}
          {([
            { axis: 'wght', label: t.fontWeight, min: 100, max: 900, defaultVal: 400, step: 1 },
            { axis: 'wdth', label: t.fontWidth, min: 75, max: 125, defaultVal: 100, step: 1 },
            { axis: 'slnt', label: t.fontSlant, min: -12, max: 0, defaultVal: 0, step: 1 },
            { axis: 'ROND', label: t.fontRound, min: 0, max: 100, defaultVal: 0, step: 1 },
          ] as const).map(({ axis, label, min, max, defaultVal, step }) => {
            const value = (editingText.style.fontVariations as any)?.[axis] ?? defaultVal;
            return (
              <div key={axis} className="flex items-center gap-2">
                <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{label}</span>
                <input
                  type="range" min={min} max={max} step={step}
                  value={value}
                  style={sv(value, min, max)}
                  onPointerDown={beginBatch}
                  onPointerUp={commitBatch}
                  onChange={(e) => updateStyle({
                    fontVariations: { ...(editingText.style.fontVariations || {}), [axis]: Number(e.target.value) }
                  })}
                  className="flex-1 min-w-0"
                />
                <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-7 text-right flex-shrink-0">{value}</span>
              </div>
            );
          })}

          {/* Alignment */}
          <div className="text-align-field flex items-center gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.textAlignment}</span>
            <div className="alignment-button-group flex rounded-lg border border-[var(--glass-border)] overflow-hidden">
              {(['left', 'center', 'right'] as const).map(align => {
                const Icon = align === 'left' ? AlignLeft : align === 'center' ? AlignCenter : AlignRight;
                const isActive = (editingText.style.textAlign ?? 'center') === align;
                return (
                  <button
                    key={align}
                    onClick={() => updateStyle({ textAlign: align })}
                    className={`flex items-center justify-center w-7 h-7 transition-colors ${
                      isActive
                        ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]'
                        : 'bg-[var(--glass-bg)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)]'
                    }`}
                    title={align}
                  >
                    <Icon className="w-3.5 h-3.5" />
                  </button>
                );
              })}
            </div>
          </div>

          {/* Opacity */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.opacity}</span>
            <input
              type="range" min="0" max="1" step="0.01"
              value={editingText.style.opacity ?? 1}
              style={sv(editingText.style.opacity ?? 1, 0, 1)}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(e) => updateStyle({ opacity: Number(e.target.value) })}
              className="flex-1 min-w-0"
            />
            <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-7 text-right flex-shrink-0">{Math.round((editingText.style.opacity ?? 1) * 100)}%</span>
          </div>

          {/* Letter Spacing */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.letterSpacing}</span>
            <input
              type="range" min="-5" max="20" step="1"
              value={editingText.style.letterSpacing ?? 0}
              style={sv(editingText.style.letterSpacing ?? 0, -5, 20)}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(e) => updateStyle({ letterSpacing: Number(e.target.value) })}
              className="flex-1 min-w-0"
            />
            <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-7 text-right flex-shrink-0">{editingText.style.letterSpacing ?? 0}</span>
          </div>

          {/* Background Pill */}
          <div>
            <label className="flex items-center gap-2 text-[10px] text-[var(--on-surface-variant)] cursor-pointer">
              <input
                type="checkbox"
                checked={editingText.style.background?.enabled ?? false}
                onChange={(e) => updateStyle({
                  background: {
                    enabled: e.target.checked,
                    color: editingText.style.background?.color ?? '#000000',
                    opacity: editingText.style.background?.opacity ?? 0.6,
                    paddingX: editingText.style.background?.paddingX ?? 16,
                    paddingY: editingText.style.background?.paddingY ?? 8,
                    borderRadius: editingText.style.background?.borderRadius ?? 32
                  }
                })}
                className="rounded"
              />
              {t.backgroundPill}
            </label>
            {editingText.style.background?.enabled && (
              <div className="background-pill-controls space-y-2 mt-1 pl-1">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.pillColor}</span>
                  <ColorPicker
                    value={editingText.style.background.color.startsWith('rgba') ? '#000000' : editingText.style.background.color}
                    onChange={(color) => updateStyle({
                      background: { ...editingText.style.background!, color }
                    })}
                    onOpen={beginBatch}
                    onClose={commitBatch}
                  />
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.pillOpacity}</span>
                  <input
                    type="range" min="0" max="1" step="0.01"
                    value={editingText.style.background.opacity ?? 0.6}
                    style={sv(editingText.style.background.opacity ?? 0.6, 0, 1)}
                    onPointerDown={beginBatch}
                    onPointerUp={commitBatch}
                    onChange={(e) => updateStyle({
                      background: { ...editingText.style.background!, opacity: Number(e.target.value) }
                    })}
                    className="flex-1 min-w-0"
                  />
                  <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-7 text-right flex-shrink-0">{Math.round((editingText.style.background.opacity ?? 0.6) * 100)}%</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-[var(--on-surface-variant)] w-14 flex-shrink-0">{t.pillRadius}</span>
                  <input
                    type="range" min="0" max="32" step="1"
                    value={editingText.style.background.borderRadius}
                    style={sv(editingText.style.background.borderRadius, 0, 32)}
                    onPointerDown={beginBatch}
                    onPointerUp={commitBatch}
                    onChange={(e) => updateStyle({
                      background: { ...editingText.style.background!, borderRadius: Number(e.target.value) }
                    })}
                    className="flex-1 min-w-0"
                  />
                  <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-7 text-right flex-shrink-0">{editingText.style.background.borderRadius}</span>
                </div>
              </div>
            )}
          </div>
        </div>
      ) : (
        <p className="text-xs text-[var(--on-surface-variant)]">{t.textPanelHint}</p>
      )}
    </div>
  );
}

// ============================================================================
// SidePanel (Main Export)
// ============================================================================
interface SidePanelProps {
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  segment: VideoSegment | null;
  editingKeyframeId: number | null;
  zoomFactor: number;
  setZoomFactor: (value: number) => void;
  onDeleteKeyframe: () => void;
  onUpdateZoom: (updates: { zoomFactor?: number; positionX?: number; positionY?: number }) => void;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  recentUploads: string[];
  onRemoveRecentUpload: (imageUrl: string) => void;
  onBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  editingTextId: string | null;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
}

export function SidePanel({
  activePanel,
  setActivePanel,
  segment,
  editingKeyframeId,
  zoomFactor,
  setZoomFactor,
  onDeleteKeyframe,
  onUpdateZoom,
  backgroundConfig,
  setBackgroundConfig,
  recentUploads,
  onRemoveRecentUpload,
  onBackgroundUpload,
  editingTextId,
  onUpdateSegment,
  beginBatch,
  commitBatch,
  canvasRef
}: SidePanelProps) {
  return (
    <div className="side-panel space-y-3">
      <PanelTabs activePanel={activePanel} onPanelChange={setActivePanel} />

      {activePanel === 'zoom' && (
        <ZoomPanel
          segment={segment}
          editingKeyframeId={editingKeyframeId}
          zoomFactor={zoomFactor}
          setZoomFactor={setZoomFactor}
          onDeleteKeyframe={onDeleteKeyframe}
          onUpdateZoom={onUpdateZoom}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
        />
      )}

      {activePanel === 'background' && (
        <BackgroundPanel
          backgroundConfig={backgroundConfig}
          setBackgroundConfig={setBackgroundConfig}
          recentUploads={recentUploads}
          onRemoveRecentUpload={onRemoveRecentUpload}
          onBackgroundUpload={onBackgroundUpload}
          canvasRef={canvasRef}
        />
      )}

      {activePanel === 'cursor' && (
        <CursorPanel backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig} />
      )}

      {activePanel === 'blur' && (
        <BlurPanel backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig} />
      )}

      {activePanel === 'text' && (
        <TextPanel
          segment={segment}
          editingTextId={editingTextId}
          onUpdateSegment={onUpdateSegment}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
        />
      )}
    </div>
  );
}
