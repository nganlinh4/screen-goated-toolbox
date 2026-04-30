import { useEffect, useRef } from 'react';
import type { VideoSegment, BackgroundConfig, ImportedAudioSegment } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';
import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { useSubtitleTranslation } from '@/hooks/useSubtitleTranslation';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import { ZoomPanel } from './ZoomPanel';
import { CameraPanel } from './CameraPanel';
import { BackgroundPanel } from './BackgroundPanel';
import { CursorPanel } from './CursorPanel';
import { SubtitlePanel } from './SubtitlePanel';
import { TextPanel } from './TextPanel';
import { BlurPanel } from './BlurPanel';
import { motion } from 'framer-motion';
import type { WebcamConfig } from '@/types/video';

// ============================================================================
// Types
// ============================================================================
export type ActivePanel =
  | 'zoom'
  | 'camera'
  | 'background'
  | 'cursor'
  | 'blur'
  | 'subtitles'
  | 'text';

const PANEL_TAB_ORDER: ActivePanel[] = ['zoom', 'camera', 'background', 'cursor', 'blur', 'subtitles', 'text'];

// ============================================================================
// PanelTabs
// ============================================================================
interface PanelTabsProps {
  activePanel: ActivePanel;
  onPanelChange: (panel: ActivePanel) => void;
  hiddenTabs?: Set<ActivePanel>;
}

