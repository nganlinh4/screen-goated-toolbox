import { Button } from '@/components/ui/button';
import { Trash2, Search, Upload, Type } from 'lucide-react';
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
    <div className="flex bg-[#272729] p-0.5 rounded-md">
      {tabs.map(tab => (
        <Button
          key={tab.id}
          onClick={() => onPanelChange(tab.id)}
          variant={activePanel === tab.id ? 'default' : 'outline'}
          size="sm"
          className={`flex-1 ${
            activePanel === tab.id
              ? 'bg-[#1a1a1b] text-[#d7dadc] border-0'
              : 'bg-transparent text-[#818384] border-0 hover:bg-[#1a1a1b]/10 hover:text-[#d7dadc]'
          }`}
        >
          {tab.label}
        </Button>
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
      <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-4">
        <div className="flex justify-between items-center mb-4">
          <h2 className="text-base font-semibold text-[#d7dadc]">Zoom Configuration</h2>
          <Button
            onClick={onDeleteKeyframe}
            variant="ghost"
            size="icon"
            className="text-[#d7dadc] hover:text-red-400 hover:bg-red-400/10 transition-colors"
          >
            <Trash2 className="w-5 h-5" />
          </Button>
        </div>
        <div className="space-y-4">
          <div>
            <label className="text-sm font-medium text-[#d7dadc] mb-2">Zoom Factor</label>
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
                className="w-full accent-[#0079d3]"
              />
              <div className="flex justify-between text-xs text-[#818384] font-medium">
                <span>1x</span>
                <span>{zoomFactor.toFixed(1)}x</span>
                <span>3x</span>
              </div>
            </div>
          </div>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
                <span>Horizontal Position</span>
                <span className="text-[#818384]">{Math.round((keyframe?.positionX ?? 0.5) * 100)}%</span>
              </label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={keyframe?.positionX ?? 0.5}
                onChange={(e) => onUpdateZoom({ positionX: Number(e.target.value) })}
                className="w-full accent-[#0079d3]"
              />
            </div>
            <div>
              <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
                <span>Vertical Position</span>
                <span className="text-[#818384]">{Math.round((keyframe?.positionY ?? 0.5) * 100)}%</span>
              </label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={keyframe?.positionY ?? 0.5}
                onChange={(e) => onUpdateZoom({ positionY: Number(e.target.value) })}
                className="w-full accent-[#0079d3]"
              />
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-6 flex flex-col items-center justify-center text-center">
      <div className="bg-[#272729] rounded-full p-3 mb-3">
        <Search className="w-6 h-6 text-[#818384]" />
      </div>
      <p className="text-[#d7dadc] font-medium">This Area Doesn't Have Manual Zoom</p>
      <p className="text-[#818384] text-sm mt-1 max-w-[200px]">
        Use your scroll wheel or drag inside the video player to add one
      </p>
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
    <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-4">
      <h2 className="text-base font-semibold text-[#d7dadc] mb-4">Background & Layout</h2>
      <div className="space-y-4">
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
            <span>Video Size</span>
            <span className="text-[#818384]">{backgroundConfig.scale}%</span>
          </label>
          <input type="range" min="50" max="100" value={backgroundConfig.scale}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, scale: Number(e.target.value) }))}
            className="w-full accent-[#0079d3]"
          />
        </div>
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
            <span>Roundness</span>
            <span className="text-[#818384]">{backgroundConfig.borderRadius}px</span>
          </label>
          <input type="range" min="0" max="64" value={backgroundConfig.borderRadius}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, borderRadius: Number(e.target.value) }))}
            className="w-full accent-[#0079d3]"
          />
        </div>
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
            <span>Shadow</span>
            <span className="text-[#818384]">{backgroundConfig.shadow || 0}px</span>
          </label>
          <input type="range" min="0" max="100" value={backgroundConfig.shadow || 0}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, shadow: Number(e.target.value) }))}
            className="w-full accent-[#0079d3]"
          />
        </div>
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
            <span>Volume</span>
            <span className="text-[#818384]">{Math.round((backgroundConfig.volume ?? 1) * 100)}%</span>
          </label>
          <input type="range" min="0" max="1" step="0.01" value={backgroundConfig.volume ?? 1}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, volume: Number(e.target.value) }))}
            className="w-full accent-[#0079d3]"
          />
        </div>
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-3 block">Background Style</label>
          <div className="grid grid-cols-4 gap-4">
            {Object.entries(GRADIENT_PRESETS).map(([key, gradient]) => (
              <button
                key={key}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: key as BackgroundConfig['backgroundType'] }))}
                className={`aspect-square h-10 rounded-lg transition-all ${gradient} ${
                  backgroundConfig.backgroundType === key
                    ? 'ring-2 ring-[#0079d3] ring-offset-2 ring-offset-[#1a1a1b] scale-105'
                    : 'ring-1 ring-[#343536] hover:ring-[#0079d3]/50'
                }`}
              />
            ))}

            <label className="aspect-square h-10 rounded-lg transition-all cursor-pointer ring-1 ring-[#343536] hover:ring-[#0079d3]/50 relative overflow-hidden group bg-[#272729]">
              <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" />
              <div className="absolute inset-0 flex items-center justify-center">
                <Upload className="w-5 h-5 text-[#818384] group-hover:text-[#0079d3] transition-colors" />
              </div>
            </label>

            {recentUploads.map((imageUrl, index) => (
              <button
                key={index}
                onClick={() => setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: imageUrl }))}
                className={`aspect-square h-10 rounded-lg transition-all relative overflow-hidden ${
                  backgroundConfig.backgroundType === 'custom' && backgroundConfig.customBackground === imageUrl
                    ? 'ring-2 ring-[#0079d3] ring-offset-2 ring-offset-[#1a1a1b] scale-105'
                    : 'ring-1 ring-[#343536] hover:ring-[#0079d3]/50'
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
    <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-4">
      <h2 className="text-base font-semibold text-[#d7dadc] mb-4">Cursor Settings</h2>
      <div className="space-y-4">
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
            <span>Cursor Size</span>
            <span className="text-[#818384]">{backgroundConfig.cursorScale ?? 2}x</span>
          </label>
          <input type="range" min="1" max="8" step="0.1" value={backgroundConfig.cursorScale ?? 2}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorScale: Number(e.target.value) }))}
            className="w-full accent-[#0079d3]"
          />
        </div>
        <div>
          <label className="text-sm font-medium text-[#d7dadc] mb-2 flex justify-between">
            <span>Movement Smoothing</span>
            <span className="text-[#818384]">{backgroundConfig.cursorSmoothness ?? 5}</span>
          </label>
          <input type="range" min="0" max="10" step="1" value={backgroundConfig.cursorSmoothness ?? 5}
            onChange={(e) => setBackgroundConfig(prev => ({ ...prev, cursorSmoothness: Number(e.target.value) }))}
            className="w-full accent-[#0079d3]"
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
    <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-4">
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-base font-semibold text-[#d7dadc]">Text Overlay</h2>
        <Button onClick={onAddText} className="bg-[#0079d3] hover:bg-[#0079d3]/90 text-white">
          <Type className="w-4 h-4 mr-2" />Add Text
        </Button>
      </div>

      {editingText && segment ? (
        <div className="space-y-4">
          <div>
            <label className="text-sm font-medium text-[#d7dadc] mb-2 block">Text Content</label>
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
              className="w-full bg-[#272729] border border-[#343536] rounded-md px-3 py-2 text-[#d7dadc]"
              rows={3}
            />
          </div>

          <div className="bg-[#272729] rounded-lg p-3 text-sm text-[#818384]">
            <p className="flex items-center gap-2">
              <span className="bg-[#343536] rounded-full p-1"><Type className="w-4 h-4" /></span>
              Drag text to reposition
            </p>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-sm font-medium text-[#d7dadc] mb-2 block">Font Size</label>
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
                className="w-full bg-[#272729] border border-[#343536] rounded-md px-3 py-2 text-[#d7dadc]"
              >
                {[16, 24, 32, 48, 64, 80, 96, 128, 160, 200].map(size => (
                  <option key={size} value={size}>{size}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-sm font-medium text-[#d7dadc] mb-2 block">Color</label>
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
                className="w-12 h-10 bg-[#272729] border border-[#343536] rounded-md p-1"
              />
            </div>
          </div>
        </div>
      ) : (
        <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-6 flex flex-col items-center justify-center text-center">
          <div className="bg-[#272729] rounded-full p-3 mb-3"><Type className="w-6 h-6 text-[#818384]" /></div>
          <p className="text-[#d7dadc] font-medium">No Text Selected</p>
          <p className="text-[#818384] text-sm mt-1 max-w-[200px]">
            Add a new text overlay or select an existing one from the timeline
          </p>
        </div>
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
    <div className="col-span-1 space-y-3">
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
