import { X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { PanelCard } from '@/components/layout/PanelCard';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { useSettings } from '@/hooks/useSettings';
import { useSubtitleNarration } from '@/hooks/useSubtitleNarration';
import {
  useNarrationSettings,
  type NarrationEdgeVoiceConfig,
  type NarrationLanguageCondition,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getSubtitleTrackLabel,
} from '@/lib/subtitleTracks';
import type { NarrationSegment, SubtitleSegment, SubtitleTrack, SubtitleViewState } from '@/types/video';

const CURRENT_SUBTITLE_VIEW_SOURCE_ID = 'current-subtitle-view';

interface NarrationPanelProps {
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
}

export function NarrationPanel({
  visibleSubtitles,
  subtitleTracks,
  activeSubtitleView,
  selectedSubtitleIds,
  selectedSubtitleRange,
  onApplyNarrationSegments,
  onFinalizeNarrationSegments,
}: NarrationPanelProps) {
  const { t } = useSettings();
  const { settings, update, profile, metadata } = useNarrationSettings();

  const availableTracks = subtitleTracks ?? [];
  const preferredSourceTrackId = activeSubtitleView?.kind === 'track'
    ? (activeSubtitleView.trackId ?? ORIGINAL_SUBTITLE_TRACK_ID)
    : CURRENT_SUBTITLE_VIEW_SOURCE_ID;
  const [selectedSourceTrackId, setSelectedSourceTrackId] = useState<string>(
    preferredSourceTrackId,
  );

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

  const geminiVoices = metadata?.geminiVoices ?? [];
  const geminiModels = metadata?.geminiModels ?? [];
  const geminiInstructionLanguages = metadata?.geminiInstructionLanguages ?? [];
  const geminiSpeedOptions = metadata?.geminiSpeedOptions ?? ['Slow', 'Normal', 'Fast'];
  const googleSpeedOptions = metadata?.googleSpeedOptions ?? ['Slow', 'Normal'];
  const edgeVoiceLanguages = metadata?.edgeVoiceLanguages ?? [];
  const edgeVoicesByLanguage = metadata?.edgeVoicesByLanguage ?? {};

  const usedConditionCodes = new Set(
    settings.geminiLanguageConditions.map((condition) => condition.languageCode.toLowerCase()),
  );
  const availableConditionLanguages = geminiInstructionLanguages.filter(
    (language) => !usedConditionCodes.has(language.languageCode.toLowerCase()),
  );

  const updateLanguageCondition = (
    index: number,
    next: Partial<NarrationLanguageCondition>,
  ) => {
    const updated = settings.geminiLanguageConditions.map((condition, i) =>
      i === index ? { ...condition, ...next } : condition,
    );
    update('geminiLanguageConditions', updated);
  };

  const removeLanguageCondition = (index: number) => {
    update(
      'geminiLanguageConditions',
      settings.geminiLanguageConditions.filter((_, i) => i !== index),
    );
  };

  const addLanguageCondition = (languageCode: string, languageName: string) => {
    update('geminiLanguageConditions', [
      ...settings.geminiLanguageConditions,
      { languageCode, languageName, instruction: '' },
    ]);
  };

  const setEdgeVoiceConfigs = (configs: NarrationEdgeVoiceConfig[]) => {
    update('edgeVoiceConfigs', configs);
    update('edgeVoice', configs[0]?.voiceName ?? settings.edgeVoice);
  };

  const updateEdgeVoiceConfig = (
    index: number,
    next: Partial<NarrationEdgeVoiceConfig>,
  ) => {
    setEdgeVoiceConfigs(
      settings.edgeVoiceConfigs.map((config, i) =>
        i === index ? { ...config, ...next } : config,
      ),
    );
  };

  const removeEdgeVoiceConfig = (index: number) => {
    setEdgeVoiceConfigs(settings.edgeVoiceConfigs.filter((_, i) => i !== index));
  };

  const addEdgeVoiceConfig = (languageCode: string, languageName: string) => {
    const voices = edgeVoicesByLanguage[languageCode] ?? [];
    const voiceName = voices[0]?.shortName ?? `${languageCode}-??-??Neural`;
    setEdgeVoiceConfigs([
      ...settings.edgeVoiceConfigs,
      { languageCode, languageName, voiceName },
    ]);
  };

  const usedEdgeVoiceCodes = new Set(
    settings.edgeVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableEdgeVoiceLanguages = edgeVoiceLanguages.filter(
    (language) => !usedEdgeVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const narration = useSubtitleNarration({
    t,
    visibleSubtitles: subtitlesFromSelectedTrack,
    selectedSubtitleIds,
    selectedSubtitleRange,
    profile,
    onApplyNarrationSegments,
    onFinalizeNarrationSegments,
  });

  const generateLabel =
    (selectedSubtitleIds?.length ?? 0) > 0 || selectedSubtitleRange
      ? t.subtitleNarrationGenerateSelection
      : t.subtitleNarrationGenerate;

  const status = narration.narrationStatus;
  const statusMessage = (() => {
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
        <div className="narration-panel-source-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.narrationSourceTrack}
          </span>
          <PanelSelect
            value={selectedSourceTrackId}
            options={sourceTrackOptions}
            onChange={(value) => setSelectedSourceTrackId(value)}
            triggerClassName="narration-source-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
            contentClassName="narration-source-menu"
          />
        </div>

        <div className="narration-panel-generate rounded-xl border border-outline/35 bg-surface-container-high/45 p-2.5">
          <div className="narration-panel-generate-header mb-2 flex items-center justify-between gap-2">
            <span className="text-[11px] font-semibold text-on-surface">
              {t.subtitleNarrationTitle}
            </span>
            <span className="text-[10px] font-medium text-on-surface-variant">
              {t.subtitleNarrationProgress
                .replace('{done}', String(status?.completedItems ?? 0))
                .replace('{total}', String(narration.narrationTargetCount))}
            </span>
          </div>
          <p className="narration-panel-hint mb-2 text-[10px] leading-4 text-on-surface-variant">
            {t.subtitleNarrationHint}
          </p>
          <div className="narration-panel-actions grid grid-cols-2 gap-1.5">
            <button
              type="button"
              disabled={!narration.canGenerateNarration}
              onClick={narration.handleGenerateNarration}
              data-tone="primary"
              className="narration-panel-generate-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
            >
              {generateLabel}
            </button>
            <button
              type="button"
              disabled={!narration.isGeneratingNarration}
              onClick={narration.handleCancelNarration}
              data-tone="danger"
              className="narration-panel-cancel-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
            >
              {t.subtitleNarrationCancel}
            </button>
          </div>
          <div className="narration-panel-status mt-2 flex items-center justify-between gap-2 text-[10px] leading-4 text-on-surface-variant">
            <span className="narration-panel-status-message min-w-0 truncate">{statusMessage}</span>
            {(status?.errors.length ?? 0) > 0 && (
              <span className="narration-panel-error-count flex-shrink-0 text-[var(--tertiary-color)]">
                {t.subtitleNarrationErrors.replace('{count}', String(status?.errors.length ?? 0))}
              </span>
            )}
          </div>
          {(status?.state === 'error' || (status?.errors.length ?? 0) > 0) && (
            <details className="narration-panel-error-details mt-2 text-[10px] leading-4">
              <summary className="cursor-pointer text-[var(--tertiary-color)]">
                {t.narrationErrorDetails}
              </summary>
              <div className="narration-panel-error-detail-body mt-1 max-h-32 overflow-y-auto rounded border border-outline/30 bg-[var(--ui-surface-2)] p-1.5 font-mono text-[10px] text-on-surface-variant">
                {status?.error && (
                  <div className="narration-panel-error-top break-words text-[var(--tertiary-color)]">
                    {status.error}
                  </div>
                )}
                {(status?.errors ?? []).map((entry, idx) => (
                  <div key={idx} className="narration-panel-error-line break-words">
                    <span className="text-[var(--secondary-color)]">{entry.subtitleId}</span>
                    : {entry.message}
                  </div>
                ))}
                <div className="narration-panel-error-hint mt-1 text-[9px] italic text-on-surface-variant/70">
                  {t.narrationErrorLogHint}
                </div>
              </div>
            </details>
          )}
        </div>

        <div className="narration-panel-tts rounded-xl border border-outline/30 bg-surface-container-high/40 p-2.5">
          <div className="narration-panel-tts-header mb-2 text-[11px] font-semibold text-on-surface">
            {t.narrationTtsTitle}
          </div>

          <div className="narration-panel-row mb-2 flex items-center gap-2">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
              {t.narrationTtsMethod}
            </span>
            <PanelSelect
              value={settings.method}
              options={[
                { value: 'GeminiLive', label: t.narrationTtsMethodGemini },
                { value: 'EdgeTTS', label: t.narrationTtsMethodEdge },
                { value: 'GoogleTranslate', label: t.narrationTtsMethodGoogle },
              ]}
              onChange={(value) => update('method', value as NarrationTtsMethod)}
              triggerClassName="narration-method-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-method-menu"
            />
          </div>

          {settings.method === 'GeminiLive' && (
            <>
              {geminiModels.length > 0 && (
                <div className="narration-panel-row mb-2 flex items-center gap-2">
                  <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
                    {t.narrationTtsModel}
                  </span>
                  <PanelSelect
                    value={settings.geminiModel}
                    options={geminiModels.map((model) => ({
                      value: model.apiModel,
                      label: model.label,
                    }))}
                    onChange={(value) => update('geminiModel', value)}
                    triggerClassName="narration-model-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                    contentClassName="narration-model-menu"
                  />
                </div>
              )}
              <div className="narration-panel-row mb-2 flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsVoice}
                </span>
                <PanelSelect
                  value={settings.geminiVoice}
                  options={geminiVoices.map((voice) => ({
                    value: voice.name,
                    label: `${voice.name} · ${voice.gender}`,
                  }))}
                  onChange={(value) => update('geminiVoice', value)}
                  triggerClassName="narration-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                  contentClassName="narration-voice-menu"
                  searchable
                />
              </div>
              <div className="narration-panel-row mb-2 flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsSpeed}
                </span>
                <PanelSelect
                  value={settings.geminiSpeed}
                  options={geminiSpeedOptions.map((speed) => ({
                    value: speed,
                    label: speed,
                  }))}
                  onChange={(value) => update('geminiSpeed', value)}
                  triggerClassName="narration-speed-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                  contentClassName="narration-speed-menu"
                />
              </div>
              <div className="narration-panel-row mb-2 flex flex-col gap-1">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsInstruction}
                </span>
                <textarea
                  value={settings.geminiInstruction}
                  onChange={(event) => update('geminiInstruction', event.target.value)}
                  rows={2}
                  className="narration-panel-instruction ui-input w-full resize-y rounded-lg px-2 py-1 text-[11px]"
                />
              </div>
              <div className="narration-panel-conditions mb-1 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsLanguageConditions}
                </span>
                {settings.geminiLanguageConditions.map((condition, index) => (
                  <div key={`${condition.languageCode}-${index}`} className="narration-panel-condition flex items-center gap-1.5">
                    <span className="w-20 flex-shrink-0 text-[11px] font-medium text-[var(--secondary-color)]">
                      {condition.languageName}
                    </span>
                    <input
                      value={condition.instruction}
                      onChange={(event) =>
                        updateLanguageCondition(index, { instruction: event.target.value })
                      }
                      placeholder={t.narrationTtsLanguageConditionHint}
                      className="narration-panel-condition-input ui-input flex-1 rounded-lg px-2 py-1 text-[11px]"
                    />
                    <button
                      type="button"
                      onClick={() => removeLanguageCondition(index)}
                      className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                      title={t.narrationTtsLanguageConditionRemove}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </div>
                ))}
                {availableConditionLanguages.length > 0 && (
                  <PanelSelect
                    value={t.narrationTtsLanguageConditionAdd}
                    options={availableConditionLanguages.map((lang) => ({
                      value: lang.languageCode,
                      label: lang.languageName,
                    }))}
                    onChange={(value) => {
                      if (!value) return;
                      const lang = availableConditionLanguages.find((l) => l.languageCode === value);
                      if (lang) addLanguageCondition(lang.languageCode, lang.languageName);
                    }}
                    triggerClassName="narration-condition-add h-8 self-start rounded-lg px-2.5 text-[11px]"
                    contentClassName="narration-condition-add-menu"
                    searchable
                  />
                )}
              </div>
            </>
          )}

          {settings.method === 'GoogleTranslate' && (
            <div className="narration-panel-row flex items-center gap-2">
              <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
                {t.narrationTtsSpeed}
              </span>
              <PanelSelect
                value={settings.googleSpeed}
                options={googleSpeedOptions.map((speed) => ({ value: speed, label: speed }))}
                onChange={(value) => update('googleSpeed', value)}
                triggerClassName="narration-google-speed h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-google-speed-menu"
              />
            </div>
          )}

          {settings.method === 'EdgeTTS' && (
            <>
              <div className="narration-panel-row mb-2 flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  {t.narrationTtsPitch}
                </span>
                <input
                  type="range"
                  min={-50}
                  max={50}
                  step={1}
                  value={settings.edgePitch}
                  onChange={(event) => update('edgePitch', parseInt(event.target.value, 10))}
                  className="narration-panel-pitch-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.edgePitch}
                </span>
              </div>
              <div className="narration-panel-row flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  {t.narrationTtsRate}
                </span>
                <input
                  type="range"
                  min={-50}
                  max={100}
                  step={1}
                  value={settings.edgeRate}
                  onChange={(event) => update('edgeRate', parseInt(event.target.value, 10))}
                  className="narration-panel-rate-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.edgeRate}
                </span>
              </div>
              <div className="narration-panel-edge-voices mt-2 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsEdgeVoiceConfigs}
                </span>
                {metadata?.edgeVoiceState === 'loading' && (
                  <span className="narration-panel-edge-loading text-[10px] text-on-surface-variant">
                    {t.narrationTtsEdgeVoicesLoading}
                  </span>
                )}
                {metadata?.edgeVoiceState === 'error' && (
                  <span className="narration-panel-edge-error text-[10px] text-[var(--tertiary-color)]">
                    {t.narrationTtsEdgeVoicesFailed}
                  </span>
                )}
                {settings.edgeVoiceConfigs.map((config, index) => {
                  const voiceOptions = edgeVoicesByLanguage[config.languageCode] ?? [];
                  const options = voiceOptions.length > 0
                    ? voiceOptions.map((voice) => ({
                        value: voice.shortName,
                        label: `${voice.shortName} · ${voice.gender}`,
                      }))
                    : [{ value: config.voiceName, label: config.voiceName }];
                  return (
                    <div
                      key={`${config.languageCode}-${index}`}
                      className="narration-panel-edge-voice-config flex items-center gap-1.5"
                    >
                      <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                        {config.languageName}
                      </span>
                      <PanelSelect
                        value={config.voiceName}
                        options={options}
                        onChange={(value) => updateEdgeVoiceConfig(index, { voiceName: value })}
                        triggerClassName="narration-edge-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                        contentClassName="narration-edge-voice-menu"
                        searchable={voiceOptions.length > 8}
                      />
                      <button
                        type="button"
                        onClick={() => removeEdgeVoiceConfig(index)}
                        className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                        title={t.narrationTtsLanguageConditionRemove}
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </div>
                  );
                })}
                {availableEdgeVoiceLanguages.length > 0 && (
                  <PanelSelect
                    value={t.narrationTtsEdgeVoiceConfigAdd}
                    options={availableEdgeVoiceLanguages.map((language) => ({
                      value: language.languageCode,
                      label: language.languageName,
                    }))}
                    onChange={(value) => {
                      const language = availableEdgeVoiceLanguages.find(
                        (item) => item.languageCode === value,
                      );
                      if (language) addEdgeVoiceConfig(language.languageCode, language.languageName);
                    }}
                    triggerClassName="narration-edge-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
                    contentClassName="narration-edge-voice-add-menu"
                    searchable
                  />
                )}
              </div>
            </>
          )}
        </div>

      </div>
    </PanelCard>
  );
}
