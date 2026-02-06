import { Button } from '@/components/ui/button';
import { Trash2, Type } from 'lucide-react';
import { VideoSegment, BackgroundConfig } from '@/types/video';

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
  const tabs: { id: ActivePanel; label: string }[] = [
    { id: 'zoom', label: 'Zoom' },
    { id: 'background', label: 'Background' },
    { id: 'cursor', label: 'Cursor' },
    { id: 'text', label: 'Text' }
  ];

  return (
    <div className="flex border-b border-white/[0.06]">
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
}

function ZoomPanel({
  segment,
  editingKeyframeId,
  zoomFactor,
  setZoomFactor,
  onDeleteKeyframe,
  onUpdateZoom
}: ZoomPanelProps) {
  if (editingKeyframeId !== null && segment) {
    const keyframe = segment.zoomKeyframes[editingKeyframeId];
    if (!keyframe) return null;

    return (
      <div className="bg-white/[0.04] backdrop-blur-xl rounded-xl border border-white/[0.06] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
        <div className="flex justify-between items-center mb-3">
          <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)]">Zoom Configuration</h2>
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
            <label className="text-xs text-[var(--on-surface-variant)] mb-2">Zoom Factor</label>
            <div className="space-y-2">
              <input
                type="range"
                min="1"
                max="3"
                step="0.1"
                value={zoomFactor}
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
                <span>Horizontal Position</span>
                <span>{Math.round((keyframe?.positionX ?? 0.5) * 100)}%</span>
              </label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={keyframe?.positionX ?? 0.5}
                onChange={(e) => onUpdateZoom({ positionX: Number(e.target.value) })}
                className="w-full"
              />
            </div>
            <div>
              <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
                <span>Vertical Position</span>
                <span>{Math.round((keyframe?.positionY ?? 0.5) * 100)}%</span>
              </label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={keyframe?.positionY ?? 0.5}
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
    <div className="bg-white/[0.04] backdrop-blur-xl rounded-xl border border-white/[0.06] p-4 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <p className="text-xs text-[var(--on-surface-variant)]">Scroll or drag in the preview to add a zoom keyframe</p>
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
  return (
    <div className="bg-white/[0.04] backdrop-blur-xl rounded-xl border border-white/[0.06] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-3">Background & Layout</h2>
      <div className="space-y-3">
        <div>
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>Video Size</span>
            <span>{backgroundConfig.scale}%</span>
          </label>
          <input type="range" min="50" max="100" value={backgroundConfig.scale}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, scale: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div>
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>Roundness</span>
            <span>{backgroundConfig.borderRadius}px</span>
          </label>
          <input type="range" min="0" max="64" value={backgroundConfig.borderRadius}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, borderRadius: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div>
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>Shadow</span>
            <span>{backgroundConfig.shadow || 0}px</span>
          </label>
          <input type="range" min="0" max="100" value={backgroundConfig.shadow || 0}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, shadow: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div>
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>Volume</span>
            <span>{Math.round((backgroundConfig.volume ?? 1) * 100)}%</span>
          </label>
          <input type="range" min="0" max="1" step="0.01" value={backgroundConfig.volume ?? 1}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, volume: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div>
          <label className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-2 block">Background Style</label>
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

            <label className="aspect-square h-10 rounded-lg transition-all duration-150 cursor-pointer ring-1 ring-white/[0.08] hover:ring-[var(--primary-color)]/40 hover:scale-105 relative overflow-hidden group bg-white/[0.04]">
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
  return (
    <div className="bg-white/[0.04] backdrop-blur-xl rounded-xl border border-white/[0.06] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)] mb-3">Cursor Settings</h2>
      <div className="space-y-3">
        <div>
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>Cursor Size</span>
            <span>{backgroundConfig.cursorScale ?? 2}x</span>
          </label>
          <input type="range" min="1" max="8" step="0.1" value={backgroundConfig.cursorScale ?? 2}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorScale: Number(e.target.value) }))}
            className="w-full"
          />
        </div>
        <div>
          <label className="text-xs text-[var(--on-surface-variant)] mb-2 flex justify-between">
            <span>Movement Smoothing</span>
            <span>{backgroundConfig.cursorSmoothness ?? 5}</span>
          </label>
          <input type="range" min="0" max="10" step="1" value={backgroundConfig.cursorSmoothness ?? 5}
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
  onAddText: () => void;
  onUpdateSegment: (segment: VideoSegment) => void;
}

function TextPanel({ segment, editingTextId, onAddText, onUpdateSegment }: TextPanelProps) {
  const editingText = editingTextId ? segment?.textSegments?.find(t => t.id === editingTextId) : null;

  return (
    <div className="bg-white/[0.04] backdrop-blur-xl rounded-xl border border-white/[0.06] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="flex justify-between items-center mb-3">
        <h2 className="text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)]">Text Overlay</h2>
        <Button onClick={onAddText} className="bg-[var(--primary-color)] hover:bg-[var(--primary-color)]/85 text-white rounded-lg text-xs transition-colors">
          <Type className="w-3.5 h-3.5 mr-1.5" />Add Text
        </Button>
      </div>

      {editingText && segment ? (
        <div className="space-y-3">
          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">Text Content</label>
            <textarea
              value={editingText.text}
              onChange={(e) => {
                onUpdateSegment({
                  ...segment,
                  textSegments: segment.textSegments.map(t =>
                    t.id === editingTextId ? { ...t, text: e.target.value } : t
                  )
                });
              }}
              className="w-full bg-white/[0.04] border border-white/[0.08] rounded-lg px-3 py-2 text-[var(--on-surface)] text-sm focus:border-[var(--primary-color)]/50 focus:ring-1 focus:ring-[var(--primary-color)]/30 transition-colors"
              rows={3}
            />
          </div>

          <p className="text-[10px] text-[var(--on-surface-variant)]">Drag text in preview to reposition</p>

          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">Font Size</label>
              <select
                value={editingText.style.fontSize}
                onChange={(e) => {
                  onUpdateSegment({
                    ...segment,
                    textSegments: segment.textSegments.map(t =>
                      t.id === editingTextId ? { ...t, style: { ...t.style, fontSize: Number(e.target.value) } } : t
                    )
                  });
                }}
                className="w-full bg-white/[0.04] border border-white/[0.08] rounded-lg px-3 py-2 text-[var(--on-surface)] text-sm"
              >
                {[16, 24, 32, 48, 64, 80, 96, 128, 160, 200].map(size => (
                  <option key={size} value={size}>{size}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">Color</label>
              <input
                type="color"
                value={editingText.style.color}
                onChange={(e) => {
                  onUpdateSegment({
                    ...segment,
                    textSegments: segment.textSegments.map(t =>
                      t.id === editingTextId ? { ...t, style: { ...t.style, color: e.target.value } } : t
                    )
                  });
                }}
                className="w-12 h-10 bg-white/[0.04] border border-white/[0.08] rounded-lg p-1"
              />
            </div>
          </div>
        </div>
      ) : (
        <p className="text-xs text-[var(--on-surface-variant)]">Add a text overlay or select one from the timeline</p>
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
  onAddText: () => void;
  onUpdateSegment: (segment: VideoSegment) => void;
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
  onAddText,
  onUpdateSegment
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
          onAddText={onAddText}
          onUpdateSegment={onUpdateSegment}
        />
      )}
    </div>
  );
}
