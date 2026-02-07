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
  solid: 'bg-black',
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
    <div className="flex border-b border-[var(--glass-border)]">
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
            <div className="absolute bottom-0 left-1/4 right-1/4 h-[2px] bg-[var(--primary-color)] rounded-full" />
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
      <div className="bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
        <div className="flex justify-between items-center mb-3">
          <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)]">{t.zoomConfiguration}</h2>
          <Button
            onClick={onDeleteKeyframe}
            variant="ghost"
            size="icon"
            className="text-[var(--on-surface-variant)] hover:text-[var(--tertiary-color)] hover:bg-[var(--tertiary-color)]/10 transition-colors"
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
        <div className="space-y-3">
          <div>
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
              <div className="flex justify-between text-[10px] text-[var(--on-surface-variant)]">
                <span>1x</span>
                <span className="text-[var(--on-surface)]">{zoomFactor.toFixed(1)}x</span>
                <span>3x</span>
              </div>
            </div>
          </div>
          <div className="space-y-3">
            <div>
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
            <div>
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
    <div className="bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-4 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
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
}

function BackgroundPanel({
  backgroundConfig,
  setBackgroundConfig,
  recentUploads,
  onBackgroundUpload
}: BackgroundPanelProps) {
  const { t } = useSettings();
  return (
    <div className="bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-3">{t.backgroundAndLayout}</h2>
      <div className="space-y-3">
        <div>
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
        <div>
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
        <div>
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
        <div>
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
        <div>
          <label className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-2 block">{t.backgroundStyle}</label>
          <div className="grid grid-cols-4 gap-2">
            {Object.entries(GRADIENT_PRESETS).map(([key, gradient]) => (
              <button
                key={key}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: key as BackgroundConfig['backgroundType'] }))}
                className={`aspect-square h-10 rounded-lg transition-all duration-150 ${gradient} ${
                  backgroundConfig.backgroundType === key
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-2 ring-offset-[var(--surface)] shadow-[0_0_12px_var(--primary-color)/30]'
                    : 'ring-1 ring-white/[0.08] hover:ring-[var(--primary-color)]/40 hover:scale-105'
                }`}
              />
            ))}

            <label className="aspect-square h-10 rounded-lg transition-all duration-150 cursor-pointer ring-1 ring-[var(--glass-border)] hover:ring-[var(--primary-color)]/40 hover:scale-105 relative overflow-hidden group bg-[var(--glass-bg)]">
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" />
              <div className="absolute inset-0 flex items-center justify-center">
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
                    : 'ring-1 ring-white/[0.08] hover:ring-[var(--primary-color)]/40 hover:scale-105'
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

function CursorPanel({ backgroundConfig, setBackgroundConfig }: CursorPanelProps) {
  const { t } = useSettings();
  return (
    <div className="bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-3">{t.cursorSettings}</h2>
      <div className="space-y-3">
        <div>
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
        <div>
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
    <div className="bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      {editingText && segment ? (
        <div className="space-y-2">
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
            <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.fontSize}</span>
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
            <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.color}</span>
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
                <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{label}</span>
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
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.textAlignment}</span>
            <div className="flex rounded-lg border border-[var(--glass-border)] overflow-hidden">
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
            <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.opacity}</span>
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
            <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.letterSpacing}</span>
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
                    color: editingText.style.background?.color ?? 'rgba(0,0,0,0.6)',
                    paddingX: editingText.style.background?.paddingX ?? 16,
                    paddingY: editingText.style.background?.paddingY ?? 8,
                    borderRadius: editingText.style.background?.borderRadius ?? 8
                  }
                })}
                className="rounded"
              />
              {t.backgroundPill}
            </label>
            {editingText.style.background?.enabled && (
              <div className="space-y-2 mt-1 pl-1">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.pillColor}</span>
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
                  <span className="text-[10px] text-[var(--on-surface-variant)] w-10 flex-shrink-0">{t.pillRadius}</span>
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
  commitBatch
}: SidePanelProps) {
  return (
    <div className="space-y-3">
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