function PanelTabs({ activePanel, onPanelChange, hiddenTabs }: PanelTabsProps) {
  const { t } = useSettings();
  const tabs: { id: ActivePanel; label: string }[] = PANEL_TAB_ORDER
    .filter((id) => !hiddenTabs?.has(id))
    .map((id) => ({
    id,
    label:
      id === 'zoom'
        ? t.tabZoom
        : id === 'camera'
          ? t.tabCamera
        : id === 'background'
          ? t.tabBackground
          : id === 'cursor'
            ? t.tabCursor
            : id === 'blur'
              ? t.tabBlur
              : id === 'subtitles'
                ? t.tabSubtitles
              : t.tabText,
  }));

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
  webcamConfig: WebcamConfig;
  setWebcamConfig: React.Dispatch<React.SetStateAction<WebcamConfig>>;
  webcamAvailable: boolean;
  recentUploads: string[];
  onRemoveRecentUpload: (imageUrl: string) => void;
  onBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  isBackgroundUploadProcessing: boolean;
  editingTextId: string | null;
  editingSubtitleId: string | null;
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  subtitleSource: SubtitleSource;
  onSubtitleSourceChange: (value: SubtitleSource) => void;
  subtitleMethod: SubtitleMethod;
  onSubtitleMethodChange: (value: SubtitleMethod) => void;
  subtitleMethodCapabilities: Array<{ method: SubtitleMethod; available: boolean; reason?: string | null }>;
  canUseSelectedSubtitleMethod: boolean;
  selectedSubtitleMethodReason?: string | null;
  subtitleLanguageHint: string;
  onSubtitleLanguageHintChange: (value: string) => void;
  subtitleGeminiPrompt: string;
  onSubtitleGeminiPromptChange: (value: string) => void;
  subtitleGroqVocabulary: string[];
  onSubtitleGroqVocabularyChange: (value: string[]) => void;
  isGeneratingSubtitles: boolean;
  subtitleStatusMessage?: string | null;
  canUseVideoSubtitleSource: boolean;
  canUseMicSubtitleSource: boolean;
  canUseAudioSubtitleSource: boolean;
  audioSegments?: ImportedAudioSegment[];
  onGenerateSubtitles: () => void;
  onCancelSubtitleGeneration: () => void;
  canExportSubtitleSrt: boolean;
  onExportSubtitleSrt: () => void;
  canExportAudioSubtitleSrt: boolean;
  onExportMusicSubtitleSrt: () => void;
  subtitleTranslation: ReturnType<typeof useSubtitleTranslation>;
  selectedTextIds?: string[];
  hasMouseData?: boolean;
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
  webcamConfig,
  setWebcamConfig,
  webcamAvailable,
  recentUploads,
  onRemoveRecentUpload,
  onBackgroundUpload,
  isBackgroundUploadProcessing,
  editingTextId,
  editingSubtitleId,
  selectedSubtitleIds,
  selectedSubtitleRange,
  subtitleSource,
  onSubtitleSourceChange,
  subtitleMethod,
  onSubtitleMethodChange,
  subtitleMethodCapabilities,
  canUseSelectedSubtitleMethod,
  selectedSubtitleMethodReason,
  subtitleLanguageHint,
  onSubtitleLanguageHintChange,
  subtitleGeminiPrompt,
  onSubtitleGeminiPromptChange,
  subtitleGroqVocabulary,
  onSubtitleGroqVocabularyChange,
  isGeneratingSubtitles,
  subtitleStatusMessage,
  canUseVideoSubtitleSource,
  canUseMicSubtitleSource,
  canUseAudioSubtitleSource,
  audioSegments,
  onGenerateSubtitles,
  onCancelSubtitleGeneration,
  canExportSubtitleSrt,
  onExportSubtitleSrt,
  canExportAudioSubtitleSrt,
  onExportMusicSubtitleSrt,
  subtitleTranslation,
  selectedTextIds,
  hasMouseData,
  onUpdateSegment,
  beginBatch,
  commitBatch
}: SidePanelProps) {
  const hasZoomFocus = editingKeyframeId !== null;
  const hasTextFocus = !!editingTextId || (selectedTextIds?.length ?? 0) > 0;
  const hasSubtitlePanel = !!segment;

  const hiddenTabs = new Set<ActivePanel>();
  if (!hasMouseData) hiddenTabs.add('cursor');
  if (!webcamAvailable) hiddenTabs.add('camera');
  if (!hasZoomFocus) hiddenTabs.add('zoom');
  if (!hasTextFocus) hiddenTabs.add('text');
  if (!hasSubtitlePanel) hiddenTabs.add('subtitles');
  const visiblePanelOrder = PANEL_TAB_ORDER.filter((id) => !hiddenTabs.has(id));
  // If active panel got hidden, fall back to first visible tab
  const effectivePanel = hiddenTabs.has(activePanel) ? (visiblePanelOrder[0] ?? 'background') : activePanel;
  // Use canonical order for slide direction so panels always slide consistently
  const canonicalActiveIndex = PANEL_TAB_ORDER.indexOf(effectivePanel);
  // Track previous active index to determine entry direction for newly mounted panels
  const prevCanonicalActiveRef = useRef(canonicalActiveIndex);
  useEffect(() => { prevCanonicalActiveRef.current = canonicalActiveIndex; });
  const prevCanonicalActive = prevCanonicalActiveRef.current;

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

    if (panelId === 'camera') {
      return (
        <CameraPanel
          webcamConfig={webcamConfig}
          setWebcamConfig={setWebcamConfig}
          webcamAvailable={webcamAvailable}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
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
          hasMouseData={hasMouseData}
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

    if (panelId === 'subtitles') {
      return (
        <SubtitlePanel
          segment={segment}
          editingSubtitleId={editingSubtitleId}
          selectedSubtitleIds={selectedSubtitleIds}
          selectedSubtitleRange={selectedSubtitleRange}
          selectedSource={subtitleSource}
          onSourceChange={onSubtitleSourceChange}
          selectedMethod={subtitleMethod}
          onMethodChange={onSubtitleMethodChange}
          methodCapabilities={subtitleMethodCapabilities}
          canUseSelectedMethod={canUseSelectedSubtitleMethod}
          selectedMethodReason={selectedSubtitleMethodReason}
          languageHint={subtitleLanguageHint}
          onLanguageHintChange={onSubtitleLanguageHintChange}
          geminiPrompt={subtitleGeminiPrompt}
          onGeminiPromptChange={onSubtitleGeminiPromptChange}
          groqVocabulary={subtitleGroqVocabulary}
          onGroqVocabularyChange={onSubtitleGroqVocabularyChange}
          isGenerating={isGeneratingSubtitles}
          statusMessage={subtitleStatusMessage}
          canUseVideoSource={canUseVideoSubtitleSource}
          canUseMicSource={canUseMicSubtitleSource}
          canUseAudioSource={canUseAudioSubtitleSource}
          audioSegments={audioSegments}
          onGenerate={onGenerateSubtitles}
          onCancel={onCancelSubtitleGeneration}
          canExportSrt={canExportSubtitleSrt}
          onExportSrt={onExportSubtitleSrt}
          canExportAudioSrt={canExportAudioSubtitleSrt}
          onExportAudioSrt={onExportMusicSubtitleSrt}
          subtitleTranslation={subtitleTranslation}
          onUpdateSegment={onUpdateSegment}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
        />
      );
    }

    return (
      <TextPanel
        segment={segment}
        editingTextId={editingTextId}
        selectedTextIds={selectedTextIds}
        onUpdateSegment={onUpdateSegment}
        beginBatch={beginBatch}
        commitBatch={commitBatch}
      />
    );
  };

  return (
    <div className="side-panel h-full min-h-0 flex flex-col">
      <PanelTabs activePanel={effectivePanel} onPanelChange={setActivePanel} hiddenTabs={hiddenTabs} />
      <div className="side-panel-content mt-3 flex-1 min-h-0 overflow-hidden px-2 pb-2">
        <div className="side-panel-panels relative h-full">
          {visiblePanelOrder.map((panelId) => {
            const canonicalIndex = PANEL_TAB_ORDER.indexOf(panelId);
            const relativeIndex = canonicalIndex - canonicalActiveIndex;
            const isActive = relativeIndex === 0;

            return (
              <motion.div
                key={panelId}
                className="side-panel-pane absolute inset-0 overflow-y-auto thin-scrollbar pr-1 pb-2"
                initial={{
                  x: canonicalIndex < prevCanonicalActive ? "-108%" : "108%",
                  opacity: 0.72,
                  scale: 0.985,
                }}
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
