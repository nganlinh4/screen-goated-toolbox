import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { PanelCard } from '@/components/layout/PanelCard';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { LabelledField } from '@/components/ui/FieldLabelContext';
import { useSettings } from '@/hooks/useSettings';
import { invoke } from '@/lib/ipc';
import { projectSubtitleForNarration } from '@/lib/compositionSubtitleNarration';
import {
  useSubtitleNarration,
  type SubtitleNarrationGroupPreview,
} from '@/hooks/useSubtitleNarration';
import {
  useS2sNarration,
} from '@/hooks/useS2sNarration';
import {
  useNarrationSettings,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { SUBTITLE_LANGUAGE_OPTIONS } from '@/lib/subtitleLanguageOptions';
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getSubtitleTrackLabel,
} from '@/lib/subtitleTracks';
import type {
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  SubtitleSegment,
  SubtitleTrack,
  SubtitleViewState,
  VideoSegment,
} from '@/types/video';
import {
  normalizeLanguage6393,
  type NarrationLanguageDetectionResponse,
} from './narrationLanguageUtils';
import {
  getInitialDirectVoiceMethod,
  getInitialNarrationGroupTextBudget,
  getInitialNarrationTargetSegmentSec,
  getInitialNarrationMode,
  getInitialReadUnsplitSubtitles,
  persistDirectVoiceMethod,
  persistNarrationGroupTextBudget,
  persistNarrationTargetSegmentSec,
  persistNarrationMode,
  persistReadUnsplitSubtitles,
  getInitialS2sTargetLanguage,
  persistS2sTargetLanguage,
} from './narrationPanelStorage';
import { NarrationModeSections } from './NarrationModeSections';
import { NarrationGeminiSettings } from './NarrationGeminiSettings';
import { NarrationVoiceProviderSettings } from './NarrationVoiceProviderSettings';
import { useNarrationProviderConfigState } from './useNarrationProviderConfigState';

const CURRENT_SUBTITLE_VIEW_SOURCE_ID = 'current-subtitle-view';
const S2S_LANGUAGE_OPTIONS = SUBTITLE_LANGUAGE_OPTIONS
  .filter((option) => option.value !== 'auto')
  .map((option) => ({ value: option.value, label: option.label }));
const S2S_TARGET_LANGUAGES = S2S_LANGUAGE_OPTIONS.map((option) => option.value);
type NarrationMode = 'subtitles' | 's2s';
type DirectVoiceMethod = 's2s' | 'gemini-translate';

interface NarrationPanelProps {
  onPopulateS2sSubtitles?: import('@/hooks/useCompositionSubtitleEditor').S2sSubtitleWriter;
  getNarrationSourceSnapshot: () => import('@/hooks/subtitleSourceOwnership').SubtitleSourceSnapshot;
  projectId: string | null;
  currentProjectIdRef: React.MutableRefObject<string | null>;
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  activeClipId?: string | null;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  visibleSubtitles: SubtitleSegment[];
  /** All available subtitle tracks (original + translations) for the source picker. */
  subtitleTracks?: SubtitleTrack[];
  activeSubtitleView?: SubtitleViewState;
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  onApplyNarrationSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  onFinalizeNarrationSegments: () => void | Promise<void>;
  onNarrationGroupPreviewChange?: (preview: SubtitleNarrationGroupPreview | null) => void;
  selectedSource: SubtitleSource;
  onSourceChange: (value: SubtitleSource) => void;
  canUseVideoSource: boolean;
  canUseMicSource: boolean;
  canUseAudioSource: boolean;
  audioSegments?: ImportedAudioSegment[];
}

