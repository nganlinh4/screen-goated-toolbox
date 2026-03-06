import { useEffect, useMemo, useRef, useState } from 'react';
import { VideoSegment, BackgroundConfig } from '@/types/video';
import { Slider } from '@/components/ui/Slider';
import { Switch } from '@/components/ui/Switch';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';
import {
  CURSOR_PACKS,
  type CursorPack,
  type CursorRenderKind,
} from '@/lib/renderer/cursorModel';

export const CURSOR_ASSET_VERSION = `cursor-variants-runtime-${Date.now()}`;
export const CURSOR_VARIANT_ROW_HEIGHT = 58;
export const CURSOR_VARIANT_VIEWPORT_HEIGHT = 280;
export const CURSOR_VARIANT_COLUMN_WIDTH = 40;
export const CURSOR_VARIANT_COLUMN_GAP = 6;
export const CURSOR_VARIANT_CANVAS_PADDING_X = 6;
export const CURSOR_VARIANT_PAN_THRESHOLD_PX = 4;

type CursorVariant = CursorPack;

export interface CursorVariantRow {
  id: CursorRenderKind;
  label: string;
  variants: Record<CursorVariant, string>;
}

interface CursorVariantButtonProps {
  isSelected: boolean;
  onClick: () => void;
  label: string;
  children: React.ReactNode;
}

const CURSOR_PACK_LABELS: Record<CursorVariant, string> = {
  screenstudio: 'Mac OG',
  macos26: 'Mac Tahoe+',
  sgtcute: 'SGT Cute',
  sgtcool: 'SGT Cool',
  sgtai: 'SGT AI',
  sgtpixel: 'SGT Pixel',
  jepriwin11: 'Jepri Win11',
  sgtwatermelon: 'SGT Watermelon',
  sgtfastfood: 'SGT Fastfood',
  sgtveggie: 'SGT Veggie',
  sgtvietnam: 'SGT Vietnam',
  sgtkorea: 'SGT Korea',
};

function getCursorVariantSrc(kind: CursorRenderKind, pack: CursorVariant): string {
  return `/cursor-${kind}-${pack}.svg`;
}

