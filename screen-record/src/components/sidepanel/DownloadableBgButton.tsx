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
 * Shared trash-can badge for downloadable backgrounds and recent-upload
 * swatches. It is a real button rendered as a SIBLING of the swatch's select
 * button (never nested inside it), so it is keyboard reachable and announced.
 * The swatch wrapper carries the `group` class that reveals it on hover.
 */
export function SwatchDeleteBadge({
  className,
  iconClassName,
  onClick,
  label,
}: {
  className: string;
  iconClassName: string;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`${className} opacity-0 group-hover:opacity-100 focus-visible:opacity-100`}
      title={label}
      aria-label={label}
    >
      <Trash2 className={iconClassName} />
    </button>
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
      void startDownload();
    }
  };

  const isSelected = isDownloaded && backgroundConfig.backgroundType === 'custom'
    && backgroundConfig.customBackground?.includes(`/bg-downloaded/${bg.id}.`);

  return (
    <div className="downloadable-bg-swatch aspect-square h-10 relative group">
      <button
        type="button"
        onClick={handleClick}
        aria-pressed={Boolean(isSelected)}
        title={
          isDownloading ? t.backgroundDownloadingProgress.replace('{progress}', String(Math.round(progress)))
          : isPrewarming ? t.backgroundPreparingExport
          : isDownloaded ? bg.id
          : state.status === 'error'
            ? t.backgroundDownloadError.replace('{message}', (state as { status: 'error'; message: string }).message)
          : t.backgroundClickToDownload
        }
        className={`downloadable-bg-btn ui-choice-tile w-full h-full rounded-lg relative overflow-hidden ${
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
      {isDownloaded && (
        <SwatchDeleteBadge
          onClick={() => void deleteBg()}
          className="downloadable-bg-delete absolute top-0.5 right-0.5 w-3.5 h-3.5 rounded-xs bg-black/50 flex items-center justify-center transition-opacity cursor-pointer hover:bg-red-500/80 z-10"
          iconClassName="w-2 h-2 text-white"
          label={t.backgroundDeleteDownloadedFile}
        />
      )}
    </div>
  );
}
