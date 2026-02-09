import { Button } from '@/components/ui/button';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { Trash2, AlignLeft, AlignCenter, AlignRight } from 'lucide-react';
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

// ============================================================================
// Types
// ============================================================================
export type ActivePanel = 'zoom' | 'background' | 'cursor' | 'text';

const GRADIENT_PRESETS = {
  solid: 'bg-[var(--surface-dim)]',
  gradient1: 'bg-gradient-to-r from-blue-600 to-violet-600',
  gradient2: 'bg-gradient-to-r from-rose-400 to-orange-300',
  gradient3: 'bg-gradient-to-r from-emerald-500 to-teal-400'
} as const;

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
// BackgroundPanel
// ============================================================================
interface BackgroundPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  recentUploads: string[];
  onBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
}

function BackgroundPanel({
  backgroundConfig,
  setBackgroundConfig,
  recentUploads,
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
          <div className="background-presets-grid grid grid-cols-4 gap-2">
            {Object.entries(GRADIENT_PRESETS).map(([key, gradient]) => (
              <button
                key={key}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: key as BackgroundConfig['backgroundType'] }))}
                className={`aspect-square h-10 rounded-lg transition-all duration-150 ${gradient} ${
                  backgroundConfig.backgroundType === key
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              />
            ))}

            <label className="background-upload-btn aspect-square h-10 rounded-lg transition-all duration-150 cursor-pointer ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105 relative overflow-hidden group bg-[var(--glass-bg)]">
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" />
              <div className="upload-icon absolute inset-0 flex items-center justify-center">
                <svg className="w-4 h-4 text-[var(--on-surface-variant)] group-hover:text-[var(--primary-color)] transition-colors" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
              </div>
            </label>

            {recentUploads.map((imageUrl, index) => (
              <button
                key={index}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: imageUrl }))}
                className={`aspect-square h-10 rounded-lg transition-all duration-150 relative overflow-hidden ${
                  backgroundConfig.backgroundType === 'custom' && backgroundConfig.customBackground === imageUrl
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              >
                <img src={imageUrl} alt={`Upload ${index + 1}`} className="absolute inset-0 w-full h-full object-cover" />
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

function CursorVariantButton({ isSelected, onClick, label, children }: CursorVariantButtonProps) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      className={`cursor-variant-button w-9 h-9 rounded-md border transition-colors flex items-center justify-center ${
        isSelected
          ? 'border-[var(--primary-color)] bg-[var(--primary-color)]/15'
          : 'border-[var(--glass-border)] bg-[var(--glass-bg)] hover:border-[var(--primary-color)]/50'
      }`}
    >
      {children}
    </button>
  );
}

function ClassicArrowPreview() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" className="cursor-preview-svg">
      <path d="M8.2 4.9L19.8 16.5H13L12.6 16.6L8.2 20.9V4.9Z" fill="black" stroke="white" strokeWidth="1.5" />
      <path d="M17.3 21.6L13.7 23.1L9 12L12.7 10.5L17.3 21.6Z" fill="black" stroke="white" strokeWidth="1.5" />
    </svg>
  );
}

function ClassicTextPreview() {
  return (
    <svg width="18" height="18" viewBox="0 0 14 18" fill="none" className="cursor-preview-svg">
      <path d="M2 1H12V3H9V15H12V17H2V15H5V3H2V1Z" fill="black" stroke="white" strokeWidth="1.2" />
    </svg>
  );
}