export function NarrationPanel({
  onPopulateS2sSubtitles,
  getNarrationSourceSnapshot,
  projectId,
  currentProjectIdRef,
  segment,
  composition,
  activeClipId,
  currentRawVideoPath,
  currentRawMicAudioPath,
  duration,
  visibleSubtitles = [],
  subtitleTracks,
  activeSubtitleView,
  selectedSubtitleIds = [],
  selectedSubtitleRange,
  onApplyNarrationSegments,
  onFinalizeNarrationSegments,
  onNarrationGroupPreviewChange,
  selectedSource,
  onSourceChange,
  canUseVideoSource,
  canUseMicSource,
  canUseAudioSource,
  audioSegments = [],
}: NarrationPanelProps) {
  const sourceSnapshotRef = useRef(getNarrationSourceSnapshot);
  sourceSnapshotRef.current = getNarrationSourceSnapshot;
  const projectSubtitle = useCallback((subtitle: SubtitleSegment) => {
    const source = sourceSnapshotRef.current();
    return projectSubtitleForNarration(source.composition, source.loadedClipId, subtitle);
  }, [composition, segment, activeClipId]);
  const { t, lang } = useSettings();
  const { settings, update, profile, metadata } = useNarrationSettings();

  const availableTracks = subtitleTracks ?? [];
  const preferredSourceTrackId = activeSubtitleView?.kind === 'track'
    ? (activeSubtitleView.trackId ?? ORIGINAL_SUBTITLE_TRACK_ID)
    : CURRENT_SUBTITLE_VIEW_SOURCE_ID;
  const [selectedSourceTrackId, setSelectedSourceTrackId] = useState<string>(
    preferredSourceTrackId,
  );
  const [readUnsplitSubtitles, setReadUnsplitSubtitles] = useState(getInitialReadUnsplitSubtitles);
  const [groupTextBudget, setGroupTextBudget] = useState(getInitialNarrationGroupTextBudget);
  const [targetSegmentSec, setTargetSegmentSec] = useState(getInitialNarrationTargetSegmentSec);
  const [isGroupSliderDragging, setIsGroupSliderDragging] = useState(false);
  const [narrationMode, setNarrationMode] = useState<NarrationMode>(
    () => getInitialNarrationMode(visibleSubtitles.length > 0),
  );
  const [directVoiceMethod, setDirectVoiceMethod] = useState<DirectVoiceMethod>(
    getInitialDirectVoiceMethod,
  );
  const [s2sTargetLanguage, setS2sTargetLanguage] = useState(() =>
    getInitialS2sTargetLanguage(S2S_TARGET_LANGUAGES, lang));
  const isDirectVoiceMode = narrationMode === 's2s';
  const effectiveTtsMethod: NarrationTtsMethod = isDirectVoiceMode
    ? 'GeminiLive'
    : settings.method;

  useEffect(() => {
    persistReadUnsplitSubtitles(readUnsplitSubtitles);
  }, [readUnsplitSubtitles]);

  useEffect(() => {
    persistS2sTargetLanguage(s2sTargetLanguage);
  }, [s2sTargetLanguage]);

  useEffect(() => {
    persistNarrationGroupTextBudget(groupTextBudget);
  }, [groupTextBudget]);

  useEffect(() => {
    persistNarrationTargetSegmentSec(targetSegmentSec);
  }, [targetSegmentSec]);

  useEffect(() => {
    persistNarrationMode(narrationMode);
  }, [narrationMode]);

  useEffect(() => {
    persistDirectVoiceMethod(directVoiceMethod);
  }, [directVoiceMethod]);

  useEffect(() => {
    setSelectedSourceTrackId(preferredSourceTrackId);
  }, [preferredSourceTrackId]);

  useEffect(() => {
    if (selectedSourceTrackId === CURRENT_SUBTITLE_VIEW_SOURCE_ID) return;
    if (
      selectedSourceTrackId !== ORIGINAL_SUBTITLE_TRACK_ID
      && !availableTracks.some((track) => track.id === selectedSourceTrackId)
    ) {
      setSelectedSourceTrackId(ORIGINAL_SUBTITLE_TRACK_ID);
    }
  }, [availableTracks, selectedSourceTrackId]);

  const sourceTrackOptions = useMemo(() => {
    const currentViewOption = activeSubtitleView?.kind === 'custom'
      ? [{ value: CURRENT_SUBTITLE_VIEW_SOURCE_ID, label: t.subtitleTrackCustom }]
      : [];
    if (availableTracks.length === 0) {
      return [
        ...currentViewOption,
        { value: ORIGINAL_SUBTITLE_TRACK_ID, label: t.subtitleTrackOriginal },
      ];
    }
    return [
      ...currentViewOption,
      ...availableTracks.map((track) => ({
        value: track.id,
        label: track.id === ORIGINAL_SUBTITLE_TRACK_ID
          ? t.subtitleTrackOriginal
          : getSubtitleTrackLabel(track),
      })),
    ];
  }, [activeSubtitleView?.kind, availableTracks, t.subtitleTrackCustom, t.subtitleTrackOriginal]);

  const subtitlesFromSelectedTrack = useMemo<SubtitleSegment[]>(() => {
    if (selectedSourceTrackId === CURRENT_SUBTITLE_VIEW_SOURCE_ID) return visibleSubtitles;
    const fromTrack = availableTracks.find((track) => track.id === selectedSourceTrackId);
    if (fromTrack) return fromTrack.segments;
    return visibleSubtitles;
  }, [availableTracks, selectedSourceTrackId, visibleSubtitles]);
  const selectedSourceLanguageCode = useMemo(() => {
    if (selectedSourceTrackId === CURRENT_SUBTITLE_VIEW_SOURCE_ID) return null;
    const fromTrack = availableTracks.find((track) => track.id === selectedSourceTrackId);
    return fromTrack?.targetLanguage ?? null;
  }, [availableTracks, selectedSourceTrackId]);
  const narrationLanguageSampleKey = useMemo(() => {
    if (selectedSourceLanguageCode) return '';
    return subtitlesFromSelectedTrack
      .map((subtitle) => subtitle.text.trim())
      .filter(Boolean)
      .slice(0, 8)
      .join('\n');
  }, [selectedSourceLanguageCode, subtitlesFromSelectedTrack]);
  const [detectedNarrationLanguageCode, setDetectedNarrationLanguageCode] = useState<string | null>(
    null,
  );

  useEffect(() => {
    const declaredLanguageCode = normalizeLanguage6393(selectedSourceLanguageCode);
    if (declaredLanguageCode) {
      setDetectedNarrationLanguageCode(declaredLanguageCode);
      return;
    }
    const sample = narrationLanguageSampleKey.trim();
    if (!sample) {
      setDetectedNarrationLanguageCode(null);
      return;
    }
    let cancelled = false;
    void invoke<NarrationLanguageDetectionResponse>('detect_narration_language', {
      items: sample.split('\n').map((text) => ({ text })),
    })
      .then((response) => {
        if (cancelled) return;
        setDetectedNarrationLanguageCode(normalizeLanguage6393(response.languageCode) ?? null);
      })
      .catch((error) => {
        if (!cancelled) {
          console.warn('[Narration] Failed to detect subtitle language:', error);
          setDetectedNarrationLanguageCode(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [narrationLanguageSampleKey, selectedSourceLanguageCode]);

  const {
    addEdgeVoiceConfig,
    addLanguageCondition,
    availableConditionLanguages,
    availableEdgeVoiceLanguages,
    detectedLanguageLabel,
    edgeVoiceConfigs,
    edgeVoicesByLanguage,
    geminiLanguageConditions,
    geminiModels,
    geminiSpeedOptions,
    geminiVoices,
    googleSpeedOptions,
    isMethodSupportedForDetectedLanguage,
    providerOptions,
    removeEdgeVoiceConfig,
    removeLanguageCondition,
    updateEdgeVoiceConfig,
    updateLanguageCondition,
  } = useNarrationProviderConfigState({
    detectedNarrationLanguageCode,
    metadata,
    settings,
    update,
  });

  const narration = useSubtitleNarration({
    projectId,
    currentProjectIdRef,
    t,
    visibleSubtitles: subtitlesFromSelectedTrack, projectSubtitle,
    selectedSubtitleIds,
    selectedSubtitleRange,
    sourceLanguageCode: selectedSourceLanguageCode,
    profile,
    readUnsplitSubtitles,
    groupTextBudget,
    previewGrouping: isGroupSliderDragging,
    onApplyNarrationSegments,
    onFinalizeNarrationSegments,
  });
  const s2sSourceOptions = [
    { value: 'video', label: t.subtitleSourceVideo, disabled: !canUseVideoSource },
    { value: 'mic', label: t.subtitleSourceMic, disabled: !canUseMicSource },
    { value: 'audio', label: t.subtitleSourceFullAudio, disabled: !canUseAudioSource },
    ...audioSegments.map((audio) => ({
      value: `audio:${audio.id}`,
      label: audio.name || t.subtitleSourceAudio,
      disabled: !audio.rawAudioPath,
    })),
  ];
  const s2sLanguageOptions = S2S_LANGUAGE_OPTIONS;
  const s2s = useS2sNarration({
    getNarrationSourceSnapshot,
    projectId,
    currentProjectIdRef,
    backendMode: directVoiceMethod,
    t,
    segment,
    composition,
    activeClipId,
    currentRawVideoPath,
    currentRawMicAudioPath,
    duration,
    sourceType: selectedSource,
    selectedRange: selectedSubtitleRange,
    targetLanguage: s2sTargetLanguage,
    geminiModel: profile.geminiModel,
    geminiVoice: profile.geminiVoice,
    geminiSpeed: profile.geminiSpeed,
    parallelRequests: profile.geminiS2sParallelRequests,
    groupTextBudget,
    targetSegmentSec,
    onApplyNarrationSegments,
    onPopulateEmptySubtitles: (...args) => {
      if (!onPopulateS2sSubtitles) throw new Error('Subtitle destination is unavailable');
      return onPopulateS2sSubtitles(...args);
    },
    onFinalize: onFinalizeNarrationSegments,
  });

  useEffect(() => {
    onNarrationGroupPreviewChange?.(narration.narrationGroupPreview);
    return () => onNarrationGroupPreviewChange?.(null);
  }, [narration.narrationGroupPreview, onNarrationGroupPreviewChange]);

  const generateLabel =
    (selectedSubtitleIds?.length ?? 0) > 0 || selectedSubtitleRange
      ? t.subtitleNarrationGenerateSelection
      : t.subtitleNarrationGenerate;
  const groupBudgetLabel = t.narrationGroupingBudgetValue.replace(
    '{count}',
    String(groupTextBudget),
  );
  const targetSegmentLabel = t.narrationTargetSegmentValue.replace(
    '{count}',
    String(targetSegmentSec),
  );

  const status = narration.narrationStatus;
  const selectedMethodSupported = isMethodSupportedForDetectedLanguage(settings.method);
  const statusMessage = (() => {
    if (!selectedMethodSupported && detectedLanguageLabel) {
      return t.narrationTtsSelectedUnsupported.replace('{language}', detectedLanguageLabel);
    }
    if (!status) return t.subtitleNarrationIdleHint;
    switch (status.state) {
      case 'queued':
        return t.subtitleNarrationStatusStarting;
      case 'running':
        return t.subtitleNarrationStatusRunning;
      case 'cancelled':
        return t.subtitleNarrationStatusCancelled;
      case 'completed':
        return status.message || t.subtitleNarrationStatusComplete;
      case 'error':
        return status.message || t.subtitleNarrationStatusFailed;
      default:
        return status.message ?? t.subtitleNarrationIdleHint;
    }
  })();

  return (
    <PanelCard className="narration-panel">
      <div className="narration-panel-body space-y-3">
        <NarrationModeSections
          generateLabel={generateLabel}
          groupBudgetLabel={groupBudgetLabel}
          groupTextBudget={groupTextBudget}
          targetSegmentLabel={targetSegmentLabel}
          targetSegmentSec={targetSegmentSec}
          hasSubtitleRange={Boolean(selectedSubtitleRange)}
          directVoiceMethod={directVoiceMethod}
          narration={narration}
          narrationMode={narrationMode}
          onSourceChange={onSourceChange}
          readUnsplitSubtitles={readUnsplitSubtitles}
          s2s={s2s}
          s2sLanguageOptions={s2sLanguageOptions}
          s2sSourceOptions={s2sSourceOptions}
          s2sTargetLanguage={s2sTargetLanguage}
          selectedMethodSupported={selectedMethodSupported}
          selectedSource={selectedSource}
          selectedSourceTrackId={selectedSourceTrackId}
          setGroupTextBudget={setGroupTextBudget}
          setTargetSegmentSec={setTargetSegmentSec}
          setIsGroupSliderDragging={setIsGroupSliderDragging}
          setDirectVoiceMethod={setDirectVoiceMethod}
          setNarrationMode={setNarrationMode}
          setReadUnsplitSubtitles={setReadUnsplitSubtitles}
          setS2sTargetLanguage={setS2sTargetLanguage}
          setSelectedSourceTrackId={setSelectedSourceTrackId}
          sourceTrackOptions={sourceTrackOptions}
          status={status}
          statusMessage={statusMessage}
          subtitlesAvailable={visibleSubtitles.length > 0}
        />

        {(!isDirectVoiceMode || directVoiceMethod === 's2s') && (
        <div className="narration-panel-tts rounded-xl border border-outline/30 bg-surface-container-high/40 p-2.5">
          <div className="narration-panel-tts-header mb-2 text-[11px] font-semibold text-on-surface">
            {t.narrationTtsTitle}
          </div>

          {!isDirectVoiceMode && (
          <div className="narration-panel-row mb-2 flex items-center gap-2">
            <LabelledField
              label={t.narrationTtsMethod}
              labelClassName="w-20 shrink-0 text-[11px] font-medium text-on-surface-variant"
            >
              <PanelSelect
                value={effectiveTtsMethod}
                options={providerOptions}
                onChange={(value) => {
                  if (!isDirectVoiceMode) update('method', value as NarrationTtsMethod);
                }}
                triggerClassName="narration-method-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-method-menu"
              />
            </LabelledField>
          </div>
          )}

          {effectiveTtsMethod === 'GeminiLive' && directVoiceMethod === 's2s' && (
            <NarrationGeminiSettings
              addLanguageCondition={addLanguageCondition}
              availableConditionLanguages={availableConditionLanguages}
              geminiLanguageConditions={geminiLanguageConditions}
              geminiModels={geminiModels}
              geminiSpeedOptions={geminiSpeedOptions}
              geminiVoices={geminiVoices}
              narrationMode={narrationMode}
              removeLanguageCondition={removeLanguageCondition}
              settings={settings}
              update={update}
              updateLanguageCondition={updateLanguageCondition}
            />
          )}

          {!isDirectVoiceMode && (
            <NarrationVoiceProviderSettings
              addEdgeVoiceConfig={addEdgeVoiceConfig}
              availableEdgeVoiceLanguages={availableEdgeVoiceLanguages}
              edgeVoiceConfigs={edgeVoiceConfigs}
              edgeVoiceState={metadata?.edgeVoiceState}
              edgeVoicesByLanguage={edgeVoicesByLanguage}
              effectiveTtsMethod={effectiveTtsMethod}
              googleSpeedOptions={googleSpeedOptions}
              removeEdgeVoiceConfig={removeEdgeVoiceConfig}
              settings={settings}
              update={update}
              updateEdgeVoiceConfig={updateEdgeVoiceConfig}
            />
          )}
        </div>
        )}

      </div>
    </PanelCard>
  );
}
