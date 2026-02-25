import { useMemo, useState } from 'react';
import { VideoSegment, BackgroundConfig } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';

/** Inline style for slider active track fill */
const sv = (v: number, min: number, max: number): React.CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as React.CSSProperties);

export const CURSOR_ASSET_VERSION = `cursor-variants-runtime-${Date.now()}`;
export const CURSOR_VARIANT_ROW_HEIGHT = 58;
export const CURSOR_VARIANT_VIEWPORT_HEIGHT = 280;

export type CursorVariant = 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';

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
      <div className="cursor-controls space-y-3.5">
        <div className="cursor-custom-toggle-field flex items-center justify-between gap-2">
          <span className="text-[10px] text-[var(--on-surface-variant)]">{t.useCustomCursor}</span>
          <button
            type="button"
            disabled={!canToggleCustomCursor}
            onClick={() => {
              if (!segment) return;
              onUpdateSegment({ ...segment, useCustomCursor: !useCustomCursor });
            }}
            className={`cursor-custom-toggle-btn relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
              !canToggleCustomCursor
                ? 'opacity-40 cursor-not-allowed bg-[var(--outline-variant)]'
                : useCustomCursor
                  ? 'bg-[var(--primary-color)]'
                  : 'bg-[var(--outline-variant)]'
            }`}
            aria-pressed={useCustomCursor}
            title={t.useCustomCursor}
          >
            <span
              className={`cursor-custom-toggle-thumb inline-block h-4 w-4 rounded-full bg-white shadow transition-transform ${
                useCustomCursor ? 'translate-x-4' : 'translate-x-0.5'
              }`}
            />
          </button>
        </div>
        <div className="cursor-size-field flex items-center gap-3">
          <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.cursorSize}</span>
          <input type="range" min="1" max="8" step="0.1" value={backgroundConfig.cursorScale ?? 2}
            style={sv(backgroundConfig.cursorScale ?? 2, 1, 8)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorScale: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{(backgroundConfig.cursorScale ?? 2).toFixed(1)}x</span>
        </div>
        <div className="cursor-shadow-field flex items-center gap-3">
          <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">Shadow</span>
          <input type="range" min="0" max="200" step="1" value={backgroundConfig.cursorShadow ?? 35}
            style={sv(backgroundConfig.cursorShadow ?? 35, 0, 200)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorShadow: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{Math.round(backgroundConfig.cursorShadow ?? 35)}%</span>
        </div>
        <div className="cursor-smoothness-field flex items-center gap-3">
          <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.movementSmoothing}</span>
          <input type="range" min="0" max="10" step="1" value={backgroundConfig.cursorSmoothness ?? 5}
            style={sv(backgroundConfig.cursorSmoothness ?? 5, 0, 10)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorSmoothness: Number(e.target.value) }))}
            className="flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{backgroundConfig.cursorSmoothness ?? 5}</span>
        </div>
        <div className="cursor-movement-delay-field flex items-center gap-3">
          <span className="cursor-movement-delay-label text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.pointerMovementDelay}</span>
          <input
            type="range"
            min="-0.5"
            max="0.5"
            step="0.01"
            value={backgroundConfig.cursorMovementDelay ?? 0}
            style={sv(backgroundConfig.cursorMovementDelay ?? 0, -0.5, 0.5)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorMovementDelay: Number(e.target.value) }))}
            className="cursor-movement-delay-slider flex-1 min-w-0"
          />
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{(backgroundConfig.cursorMovementDelay ?? 0).toFixed(2)}s</span>
        </div>
        <div className="cursor-wiggle-strength-field flex items-center gap-3">
          <span className="cursor-wiggle-strength-label text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.pointerWiggleStrength}</span>
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
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{Math.round((backgroundConfig.cursorWiggleStrength ?? 0.30) * 100)}%</span>
        </div>
        <div className="cursor-tilt-angle-field flex items-center gap-3">
          <span className="cursor-tilt-angle-label text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.cursorTilt}</span>
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
          <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{backgroundConfig.cursorTiltAngle ?? -10}°</span>
        </div>
        <div className="cursor-variants-section space-y-3.5">
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
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                >
                  Mac OG
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                >
                  Mac Tahoe+
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                >
                  SGT Cute
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                >
                  SGT Cool
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                >
                  SGT AI
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
                >
                  SGT Pixel
                </span>
                <span
                  className="text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-[var(--on-surface-variant)]"
                  style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 84, 'ROND' 100" }}
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