function CursorPanel({ backgroundConfig, setBackgroundConfig }: CursorPanelProps) {
  const { t } = useSettings();
  const defaultVariant = backgroundConfig.cursorDefaultVariant ?? 'classic';
  const textVariant = backgroundConfig.cursorTextVariant ?? 'classic';
  const pointerVariant = backgroundConfig.cursorPointerVariant ?? 'screenstudio';
  const openHandVariant = backgroundConfig.cursorOpenHandVariant ?? 'screenstudio';
  return (
    <div className="cursor-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="cursor-controls space-y-3">
        <div className="cursor-size-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.cursorSize}</span>
            <span>{backgroundConfig.cursorScale ?? 2}x</span>
          </label>
          <input type="range" min="1" max="8" step="0.1" value={backgroundConfig.cursorScale ?? 2}
            style={sv(backgroundConfig.cursorScale ?? 2, 1, 8)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorScale: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div className="cursor-smoothness-field">
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.movementSmoothing}</span>
            <span>{backgroundConfig.cursorSmoothness ?? 5}</span>
          </label>
          <input type="range" min="0" max="10" step="1" value={backgroundConfig.cursorSmoothness ?? 5}
            style={sv(backgroundConfig.cursorSmoothness ?? 5, 0, 10)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorSmoothness: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div className="cursor-movement-delay-field">
          <label className="cursor-movement-delay-label text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.pointerMovementDelay}</span>
            <span>{(backgroundConfig.cursorMovementDelay ?? 0.03).toFixed(2)}s</span>
          </label>
          <input
            type="range"
            min="0"
            max="0.5"
            step="0.01"
            value={backgroundConfig.cursorMovementDelay ?? 0.03}
            style={sv(backgroundConfig.cursorMovementDelay ?? 0.03, 0, 0.5)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorMovementDelay: Number(e.target.value) }))}
            className="cursor-movement-delay-slider w-full"
          />
        </div>
        <div className="cursor-wiggle-strength-field">
          <label className="cursor-wiggle-strength-label text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>{t.pointerWiggleStrength}</span>
            <span>{Math.round((backgroundConfig.cursorWiggleStrength ?? 0.15) * 100)}%</span>
          </label>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={backgroundConfig.cursorWiggleStrength ?? 0.15}
            style={sv(backgroundConfig.cursorWiggleStrength ?? 0.15, 0, 1)}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorWiggleStrength: Number(e.target.value) }))}
            className="cursor-wiggle-strength-slider w-full"
          />
        </div>
        <div className="cursor-variants-section space-y-2">
          <label className="cursor-variants-label text-xs text-[var(--on-surface-variant)] block">{t.cursorVariants}</label>

          <div className="cursor-variant-row flex items-center justify-between gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)]">{t.cursorDefault}</span>
            <div className="cursor-variant-picker flex items-center gap-1.5">
              <CursorVariantButton
                isSelected={defaultVariant === 'classic'}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, cursorDefaultVariant: 'classic' }))}
                label={`${t.cursorDefault} classic`}
              >
                <ClassicArrowPreview />
              </CursorVariantButton>
              <CursorVariantButton
                isSelected={defaultVariant === 'screenstudio'}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, cursorDefaultVariant: 'screenstudio' }))}
                label={`${t.cursorDefault} screen studio`}
              >
                <img src="/cursor-default-screenstudio.svg" alt="" className="cursor-preview-image w-5 h-5 object-contain" />
              </CursorVariantButton>
            </div>
          </div>

          <div className="cursor-variant-row flex items-center justify-between gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)]">{t.cursorText}</span>
            <div className="cursor-variant-picker flex items-center gap-1.5">
              <CursorVariantButton
                isSelected={textVariant === 'classic'}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, cursorTextVariant: 'classic' }))}
                label={`${t.cursorText} classic`}
              >
                <ClassicTextPreview />
              </CursorVariantButton>
              <CursorVariantButton
                isSelected={textVariant === 'screenstudio'}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, cursorTextVariant: 'screenstudio' }))}
                label={`${t.cursorText} screen studio`}
              >
                <img src="/cursor-text-screenstudio.svg" alt="" className="cursor-preview-image w-5 h-5 object-contain" />
              </CursorVariantButton>
            </div>
          </div>

          <div className="cursor-variant-row flex items-center justify-between gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)]">{t.cursorPointer}</span>
            <div className="cursor-variant-picker flex items-center gap-1.5">
              <CursorVariantButton
                isSelected={pointerVariant === 'screenstudio'}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, cursorPointerVariant: 'screenstudio' }))}
                label={`${t.cursorPointer} screen studio`}
              >
                <img src="/cursor-pointer-screenstudio.svg" alt="" className="cursor-preview-image w-5 h-5 object-contain" />
              </CursorVariantButton>
            </div>
          </div>

          <div className="cursor-variant-row flex items-center justify-between gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)]">{t.cursorOpenHand}</span>
            <div className="cursor-variant-picker flex items-center gap-1.5">
              <CursorVariantButton
                isSelected={openHandVariant === 'screenstudio'}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, cursorOpenHandVariant: 'screenstudio' }))}
                label={`${t.cursorOpenHand} screen studio`}
              >
                <img src="/cursor-openhand-screenstudio.svg" alt="" className="cursor-preview-image w-5 h-5 object-contain" />
              </CursorVariantButton>
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
          onBackgroundUpload={onBackgroundUpload}
          canvasRef={canvasRef}
        />
      )}

      {activePanel === 'cursor' && (
        <CursorPanel backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig} />
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
