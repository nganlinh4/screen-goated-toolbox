import { useEffect, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { Loader2, Upload } from '@/components/ui/MaterialIcon';
import { BackgroundConfig } from '@/types/video';
import { Slider } from '@/components/ui/Slider';
import { Switch } from '@/components/ui/Switch';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';
import downloadableBackgrounds from '@/config/downloadable-backgrounds.json';
import {
  BUILT_IN_BACKGROUND_PANEL_ORDER,
  DEFAULT_BUILT_IN_BACKGROUND_ID,
  type BuiltInBackgroundId,
} from '@/lib/backgroundPresets';
import { BUILT_IN_BACKGROUND_SWATCHES } from '@/lib/renderer/builtInBackgrounds';
import {
  type BgDlState,
  type DownloadableBg,
  buildDownloadedBgUrl,
  nativeBgStateToUiState,
} from './useDownloadableBg';
import { DownloadableBgButton, SwatchDeleteBadge } from './DownloadableBgButton';

const GRADIENT_PRESETS: Record<BuiltInBackgroundId, { style?: React.CSSProperties }> =
  Object.fromEntries(
    BUILT_IN_BACKGROUND_PANEL_ORDER.map((id) => [id, { style: BUILT_IN_BACKGROUND_SWATCHES[id] }])
  ) as Record<BuiltInBackgroundId, { style?: React.CSSProperties }>;

const DOWNLOADABLE_BACKGROUNDS: ReadonlyArray<DownloadableBg> = downloadableBackgrounds;

type DownloadableBgStateMap = Record<string, {
  downloaded?: boolean;
  ext?: string | null;
  version?: number | null;
  progress?: unknown;
}>;

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
  const [downloadedBgStates, setDownloadedBgStates] = useState<Record<string, BgDlState>>({});

  useEffect(() => {
    let cancelled = false;
    const syncDownloadableBackgrounds = async () => {
      try {
        const nativeStates = await invoke<DownloadableBgStateMap>('get_bg_download_states', {
          ids: DOWNLOADABLE_BACKGROUNDS.map(bg => bg.id),
        });
        if (cancelled) return;
        const nextStates = Object.fromEntries(
          DOWNLOADABLE_BACKGROUNDS.map(bg => [bg.id, nativeBgStateToUiState(nativeStates[bg.id])])
        ) as Record<string, BgDlState>;
        setDownloadedBgStates(nextStates);

        setBackgroundConfig(prev => {
          if (prev.backgroundType !== 'custom' || typeof prev.customBackground !== 'string') return prev;
          const selectedBg = DOWNLOADABLE_BACKGROUNDS.find(bg =>
            prev.customBackground?.includes(`/bg-downloaded/${bg.id}.`)
          );
          if (!selectedBg) return prev;
          const selectedState = nextStates[selectedBg.id];
          if (selectedState?.status === 'done') {
            const syncedUrl = buildDownloadedBgUrl(selectedBg.id, selectedState.ext, selectedState.version);
            return prev.customBackground === syncedUrl ? prev : { ...prev, customBackground: syncedUrl };
          }
          return { ...prev, backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID, customBackground: undefined };
        });
      } catch (error) {
        console.warn('Failed to sync downloadable background states:', error);
      }
    };

    void syncDownloadableBackgrounds();
    return () => {
      cancelled = true;
    };
  }, [setBackgroundConfig]);

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
        <div className="background-zoom-with-video-field flex items-center justify-between gap-3">
          <span className="text-[11px] font-medium text-on-surface-variant">{t.backgroundZoomWithVideo}</span>
          <Switch
            checked={backgroundConfig.backgroundZoomWithVideo !== false}
            onCheckedChange={(checked) => setBackgroundConfig(prev => ({ ...prev, backgroundZoomWithVideo: checked }))}
          />
        </div>
        <div className="background-style-field">
          <label className="text-xs font-medium uppercase tracking-wide text-on-surface-variant mb-2 block">{t.backgroundStyle}</label>
          <div className="background-presets-grid grid grid-cols-7 gap-2">
            {/* Upload button */}
            <label className={`background-upload-btn ui-choice-tile aspect-square h-10 rounded-lg cursor-pointer relative overflow-hidden group ${
              isBackgroundUploadProcessing
                ? 'opacity-80 cursor-wait'
                : ''
            }`}>
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" disabled={isBackgroundUploadProcessing} />
              <div className="upload-icon absolute inset-0 flex items-center justify-center">
                {isBackgroundUploadProcessing ? (
                  <Loader2 className="w-4 h-4 text-[var(--primary-color)] animate-spin" />
                ) : (
                  <Upload className="w-4 h-4 text-on-surface-variant group-hover:text-[var(--primary-color)] transition-colors" />
                )}
              </div>
            </label>

            {BUILT_IN_BACKGROUND_PANEL_ORDER.map((key) => {
              const preset = GRADIENT_PRESETS[key];
              const spinnerClass = key === 'white' ? 'text-gray-500/80' : 'text-white/85';
              return (
              <button
                key={key}
                onClick={() => applyPreset(key, { backgroundType: key as BackgroundConfig['backgroundType'] })}
                style={preset.style}
                className={`bg-preset-${key} ui-choice-tile aspect-square h-10 rounded-lg relative overflow-hidden ${
                  backgroundConfig.backgroundType === key
                    ? 'ui-choice-tile-active'
                    : ''
                }`}
              >
                {applyingKey === key && <div className="absolute inset-0 flex items-center justify-center"><Loader2 className={`w-3.5 h-3.5 ${spinnerClass} animate-spin drop-shadow-sm`} /></div>}
              </button>
              );
            })}

            {DOWNLOADABLE_BACKGROUNDS.map(bg => (
              <DownloadableBgButton
                key={bg.id}
                bg={bg}
                backgroundConfig={backgroundConfig}
                setBackgroundConfig={setBackgroundConfig}
                syncedState={downloadedBgStates[bg.id]}
              />
            ))}

            {recentUploads.map((imageUrl, index) => (
              <button
                key={index}
                onClick={() => applyPreset(imageUrl, { backgroundType: 'custom', customBackground: imageUrl })}
                className={`uploaded-bg-btn ui-choice-tile aspect-square h-10 rounded-lg relative overflow-hidden group ${
                  backgroundConfig.backgroundType === 'custom' && backgroundConfig.customBackground === imageUrl
                    ? 'ui-choice-tile-active'
                    : ''
                }`}
              >
                <img src={imageUrl} alt={`Upload ${index + 1}`} className="absolute inset-0 w-full h-full object-cover" />
                {applyingKey === imageUrl && <div className="absolute inset-0 flex items-center justify-center bg-black/20 z-20"><Loader2 className="w-3.5 h-3.5 text-white/85 animate-spin drop-shadow-sm" /></div>}
                <SwatchDeleteBadge
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onRemoveRecentUpload(imageUrl);
                  }}
                  className="uploaded-bg-delete absolute top-0.5 right-0.5 w-3.5 h-3.5 rounded-xs bg-black/50 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer hover:bg-red-500/80 z-10"
                  iconClassName="w-2.5 h-2.5 text-white"
                  title={t.backgroundRemoveUploaded}
                  ariaLabel={t.backgroundRemoveUploaded}
                />
              </button>
            ))}
          </div>
        </div>
      </div>
    </PanelCard>
  );
}
