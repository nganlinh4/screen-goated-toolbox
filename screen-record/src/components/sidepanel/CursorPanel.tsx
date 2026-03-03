import { useMemo, useState } from 'react';
import { VideoSegment, BackgroundConfig } from '@/types/video';
import { Slider } from '@/components/ui/Slider';
import { Switch } from '@/components/ui/Switch';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';

export const CURSOR_ASSET_VERSION = `cursor-variants-runtime-${Date.now()}`;
export const CURSOR_VARIANT_ROW_HEIGHT = 58;
export const CURSOR_VARIANT_VIEWPORT_HEIGHT = 280;

export type CursorVariant = 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11' | 'sgtwatermelon';

export interface CursorVariantRow {
  id: string;
  label: string;
  screenstudioSrc: string;
  macos26Src: string;
  sgtcuteSrc: string;
  sgtcoolSrc: string;
  sgtaiSrc: string;
  sgtpixelSrc: string;
  jepriwin11Src: string;
  sgtwatermelonSrc: string;
}

interface CursorVariantButtonProps {
  isSelected: boolean;
  onClick: () => void;
  label: string;
  children: React.ReactNode;
}

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

export interface CursorPanelProps {
  segment: VideoSegment | null;
  onUpdateSegment: (segment: VideoSegment) => void;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
}

