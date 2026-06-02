import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { PanelCard } from '@/components/layout/PanelCard';
import { useSettings } from '@/hooks/useSettings';
import type { useSubtitleTranslation } from '@/hooks/useSubtitleTranslation';
import { normalizeTextStyle } from '@/lib/textStyleDefaults';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import {
  splitSubtitleIdsAcrossTracks,
  updateSubtitleStylesAcrossTracks,
  updateSubtitleTextsOnActiveTrack,
} from '@/lib/subtitleTrackMutations';
import type { ImportedAudioSegment, SubtitleSegment, VideoSegment } from '@/types/video';
import { SubtitleGenerationSettings } from './subtitle-panel/SubtitleGenerationSettings';
import { SubtitleStyleControls } from './subtitle-panel/SubtitleStyleControls';
import { SubtitleTranslationSettings } from './subtitle-panel/SubtitleTranslationSettings';

export interface SubtitlePanelProps {
  segment: VideoSegment | null;
  editingSubtitleId: string | null;
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  selectedSource: SubtitleSource;
  onSourceChange: (value: SubtitleSource) => void;
  selectedMethod: SubtitleMethod;
  onMethodChange: (value: SubtitleMethod) => void;
  methodCapabilities: Array<{ method: SubtitleMethod; available: boolean; reason?: string | null }>;
  canUseSelectedMethod: boolean;
  selectedMethodReason?: string | null;
  languageHint: string;
  onLanguageHintChange: (value: string) => void;
  geminiPrompt: string;
  onGeminiPromptChange: (value: string) => void;
  groqVocabulary: string[];
  onGroqVocabularyChange: (value: string[]) => void;
  autoSplitSubtitles: boolean;
  onAutoSplitSubtitlesChange: (value: boolean) => void;
  autoSplitSubtitleMaxUnits: number;
  onAutoSplitSubtitleMaxUnitsChange: (value: number) => void;
  isGenerating: boolean;
  statusMessage?: string | null;
  canUseVideoSource: boolean;
  canUseMicSource: boolean;
  canUseAudioSource: boolean;
  audioSegments?: ImportedAudioSegment[];
  onGenerate: () => void;
  onCancel: () => void;
  canExportSrt: boolean;
  onExportSrt: () => void;
  canExportAudioSrt: boolean;
  onExportAudioSrt: () => void;
  subtitleTranslation: ReturnType<typeof useSubtitleTranslation>;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function SubtitlePanel({
  segment,
  editingSubtitleId,
  selectedSubtitleIds,
  selectedSubtitleRange,
  selectedSource,
  onSourceChange,
  selectedMethod,
  onMethodChange,
  methodCapabilities,
  canUseSelectedMethod,
  selectedMethodReason,
  languageHint,
  onLanguageHintChange,
  geminiPrompt,
  onGeminiPromptChange,
  groqVocabulary,
  onGroqVocabularyChange,
  autoSplitSubtitles,
  onAutoSplitSubtitlesChange,
  autoSplitSubtitleMaxUnits,
  onAutoSplitSubtitleMaxUnitsChange,
  isGenerating,
  statusMessage,
  canUseVideoSource,
  canUseMicSource,
  canUseAudioSource,
  audioSegments = [],
  onGenerate,
  onCancel,
  canExportSrt,
  onExportSrt,
  canExportAudioSrt,
  onExportAudioSrt,
  subtitleTranslation,
  onUpdateSegment,
  beginBatch,
  commitBatch,
}: SubtitlePanelProps) {
  const { t } = useSettings();
  const visibleSubtitles = subtitleTranslation.visibleSubtitleSegments;
  const selectedSubtitleCount = selectedSubtitleIds?.length ?? 0;
  const hasSelection = selectedSubtitleCount > 0;
  const selection = hasSelection ? new Set(selectedSubtitleIds ?? []) : null;
  const sourceId = hasSelection ? selectedSubtitleIds?.[0] ?? null : editingSubtitleId;
  const sourceSubtitle = sourceId
    ? visibleSubtitles.find((subtitle) => subtitle.id === sourceId) ?? null
    : null;
  const resolvedStyle = sourceSubtitle ? normalizeTextStyle(sourceSubtitle.style) : null;
  const editableSubtitles = selection
    ? visibleSubtitles.filter((subtitle) => selection.has(subtitle.id))
    : sourceSubtitle
      ? [sourceSubtitle]
      : [];

  const updateSelectedSubtitles = (updater: (subtitle: SubtitleSegment) => SubtitleSegment) => {
    if (!segment || !sourceSubtitle) return;
    const targetIds = selection ?? new Set([sourceSubtitle.id]);
    onUpdateSegment(updateSubtitleStylesAcrossTracks(segment, targetIds, updater));
  };

  const updateSubtitleText = (text: string) => {
    if (!segment || !sourceSubtitle || subtitleTranslation.isCustomSubtitleView) return;
    const targetIds = selection ?? new Set([sourceSubtitle.id]);
    onUpdateSegment(updateSubtitleTextsOnActiveTrack(segment, targetIds, (subtitle) => ({
      ...subtitle,
      text,
    })));
  };

  const splitSelectedSubtitles = (maxUnits: number) => {
    if (!segment || editableSubtitles.length === 0 || subtitleTranslation.isCustomSubtitleView) return;
    beginBatch();
    onUpdateSegment(splitSubtitleIdsAcrossTracks(
      segment,
      editableSubtitles.map((subtitle) => subtitle.id),
      maxUnits,
    ));
    commitBatch();
  };

  return (
    <PanelCard className="subtitle-panel">
      <div className="subtitle-panel-body space-y-3">
        <p className="subtitle-panel-hint text-[11px] leading-4 text-on-surface-variant">{t.subtitlePanelHint}</p>

        <SubtitleGenerationSettings
          t={t}
          visibleSubtitleCount={visibleSubtitles.length}
          selectedSubtitleRange={selectedSubtitleRange}
          selectedSource={selectedSource}
          onSourceChange={onSourceChange}
          selectedMethod={selectedMethod}
          onMethodChange={onMethodChange}
          methodCapabilities={methodCapabilities}
          canUseSelectedMethod={canUseSelectedMethod}
          selectedMethodReason={selectedMethodReason}
          languageHint={languageHint}
          onLanguageHintChange={onLanguageHintChange}
          geminiPrompt={geminiPrompt}
          onGeminiPromptChange={onGeminiPromptChange}
          groqVocabulary={groqVocabulary}
          onGroqVocabularyChange={onGroqVocabularyChange}
          autoSplitSubtitles={autoSplitSubtitles}
          onAutoSplitSubtitlesChange={onAutoSplitSubtitlesChange}
          autoSplitSubtitleMaxUnits={autoSplitSubtitleMaxUnits}
          onAutoSplitSubtitleMaxUnitsChange={onAutoSplitSubtitleMaxUnitsChange}
          isGenerating={isGenerating}
          statusMessage={statusMessage}
          canUseVideoSource={canUseVideoSource}
          canUseMicSource={canUseMicSource}
          canUseAudioSource={canUseAudioSource}
          audioSegments={audioSegments}
          onGenerate={onGenerate}
          onCancel={onCancel}
          canExportSrt={canExportSrt}
          onExportSrt={onExportSrt}
          canExportAudioSrt={canExportAudioSrt}
          onExportAudioSrt={onExportAudioSrt}
          subtitleTranslation={subtitleTranslation}
        />

        <SubtitleTranslationSettings
          t={t}
          subtitleTranslation={subtitleTranslation}
        />

        {sourceSubtitle && editableSubtitles.length > 0 && resolvedStyle ? (
          <SubtitleStyleControls
            t={t}
            sourceSubtitle={sourceSubtitle}
            editableSubtitles={editableSubtitles}
            resolvedStyle={resolvedStyle}
            hasSelection={hasSelection}
            selectedSubtitleCount={selectedSubtitleCount}
            isCustomSubtitleView={subtitleTranslation.isCustomSubtitleView}
            onUpdateSubtitleText={updateSubtitleText}
            onUpdateSelectedSubtitles={updateSelectedSubtitles}
            onSplitSelectedSubtitles={splitSelectedSubtitles}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
          />
        ) : null}
      </div>
    </PanelCard>
  );
}
