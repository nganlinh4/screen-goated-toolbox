import { VideoSegment, BackgroundConfig } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';
import { ZoomPanel } from './ZoomPanel';
import { BackgroundPanel } from './BackgroundPanel';
import { CursorPanel } from './CursorPanel';
import { TextPanel } from './TextPanel';
import { BlurPanel } from './BlurPanel';

// ============================================================================
// Types
// ============================================================================
export type ActivePanel = 'zoom' | 'background' | 'cursor' | 'blur' | 'text';

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
    { id: 'blur', label: t.tabBlur },
    { id: 'text', label: t.tabText }
  ];

  return (
    <div className="panel-tabs flex flex-nowrap border-b border-[var(--glass-border)]">
      {tabs.map(tab => (
        <button
          key={tab.id}
          onClick={() => onPanelChange(tab.id)}
          className={`panel-tab-button flex-1 px-2 py-2 text-[11px] font-medium whitespace-nowrap transition-colors relative ${
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
  onRemoveRecentUpload: (imageUrl: string) => void;
  onBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  isBackgroundUploadProcessing: boolean;
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
  onRemoveRecentUpload,
  onBackgroundUpload,
  isBackgroundUploadProcessing,
  editingTextId,
  onUpdateSegment,
  beginBatch,
  commitBatch
}: SidePanelProps) {
  return (
    <div className="side-panel h-full min-h-0 flex flex-col">
      <PanelTabs activePanel={activePanel} onPanelChange={setActivePanel} />
      <div className="side-panel-content mt-3 flex-1 min-h-0 overflow-y-auto thin-scrollbar px-2 pb-2">
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
          onRemoveRecentUpload={onRemoveRecentUpload}
          onBackgroundUpload={onBackgroundUpload}
          isBackgroundUploadProcessing={isBackgroundUploadProcessing}
        />
        )}

        {activePanel === 'cursor' && (
          <CursorPanel
            segment={segment}
            onUpdateSegment={onUpdateSegment}
            backgroundConfig={backgroundConfig}
            setBackgroundConfig={setBackgroundConfig}
          />
        )}

        {activePanel === 'blur' && (
          <BlurPanel
            backgroundConfig={backgroundConfig}
            setBackgroundConfig={setBackgroundConfig}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
          />
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
    </div>
  );
}