export function CursorPanel({
  segment,
  onUpdateSegment,
  backgroundConfig,
  setBackgroundConfig
}: CursorPanelProps) {
  const { t } = useSettings();
  const [variantScrollTop, setVariantScrollTop] = useState(0);
  const useCustomCursor = segment?.useCustomCursor !== false;
  const canToggleCustomCursor = Boolean(segment);
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
    { id: 'default', label: t.cursorDefault, screenstudioSrc: '/cursor-default-screenstudio.svg', macos26Src: '/cursor-default-macos26.svg', sgtcuteSrc: '/cursor-default-sgtcute.svg', sgtcoolSrc: '/cursor-default-sgtcool.svg', sgtaiSrc: '/cursor-default-sgtai.svg', sgtpixelSrc: '/cursor-default-sgtpixel.svg', jepriwin11Src: '/cursor-default-jepriwin11.svg', sgtwatermelonSrc: '/cursor-default-sgtwatermelon.svg' },
    { id: 'text', label: t.cursorText, screenstudioSrc: '/cursor-text-screenstudio.svg', macos26Src: '/cursor-text-macos26.svg', sgtcuteSrc: '/cursor-text-sgtcute.svg', sgtcoolSrc: '/cursor-text-sgtcool.svg', sgtaiSrc: '/cursor-text-sgtai.svg', sgtpixelSrc: '/cursor-text-sgtpixel.svg', jepriwin11Src: '/cursor-text-jepriwin11.svg', sgtwatermelonSrc: '/cursor-text-sgtwatermelon.svg' },
    { id: 'pointer', label: t.cursorPointer, screenstudioSrc: '/cursor-pointer-screenstudio.svg', macos26Src: '/cursor-pointer-macos26.svg', sgtcuteSrc: '/cursor-pointer-sgtcute.svg', sgtcoolSrc: '/cursor-pointer-sgtcool.svg', sgtaiSrc: '/cursor-pointer-sgtai.svg', sgtpixelSrc: '/cursor-pointer-sgtpixel.svg', jepriwin11Src: '/cursor-pointer-jepriwin11.svg', sgtwatermelonSrc: '/cursor-pointer-sgtwatermelon.svg' },
    { id: 'openhand', label: t.cursorOpenHand, screenstudioSrc: '/cursor-openhand-screenstudio.svg', macos26Src: '/cursor-openhand-macos26.svg', sgtcuteSrc: '/cursor-openhand-sgtcute.svg', sgtcoolSrc: '/cursor-openhand-sgtcool.svg', sgtaiSrc: '/cursor-openhand-sgtai.svg', sgtpixelSrc: '/cursor-openhand-sgtpixel.svg', jepriwin11Src: '/cursor-openhand-jepriwin11.svg', sgtwatermelonSrc: '/cursor-openhand-sgtwatermelon.svg' },
    { id: 'closehand', label: 'Closed Hand', screenstudioSrc: '/cursor-closehand-screenstudio.svg', macos26Src: '/cursor-closehand-macos26.svg', sgtcuteSrc: '/cursor-closehand-sgtcute.svg', sgtcoolSrc: '/cursor-closehand-sgtcool.svg', sgtaiSrc: '/cursor-closehand-sgtai.svg', sgtpixelSrc: '/cursor-closehand-sgtpixel.svg', jepriwin11Src: '/cursor-closehand-jepriwin11.svg', sgtwatermelonSrc: '/cursor-closehand-sgtwatermelon.svg' },
    { id: 'wait', label: 'Wait', screenstudioSrc: '/cursor-wait-screenstudio.svg', macos26Src: '/cursor-wait-macos26.svg', sgtcuteSrc: '/cursor-wait-sgtcute.svg', sgtcoolSrc: '/cursor-wait-sgtcool.svg', sgtaiSrc: '/cursor-wait-sgtai.svg', sgtpixelSrc: '/cursor-wait-sgtpixel.svg', jepriwin11Src: '/cursor-wait-jepriwin11.svg', sgtwatermelonSrc: '/cursor-wait-sgtwatermelon.svg' },
    { id: 'appstarting', label: 'App Starting', screenstudioSrc: '/cursor-appstarting-screenstudio.svg', macos26Src: '/cursor-appstarting-macos26.svg', sgtcuteSrc: '/cursor-appstarting-sgtcute.svg', sgtcoolSrc: '/cursor-appstarting-sgtcool.svg', sgtaiSrc: '/cursor-appstarting-sgtai.svg', sgtpixelSrc: '/cursor-appstarting-sgtpixel.svg', jepriwin11Src: '/cursor-appstarting-jepriwin11.svg', sgtwatermelonSrc: '/cursor-appstarting-sgtwatermelon.svg' },
    { id: 'crosshair', label: 'Crosshair', screenstudioSrc: '/cursor-crosshair-screenstudio.svg', macos26Src: '/cursor-crosshair-macos26.svg', sgtcuteSrc: '/cursor-crosshair-sgtcute.svg', sgtcoolSrc: '/cursor-crosshair-sgtcool.svg', sgtaiSrc: '/cursor-crosshair-sgtai.svg', sgtpixelSrc: '/cursor-crosshair-sgtpixel.svg', jepriwin11Src: '/cursor-crosshair-jepriwin11.svg', sgtwatermelonSrc: '/cursor-crosshair-sgtwatermelon.svg' },
    { id: 'resize_ns', label: 'Resize N-S', screenstudioSrc: '/cursor-resize-ns-screenstudio.svg', macos26Src: '/cursor-resize-ns-macos26.svg', sgtcuteSrc: '/cursor-resize-ns-sgtcute.svg', sgtcoolSrc: '/cursor-resize-ns-sgtcool.svg', sgtaiSrc: '/cursor-resize-ns-sgtai.svg', sgtpixelSrc: '/cursor-resize-ns-sgtpixel.svg', jepriwin11Src: '/cursor-resize-ns-jepriwin11.svg', sgtwatermelonSrc: '/cursor-resize-ns-sgtwatermelon.svg' },
    { id: 'resize_we', label: 'Resize W-E', screenstudioSrc: '/cursor-resize-we-screenstudio.svg', macos26Src: '/cursor-resize-we-macos26.svg', sgtcuteSrc: '/cursor-resize-we-sgtcute.svg', sgtcoolSrc: '/cursor-resize-we-sgtcool.svg', sgtaiSrc: '/cursor-resize-we-sgtai.svg', sgtpixelSrc: '/cursor-resize-we-sgtpixel.svg', jepriwin11Src: '/cursor-resize-we-jepriwin11.svg', sgtwatermelonSrc: '/cursor-resize-we-sgtwatermelon.svg' },
    { id: 'resize_nwse', label: 'Resize NW-SE', screenstudioSrc: '/cursor-resize-nwse-screenstudio.svg', macos26Src: '/cursor-resize-nwse-macos26.svg', sgtcuteSrc: '/cursor-resize-nwse-sgtcute.svg', sgtcoolSrc: '/cursor-resize-nwse-sgtcool.svg', sgtaiSrc: '/cursor-resize-nwse-sgtai.svg', sgtpixelSrc: '/cursor-resize-nwse-sgtpixel.svg', jepriwin11Src: '/cursor-resize-nwse-jepriwin11.svg', sgtwatermelonSrc: '/cursor-resize-nwse-sgtwatermelon.svg' },
    { id: 'resize_nesw', label: 'Resize NE-SW', screenstudioSrc: '/cursor-resize-nesw-screenstudio.svg', macos26Src: '/cursor-resize-nesw-macos26.svg', sgtcuteSrc: '/cursor-resize-nesw-sgtcute.svg', sgtcoolSrc: '/cursor-resize-nesw-sgtcool.svg', sgtaiSrc: '/cursor-resize-nesw-sgtai.svg', sgtpixelSrc: '/cursor-resize-nesw-sgtpixel.svg', jepriwin11Src: '/cursor-resize-nesw-jepriwin11.svg', sgtwatermelonSrc: '/cursor-resize-nesw-sgtwatermelon.svg' },
  ]), [t.cursorDefault, t.cursorText, t.cursorPointer, t.cursorOpenHand]);
  const viewportHeight = CURSOR_VARIANT_VIEWPORT_HEIGHT;
  const totalHeight = rows.length * CURSOR_VARIANT_ROW_HEIGHT;
  const startIndex = Math.max(0, Math.floor(variantScrollTop / CURSOR_VARIANT_ROW_HEIGHT) - 2);
  const visibleCount = Math.ceil(viewportHeight / CURSOR_VARIANT_ROW_HEIGHT) + 4;
  const endIndex = Math.min(rows.length, startIndex + visibleCount);
  const visibleRows = rows.slice(startIndex, endIndex);
  return (
    <PanelCard className="cursor-panel">
      <div className="cursor-controls space-y-3.5">
        <div className="cursor-custom-toggle-field flex items-center justify-between gap-2">
          <span className="text-[10px] text-on-surface-variant">{t.useCustomCursor}</span>
          <Switch
            checked={useCustomCursor}
            disabled={!canToggleCustomCursor}
            onCheckedChange={(val) => {
              if (!segment) return;
              onUpdateSegment({ ...segment, useCustomCursor: val });
            }}
          />
        </div>
        <SettingRow label={t.cursorSize} valueDisplay={`${(backgroundConfig.cursorScale ?? 2).toFixed(1)}x`} className="cursor-size-field">
          <Slider
            min={1} max={8} step={0.1} value={backgroundConfig.cursorScale ?? 2}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, cursorScale: val }))}
          />
        </SettingRow>
        <SettingRow label="Shadow" valueDisplay={`${Math.round(backgroundConfig.cursorShadow ?? 35)}%`} className="cursor-shadow-field">
          <Slider
            min={0} max={200} step={1} value={backgroundConfig.cursorShadow ?? 35}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, cursorShadow: val }))}
          />
        </SettingRow>
        <SettingRow label={t.movementSmoothing} valueDisplay={`${backgroundConfig.cursorSmoothness ?? 5}`} className="cursor-smoothness-field">
          <Slider
            min={0} max={10} step={1} value={backgroundConfig.cursorSmoothness ?? 5}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, cursorSmoothness: val }))}
          />
        </SettingRow>
        <SettingRow label={t.pointerMovementDelay} valueDisplay={`${(backgroundConfig.cursorMovementDelay ?? 0).toFixed(2)}s`} className="cursor-movement-delay-field">
          <Slider
            min={-0.5} max={0.5} step={0.01} value={backgroundConfig.cursorMovementDelay ?? 0}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, cursorMovementDelay: val }))}
            className="cursor-movement-delay-slider"
          />
        </SettingRow>
        <SettingRow label={t.pointerWiggleStrength} valueDisplay={`${Math.round((backgroundConfig.cursorWiggleStrength ?? 0.30) * 100)}%`} className="cursor-wiggle-strength-field">
          <Slider
            min={0} max={1} step={0.01} value={backgroundConfig.cursorWiggleStrength ?? 0.30}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, cursorWiggleStrength: val }))}
            className="cursor-wiggle-strength-slider"
          />
        </SettingRow>
        <SettingRow label={t.cursorTilt} valueDisplay={`${backgroundConfig.cursorTiltAngle ?? -10}°`} className="cursor-tilt-angle-field">
          <Slider
            min={-30} max={60} step={1} value={backgroundConfig.cursorTiltAngle ?? -10}
            onChange={(val) => setBackgroundConfig(prev => ({ ...prev, cursorTiltAngle: val }))}
            className="cursor-tilt-angle-slider"
          />
        </SettingRow>
        <div className="cursor-variants-section space-y-3.5">
          <div
            className="cursor-variant-virtualized-list border border-glass-border rounded-lg overflow-hidden"
            style={{ height: `${viewportHeight}px` }}
          >
            <div
              className="cursor-variant-virtualized-scroll thin-scrollbar h-full overflow-y-auto"
              onScroll={(e) => setVariantScrollTop(e.currentTarget.scrollTop)}
            >
              <div className="cursor-variant-column-header sticky top-0 z-10 min-h-8 py-1 px-1.5 border-b border-glass-border grid grid-cols-8 gap-1.5 items-start bg-surface">
                {['Mac OG', 'Mac Tahoe+', 'SGT Cute', 'SGT Cool', 'SGT AI', 'SGT Pixel', 'Jepri Win11', 'SGT Watermelon'].map((name) => (
                  <span
                    key={name}
                    className="cursor-variant-col-label text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-on-surface-variant"
                    style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                  >
                    {name}
                  </span>
                ))}
              </div>
              <div className="cursor-variant-virtualized-inner relative" style={{ height: `${totalHeight}px` }}>
                {visibleRows.map((row, i) => {
                  const absoluteIndex = startIndex + i;
                  const tiltDeg = backgroundConfig.cursorTiltAngle ?? -10;
                  const hasTilt = (row.id === 'default' || row.id === 'pointer') && Math.abs(tiltDeg) > 0.5;
                  const tiltStyle = hasTilt ? { rotate: `${tiltDeg}deg` } as React.CSSProperties : undefined;
                  const variantKeys: Array<{ pack: CursorVariant; src: string }> = [
                    { pack: 'screenstudio', src: row.screenstudioSrc },
                    { pack: 'macos26', src: row.macos26Src },
                    { pack: 'sgtcute', src: row.sgtcuteSrc },
                    { pack: 'sgtcool', src: row.sgtcoolSrc },
                    { pack: 'sgtai', src: row.sgtaiSrc },
                    { pack: 'sgtpixel', src: row.sgtpixelSrc },
                    { pack: 'jepriwin11', src: row.jepriwin11Src },
                    { pack: 'sgtwatermelon', src: row.sgtwatermelonSrc },
                  ];
                  return (
                    <div
                      key={row.id}
                      className="cursor-variant-row absolute left-0 right-0 px-1.5 grid grid-cols-8 gap-1.5 items-center"
                      style={{ top: `${absoluteIndex * CURSOR_VARIANT_ROW_HEIGHT}px`, height: `${CURSOR_VARIANT_ROW_HEIGHT}px` }}
                    >
                      {variantKeys.map(({ pack, src }) => (
                        <CursorVariantButton
                          key={pack}
                          isSelected={inferredPack === pack}
                          onClick={() => setCursorPack(pack)}
                          label={`${row.label} ${pack}`}
                        >
                          <img src={`${src}?v=${CURSOR_ASSET_VERSION}`} alt="" className="cursor-preview-image w-8 h-8 min-w-8 min-h-8 object-contain scale-[1.35]" style={tiltStyle} />
                        </CursorVariantButton>
                      ))}
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      </div>
    </PanelCard>
  );
}