function buildCursorVariantRow(id: CursorRenderKind, label: string): CursorVariantRow {
  const variants = {} as Record<CursorVariant, string>;
  for (const pack of CURSOR_PACKS) {
    variants[pack] = getCursorVariantSrc(id, pack);
  }
  return { id, label, variants };
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
  const [isVariantCanvasPanning, setIsVariantCanvasPanning] = useState(false);
  const variantScrollRef = useRef<HTMLDivElement | null>(null);
  const variantPanCleanupRef = useRef<(() => void) | null>(null);
  const suppressVariantClickRef = useRef(false);
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
    buildCursorVariantRow('default', t.cursorDefault),
    buildCursorVariantRow('text', t.cursorText),
    buildCursorVariantRow('pointer', t.cursorPointer),
    buildCursorVariantRow('openhand', t.cursorOpenHand),
    buildCursorVariantRow('closehand', 'Closed Hand'),
    buildCursorVariantRow('wait', 'Wait'),
    buildCursorVariantRow('appstarting', 'App Starting'),
    buildCursorVariantRow('crosshair', 'Crosshair'),
    buildCursorVariantRow('resize-ns', 'Resize N-S'),
    buildCursorVariantRow('resize-we', 'Resize W-E'),
    buildCursorVariantRow('resize-nwse', 'Resize NW-SE'),
    buildCursorVariantRow('resize-nesw', 'Resize NE-SW'),
  ]), [t.cursorDefault, t.cursorText, t.cursorPointer, t.cursorOpenHand]);
  const viewportHeight = CURSOR_VARIANT_VIEWPORT_HEIGHT;
  const totalHeight = rows.length * CURSOR_VARIANT_ROW_HEIGHT;
  const variantGridWidth = (CURSOR_PACKS.length * CURSOR_VARIANT_COLUMN_WIDTH)
    + ((CURSOR_PACKS.length - 1) * CURSOR_VARIANT_COLUMN_GAP);
  const variantCanvasWidth = variantGridWidth + (CURSOR_VARIANT_CANVAS_PADDING_X * 2);
  const startIndex = Math.max(0, Math.floor(variantScrollTop / CURSOR_VARIANT_ROW_HEIGHT) - 2);
  const visibleCount = Math.ceil(viewportHeight / CURSOR_VARIANT_ROW_HEIGHT) + 4;
  const endIndex = Math.min(rows.length, startIndex + visibleCount);
  const visibleRows = rows.slice(startIndex, endIndex);

  useEffect(() => {
    return () => {
      variantPanCleanupRef.current?.();
      variantPanCleanupRef.current = null;
      suppressVariantClickRef.current = false;
    };
  }, []);

  const handleVariantCanvasPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const scrollEl = variantScrollRef.current;
    if (!scrollEl) return;

    const startX = event.clientX;
    const startY = event.clientY;
    const startScrollLeft = scrollEl.scrollLeft;
    const startScrollTop = scrollEl.scrollTop;
    let isDragging = false;

    const handlePointerMove = (moveEvent: PointerEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;

      if (!isDragging) {
        if (Math.hypot(dx, dy) < CURSOR_VARIANT_PAN_THRESHOLD_PX) return;
        isDragging = true;
        suppressVariantClickRef.current = true;
        setIsVariantCanvasPanning(true);
      }

      scrollEl.scrollLeft = startScrollLeft - dx;
      scrollEl.scrollTop = startScrollTop - dy;
      moveEvent.preventDefault();
    };

    const handlePointerEnd = () => {
      variantPanCleanupRef.current?.();
      variantPanCleanupRef.current = null;
      setIsVariantCanvasPanning(false);
      if (!isDragging) {
        suppressVariantClickRef.current = false;
        return;
      }
      window.setTimeout(() => {
        suppressVariantClickRef.current = false;
      }, 0);
    };

    variantPanCleanupRef.current?.();
    variantPanCleanupRef.current = () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerEnd);
      window.removeEventListener('pointercancel', handlePointerEnd);
    };
    window.addEventListener('pointermove', handlePointerMove, { passive: false });
    window.addEventListener('pointerup', handlePointerEnd);
    window.addEventListener('pointercancel', handlePointerEnd);
  };

  const handleVariantCanvasClickCapture = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!suppressVariantClickRef.current) return;
    event.preventDefault();
    event.stopPropagation();
    suppressVariantClickRef.current = false;
  };

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
              ref={variantScrollRef}
              className={`cursor-variant-virtualized-scroll thin-scrollbar h-full overflow-auto ${isVariantCanvasPanning ? 'cursor-grabbing select-none' : 'cursor-grab'}`}
              onScroll={(e) => setVariantScrollTop(e.currentTarget.scrollTop)}
              onPointerDown={handleVariantCanvasPointerDown}
              onClickCapture={handleVariantCanvasClickCapture}
              style={{ touchAction: 'none' }}
            >
              <div
                className="cursor-variant-column-header sticky top-0 z-10 min-h-8 py-1 px-1.5 border-b border-glass-border grid items-start bg-surface"
                style={{
                  width: `${variantCanvasWidth}px`,
                  gridTemplateColumns: `repeat(${CURSOR_PACKS.length}, ${CURSOR_VARIANT_COLUMN_WIDTH}px)`,
                  gap: `${CURSOR_VARIANT_COLUMN_GAP}px`,
                }}
              >
                {CURSOR_PACKS.map((pack) => (
                  <span
                    key={pack}
                    className="cursor-variant-col-label text-center text-[9px] leading-[1.05] tracking-tight whitespace-normal break-words text-on-surface-variant"
                    style={{ fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif", fontVariationSettings: "'wdth' 30, 'ROND' 100" }}
                  >
                    {CURSOR_PACK_LABELS[pack]}
                  </span>
                ))}
              </div>
              <div
                className="cursor-variant-virtualized-inner relative"
                style={{ height: `${totalHeight}px`, width: `${variantCanvasWidth}px` }}
              >
                {visibleRows.map((row, i) => {
                  const absoluteIndex = startIndex + i;
                  const tiltDeg = backgroundConfig.cursorTiltAngle ?? -10;
                  const hasTilt = (row.id === 'default' || row.id === 'pointer') && Math.abs(tiltDeg) > 0.5;
                  const tiltStyle = hasTilt ? { rotate: `${tiltDeg}deg` } as React.CSSProperties : undefined;
                  const variantKeys = CURSOR_PACKS.map((pack) => ({
                    pack,
                    src: row.variants[pack],
                  }));
                  return (
                    <div
                      key={row.id}
                      className="cursor-variant-row absolute left-0 px-1.5 grid items-center"
                      style={{
                        top: `${absoluteIndex * CURSOR_VARIANT_ROW_HEIGHT}px`,
                        height: `${CURSOR_VARIANT_ROW_HEIGHT}px`,
                        width: `${variantCanvasWidth}px`,
                        gridTemplateColumns: `repeat(${CURSOR_PACKS.length}, ${CURSOR_VARIANT_COLUMN_WIDTH}px)`,
                        gap: `${CURSOR_VARIANT_COLUMN_GAP}px`,
                      }}
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
