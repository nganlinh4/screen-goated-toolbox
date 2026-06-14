import { useState } from 'react';
import { Trash2, Download, Loader2 } from '@/components/ui/MaterialIcon';
import { BackgroundConfig } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';
import {
  type BgDlState,
  type DownloadableBg,
  useDownloadableBg,
} from './useDownloadableBg';

/**
 * Shared trash-can overlay badge shown on hover for downloadable backgrounds
 * and recent-upload swatches. The exact class strings are passed in by the
 * caller so each swatch keeps its existing markup verbatim.
 */
export function SwatchDeleteBadge({
  className,
  iconClassName,
  onClick,
  title,
  ariaLabel,
}: {
  className: string;
  iconClassName: string;
  onClick: (e: React.MouseEvent) => void;
  title: string;
  ariaLabel?: string;
}) {
  return (
    <div
      onClick={onClick}
      className={className}
      title={title}
      aria-label={ariaLabel}
    >
      <Trash2 className={iconClassName} />
    </div>
  );
}

export function DownloadableBgButton({ bg, backgroundConfig, setBackgroundConfig, syncedState }: {
  bg: DownloadableBg;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  syncedState?: BgDlState;
}) {
  const { t } = useSettings();
  const { state, startDownload, selectBg, deleteBg } = useDownloadableBg(bg, setBackgroundConfig, syncedState);

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
        isDownloading ? t.backgroundDownloadingProgress.replace('{progress}', String(Math.round(progress)))
        : isPrewarming ? t.backgroundPreparingExport
        : isDownloaded ? bg.id
        : state.status === 'error'
          ? t.backgroundDownloadError.replace('{message}', (state as { status: 'error'; message: string }).message)
        : t.backgroundClickToDownload
      }
      className={`downloadable-bg-btn ui-choice-tile aspect-square h-10 rounded-lg relative overflow-hidden group ${
        isSelected
          ? 'ui-choice-tile-active'
          : ''
      }`}
    >
      <img
        src={bg.preview}
        alt={bg.id}
        className="absolute inset-0 w-full h-full object-cover"
        draggable={false}
      />
      {isDownloaded && (
        <SwatchDeleteBadge
          onClick={handleDelete}
          className="downloadable-bg-delete absolute top-0.5 right-0.5 w-3.5 h-3.5 rounded-sm bg-black/50 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer hover:bg-red-500/80 z-10"
          iconClassName="w-2 h-2 text-white"
          title={t.backgroundDeleteDownloadedFile}
        />
      )}
      {overlayOpacity > 0 && (
        <div
          className="downloadable-bg-overlay absolute inset-0 flex items-center justify-center transition-opacity duration-200"
          style={{
            backgroundColor: `rgba(0, 0, 0, ${0.18 * overlayOpacity})`,
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
