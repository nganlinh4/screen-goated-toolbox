import { VideoSegment, BackgroundConfig } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';
import { ZoomPanel } from './ZoomPanel';
import { BackgroundPanel } from './BackgroundPanel';
import { CursorPanel } from './CursorPanel';
import { TextPanel } from './TextPanel';
import { BlurPanel } from './BlurPanel';
import { motion } from 'framer-motion';

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
    <div className="panel-tabs ui-segmented relative flex flex-nowrap overflow-hidden">
      {tabs.map(tab => (
        <button
          key={tab.id}
          onClick={() => onPanelChange(tab.id)}
          className={`panel-tab-button ui-segmented-button relative flex-1 px-2 py-2 text-[11px] font-medium whitespace-nowrap ${
            activePanel === tab.id
              ? 'text-[var(--primary-color)]'
              : ''
          }`}
        >
          {activePanel === tab.id && (
            <motion.span
              layoutId="side-panel-tab-pill"
              className="panel-tab-pill absolute inset-0 rounded-[10px] border"
              style={{
                background:
                  "color-mix(in srgb, var(--primary-color) 12%, var(--ui-surface-3))",
                borderColor:
                  "color-mix(in srgb, var(--primary-color) 36%, var(--ui-border))",
                boxShadow: "var(--shadow-elevation-1)",
              }}
              transition={{ type: "spring", stiffness: 420, damping: 36, mass: 0.9 }}
            />
          )}
          <span className="panel-tab-label relative z-10">{tab.label}</span>
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
  const panelOrder: ActivePanel[] = ['background', 'zoom', 'cursor', 'blur', 'text'];
  const activePanelIndex = panelOrder.indexOf(activePanel);

  const renderPanel = (panelId: ActivePanel) => {
    if (panelId === 'zoom') {
      return (
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
      );
    }

    if (panelId === 'background') {
      return (
        <BackgroundPanel
          backgroundConfig={backgroundConfig}
          setBackgroundConfig={setBackgroundConfig}
          recentUploads={recentUploads}
          onRemoveRecentUpload={onRemoveRecentUpload}
          onBackgroundUpload={onBackgroundUpload}
          isBackgroundUploadProcessing={isBackgroundUploadProcessing}
        />
      );
    }

    if (panelId === 'cursor') {
      return (
        <CursorPanel
          segment={segment}
          onUpdateSegment={onUpdateSegment}
          backgroundConfig={backgroundConfig}
          setBackgroundConfig={setBackgroundConfig}
        />
      );
    }

    if (panelId === 'blur') {
      return (
        <BlurPanel
          backgroundConfig={backgroundConfig}
          setBackgroundConfig={setBackgroundConfig}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
        />
      );
    }

    return (
      <TextPanel
        segment={segment}
        editingTextId={editingTextId}
        onUpdateSegment={onUpdateSegment}
        beginBatch={beginBatch}
        commitBatch={commitBatch}
      />
    );
  };

  return (
    <div className="side-panel h-full min-h-0 flex flex-col">
      <PanelTabs activePanel={activePanel} onPanelChange={setActivePanel} />
      <div className="side-panel-content mt-3 flex-1 min-h-0 overflow-hidden px-2 pb-2">
        <div className="side-panel-panels relative h-full">
          {panelOrder.map((panelId, index) => {
            const relativeIndex = index - activePanelIndex;
            const isActive = relativeIndex === 0;

            return (
              <motion.div
                key={panelId}
                className="side-panel-pane absolute inset-0 overflow-y-auto thin-scrollbar pr-1 pb-2"
                initial={false}
                animate={{
                  x:
                    relativeIndex === 0
                      ? "0%"
                      : relativeIndex < 0
                        ? "-108%"
                        : "108%",
                  opacity: isActive ? 1 : 0.72,
                  scale: isActive ? 1 : 0.985,
                }}
                transition={{
                  x: { type: "spring", stiffness: 360, damping: 34, mass: 0.9 },
                  opacity: { duration: 0.24, ease: [0.22, 1, 0.36, 1] },
                  scale: { duration: 0.24, ease: [0.22, 1, 0.36, 1] },
                }}
                style={{
                  pointerEvents: isActive ? "auto" : "none",
                }}
              >
                {renderPanel(panelId)}
              </motion.div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
