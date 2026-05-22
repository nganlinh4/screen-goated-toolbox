import { AlertTriangle, RotateCcw, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { PanelCard } from '@/components/layout/PanelCard';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import { useSettings } from '@/hooks/useSettings';
import { invoke } from '@/lib/ipc';
import {
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  MAX_NARRATION_GROUP_TEXT_BUDGET,
  MIN_NARRATION_GROUP_TEXT_BUDGET,
  useSubtitleNarration,
  type SubtitleNarrationGroupPreview,
} from '@/hooks/useSubtitleNarration';
import {
  populateEmptyS2sSubtitleTracks,
  useS2sNarration,
} from '@/hooks/useS2sNarration';
import {
  useNarrationSettings,
  type NarrationEdgeVoiceConfig,
  type NarrationKokoroVoiceConfig,
  type NarrationLanguageCondition,
  type NarrationMagpieVoiceConfig,
  type NarrationSupertonicVoiceConfig,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { SUBTITLE_LANGUAGE_OPTIONS_GROQ } from '@/lib/subtitleLanguageOptions';
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

const CURRENT_SUBTITLE_VIEW_SOURCE_ID = 'current-subtitle-view';
const READ_UNSPLIT_SUBTITLES_KEY = 'screen-record-narration-read-unsplit-subtitles-v1';
const NARRATION_GROUP_TEXT_BUDGET_KEY = 'screen-record-narration-group-text-budget-v2';

const LANGUAGE_6393_TO_PRIMARY: Record<string, string> = {
  arb: 'ar',
  cmn: 'zh',
  deu: 'de',
  eng: 'en',
  fra: 'fr',
  hin: 'hi',
  ita: 'it',
  jpn: 'ja',
  kor: 'ko',
  por: 'pt',
  spa: 'es',
  vie: 'vi',
  yue: 'zh',
  zho: 'zh',
};

const LANGUAGE_PRIMARY_TO_6393: Record<string, string> = {
  ar: 'arb',
  de: 'deu',
  en: 'eng',
  es: 'spa',
  fr: 'fra',
  hi: 'hin',
  it: 'ita',
  ja: 'jpn',
  ko: 'kor',
  pt: 'por',
  vi: 'vie',
  zh: 'cmn',
};

interface NarrationLanguageDetectionResponse {
  languageCode?: string | null;
  sample?: string;
}

function normalizeLanguagePrimary(code: string | null | undefined) {
  const normalized = code?.trim().toLowerCase();
  if (!normalized || normalized === 'auto') return null;
  const base = normalized.split(/[-_]/)[0];
  return LANGUAGE_6393_TO_PRIMARY[base] ?? base;
}

function normalizeLanguage6393(code: string | null | undefined) {
  const primary = normalizeLanguagePrimary(code);
  if (!primary) return null;
  if (primary.length === 3) return primary;
  return LANGUAGE_PRIMARY_TO_6393[primary] ?? primary;
}

function languageMatches(candidate: string | null | undefined, detectedCode: string) {
  const candidatePrimary = normalizeLanguagePrimary(candidate);
  const candidate6393 = normalizeLanguage6393(candidate);
  const detectedPrimary = normalizeLanguagePrimary(detectedCode);
  const detected6393 = normalizeLanguage6393(detectedCode);
  return !!candidatePrimary && (
    candidatePrimary === detectedPrimary
    || candidate6393 === detected6393
  );
}

function getInitialNarrationGroupTextBudget() {
  try {
    const raw = Number(localStorage.getItem(NARRATION_GROUP_TEXT_BUDGET_KEY));
    if (
      Number.isFinite(raw) &&
      raw >= MIN_NARRATION_GROUP_TEXT_BUDGET &&
      raw <= MAX_NARRATION_GROUP_TEXT_BUDGET
    ) {
      return Math.round(raw);
    }
  } catch {
    // ignore persistence failures
  }
  return DEFAULT_NARRATION_GROUP_TEXT_BUDGET;
}

function getInitialReadUnsplitSubtitles() {
  try {
    const raw = localStorage.getItem(READ_UNSPLIT_SUBTITLES_KEY);
    return raw === null ? true : raw === 'true';
  } catch {
    return true;
  }
}

function kokoroVoiceLanguageForCondition(languageCode: string) {
  switch (languageCode.toLowerCase()) {
    case 'eng':
      return 'en-us';
    case 'cmn':
    case 'zho':
      return 'zh';
    case 'jpn':
      return 'ja';
    case 'spa':
      return 'es';
    case 'fra':
      return 'fr';
    case 'hin':
      return 'hi';
    case 'ita':
      return 'it';
    case 'por':
      return 'pt-br';
    default:
      return '';
  }
}

interface NarrationPanelProps {
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
  onUpdateSegment: (segment: VideoSegment) => void;
}

export function NarrationPanel({
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
  onUpdateSegment,
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
  const [readUnsplitSubtitles, setReadUnsplitSubtitles] = useState(getInitialReadUnsplitSubtitles);
  const [groupTextBudget, setGroupTextBudget] = useState(getInitialNarrationGroupTextBudget);
  const [isGroupSliderDragging, setIsGroupSliderDragging] = useState(false);
  const [narrationMode, setNarrationMode] = useState<'subtitles' | 's2s'>(
    visibleSubtitles.length > 0 ? 'subtitles' : 's2s',
  );
  const [s2sTargetLanguage, setS2sTargetLanguage] = useState('vi');
  const effectiveTtsMethod: NarrationTtsMethod = narrationMode === 's2s'
    ? 'GeminiLive'
    : settings.method;

  useEffect(() => {
    try {
      localStorage.setItem(READ_UNSPLIT_SUBTITLES_KEY, String(readUnsplitSubtitles));
    } catch {
      // ignore persistence failures
    }
  }, [readUnsplitSubtitles]);

  useEffect(() => {
    try {
      localStorage.setItem(NARRATION_GROUP_TEXT_BUDGET_KEY, String(groupTextBudget));
    } catch {
      // ignore persistence failures
    }
  }, [groupTextBudget]);

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

  const geminiVoices = metadata?.geminiVoices ?? [];
  const geminiModels = metadata?.geminiModels ?? [];
  const geminiInstructionLanguages = metadata?.geminiInstructionLanguages ?? [];
  const geminiSpeedOptions = metadata?.geminiSpeedOptions ?? ['Slow', 'Normal', 'Fast'];
  const googleSpeedOptions = metadata?.googleSpeedOptions ?? ['Slow', 'Normal'];
  const kokoroVoices = metadata?.kokoroVoices ?? [];
  const kokoroVoiceLanguages = metadata?.kokoroVoiceLanguages ?? [];
  const magpieVoices = metadata?.magpieVoices ?? [];
  const magpieVoiceLanguages = metadata?.magpieVoiceLanguages ?? [];
  const supertonicLanguages = metadata?.supertonicLanguages ?? [];
  const supertonicVoices = metadata?.supertonicVoices ?? [];
  const stepAudioVoices = metadata?.stepAudioVoices ?? [];
  const stepAudioVoiceLanguages = metadata?.stepAudioVoiceLanguages ?? [];
  const referenceVoices = metadata?.stepAudioReferenceVoices ?? stepAudioVoices;
  const edgeVoiceLanguages = metadata?.edgeVoiceLanguages ?? [];
  const edgeVoicesByLanguage = metadata?.edgeVoicesByLanguage ?? {};
  const geminiLanguageConditions = settings.geminiLanguageConditions ?? [];
  const edgeVoiceConfigs = settings.edgeVoiceConfigs ?? [];
  const kokoroVoiceConfigs = settings.kokoroVoiceConfigs ?? [];
  const magpieVoiceConfigs = settings.magpieVoiceConfigs ?? [];
  const supertonicVoiceConfigs = settings.supertonicVoiceConfigs ?? [];
  const methodLabel = (method: NarrationTtsMethod, fallback: string) => {
    switch (method) {
      case 'GeminiLive':
        return t.narrationTtsMethodGemini;
      case 'EdgeTTS':
        return t.narrationTtsMethodEdge;
      case 'GoogleTranslate':
        return t.narrationTtsMethodGoogle;
      case 'Kokoro':
        return t.narrationTtsMethodKokoro;
      case 'Supertonic':
        return 'Supertonic 3';
      case 'VieneuTts':
        return 'VieNeu-TTS v2';
      case 'StepAudioEditX':
        return 'Step Audio EditX';
      case 'MagpieMultilingual':
        return 'NVIDIA Magpie-Multilingual 357M';
      default:
        return fallback;
    }
  };
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

  const isMethodSupportedForDetectedLanguage = (method: NarrationTtsMethod) => {
    const detectedCode = detectedNarrationLanguageCode;
    if (!detectedCode) return true;
    switch (method) {
      case 'Kokoro':
        return kokoroVoiceLanguages.length === 0
          || kokoroVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'Supertonic':
        return supertonicLanguages.length === 0
          || supertonicLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'MagpieMultilingual':
        return magpieVoiceLanguages.length === 0
          || magpieVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'VieneuTts':
        return languageMatches('vie', detectedCode);
      case 'StepAudioEditX':
        return stepAudioVoiceLanguages.length === 0
          || stepAudioVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'EdgeTTS':
        return edgeVoiceLanguages.length === 0
          || edgeVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'GeminiLive':
        return geminiInstructionLanguages.length === 0
          || geminiInstructionLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'GoogleTranslate':
      default:
        return true;
    }
  };

  const detectedLanguageLabel = useMemo(() => {
    const detectedCode = detectedNarrationLanguageCode;
    if (!detectedCode) return null;
    const allLanguages = [
      ...geminiInstructionLanguages,
      ...edgeVoiceLanguages,
      ...kokoroVoiceLanguages,
      ...magpieVoiceLanguages,
      ...supertonicLanguages,
      ...stepAudioVoiceLanguages,
    ];
    const match = allLanguages.find((language) =>
      languageMatches(language.languageCode, detectedCode),
    );
    return match?.languageName ?? detectedCode;
  }, [
    detectedNarrationLanguageCode,
    edgeVoiceLanguages,
    geminiInstructionLanguages,
    kokoroVoiceLanguages,
    magpieVoiceLanguages,
    stepAudioVoiceLanguages,
    supertonicLanguages,
  ]);

  const unsupportedLanguageTitle = detectedLanguageLabel
    ? t.narrationTtsUnsupportedForLanguage.replace('{language}', detectedLanguageLabel)
    : t.narrationTtsUnsupported;
  const providerOptions = (metadata?.providers?.length
    ? metadata.providers
    : [
        { method: 'GeminiLive' as const, label: 'Gemini Live' },
        { method: 'EdgeTTS' as const, label: 'Edge TTS' },
        { method: 'GoogleTranslate' as const, label: 'Google Translate' },
        { method: 'Kokoro' as const, label: 'Kokoro 82M v1.0' },
        { method: 'Supertonic' as const, label: 'Supertonic 3' },
        { method: 'VieneuTts' as const, label: 'VieNeu-TTS v2' },
        { method: 'StepAudioEditX' as const, label: 'Step Audio EditX' },
        { method: 'MagpieMultilingual' as const, label: 'NVIDIA Magpie-Multilingual 357M' },
      ]).map((provider) => {
        const isSupported = isMethodSupportedForDetectedLanguage(provider.method);
        return {
          value: provider.method,
          label: methodLabel(provider.method, provider.label),
          disabled: !isSupported,
          trailing: !isSupported ? (
            <span className="narration-method-language-warning-wrapper" title={unsupportedLanguageTitle}>
              <AlertTriangle
                className="narration-method-language-warning h-3.5 w-3.5 text-[var(--tertiary-color)]"
                aria-label={unsupportedLanguageTitle}
              />
            </span>
          ) : undefined,
        };
      });

  const usedConditionCodes = new Set(
    geminiLanguageConditions.map((condition) => condition.languageCode.toLowerCase()),
  );
  const availableConditionLanguages = geminiInstructionLanguages.filter(
    (language) => !usedConditionCodes.has(language.languageCode.toLowerCase()),
  );

  const updateLanguageCondition = (
    index: number,
    next: Partial<NarrationLanguageCondition>,
  ) => {
    const updated = geminiLanguageConditions.map((condition, i) =>
      i === index ? { ...condition, ...next } : condition,
    );
    update('geminiLanguageConditions', updated);
  };

  const removeLanguageCondition = (index: number) => {
    update(
      'geminiLanguageConditions',
      geminiLanguageConditions.filter((_, i) => i !== index),
    );
  };

  const addLanguageCondition = (languageCode: string, languageName: string) => {
    update('geminiLanguageConditions', [
      ...geminiLanguageConditions,
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
      edgeVoiceConfigs.map((config, i) =>
        i === index ? { ...config, ...next } : config,
      ),
    );
  };

  const removeEdgeVoiceConfig = (index: number) => {
    setEdgeVoiceConfigs(edgeVoiceConfigs.filter((_, i) => i !== index));
  };

  const addEdgeVoiceConfig = (languageCode: string, languageName: string) => {
    const voices = edgeVoicesByLanguage[languageCode] ?? [];
    const voiceName = voices[0]?.shortName ?? `${languageCode}-??-??Neural`;
    setEdgeVoiceConfigs([
      ...edgeVoiceConfigs,
      { languageCode, languageName, voiceName },
    ]);
  };

  const usedEdgeVoiceCodes = new Set(
    edgeVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableEdgeVoiceLanguages = edgeVoiceLanguages.filter(
    (language) => !usedEdgeVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const setKokoroVoiceConfigs = (configs: NarrationKokoroVoiceConfig[]) => {
    update('kokoroVoiceConfigs', configs);
    update('kokoroVoice', configs[0]?.voiceId ?? settings.kokoroVoice);
  };
  const updateKokoroVoiceConfig = (
    index: number,
    next: Partial<NarrationKokoroVoiceConfig>,
  ) => {
    setKokoroVoiceConfigs(
      kokoroVoiceConfigs.map((config, i) =>
        i === index ? { ...config, ...next } : config,
      ),
    );
  };
  const removeKokoroVoiceConfig = (index: number) => {
    setKokoroVoiceConfigs(kokoroVoiceConfigs.filter((_, i) => i !== index));
  };
  const addKokoroVoiceConfig = (languageCode: string, languageName: string) => {
    const normalized = kokoroVoiceLanguageForCondition(languageCode);
    const voiceId = kokoroVoices.find((voice) => voice.languageCode === normalized)?.id
      ?? kokoroVoices[0]?.id
      ?? 'af_heart';
    setKokoroVoiceConfigs([
      ...kokoroVoiceConfigs,
      { languageCode, languageName, voiceId },
    ]);
  };
  const usedKokoroVoiceCodes = new Set(
    kokoroVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableKokoroVoiceLanguages = kokoroVoiceLanguages.filter(
    (language) => !usedKokoroVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const setMagpieVoiceConfigs = (configs: NarrationMagpieVoiceConfig[]) => {
    update('magpieVoiceConfigs', configs);
  };
  const updateMagpieVoiceConfig = (
    index: number,
    next: Partial<NarrationMagpieVoiceConfig>,
  ) => {
    setMagpieVoiceConfigs(
      magpieVoiceConfigs.map((config, i) =>
        i === index ? { ...config, ...next } : config,
      ),
    );
  };
  const removeMagpieVoiceConfig = (index: number) => {
    setMagpieVoiceConfigs(magpieVoiceConfigs.filter((_, i) => i !== index));
  };
  const addMagpieVoiceConfig = (languageCode: string, languageName: string) => {
    setMagpieVoiceConfigs([
      ...magpieVoiceConfigs,
      { languageCode, languageName, voiceId: magpieVoices[0]?.id ?? 'Sofia' },
    ]);
  };
  const usedMagpieVoiceCodes = new Set(
    magpieVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableMagpieVoiceLanguages = magpieVoiceLanguages.filter(
    (language) => !usedMagpieVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const setSupertonicVoiceConfigs = (configs: NarrationSupertonicVoiceConfig[]) => {
    update('supertonicVoiceConfigs', configs);
  };
  const updateSupertonicVoiceConfig = (
    index: number,
    next: Partial<NarrationSupertonicVoiceConfig>,
  ) => {
    setSupertonicVoiceConfigs(
      supertonicVoiceConfigs.map((config, i) =>
        i === index ? { ...config, ...next } : config,
      ),
    );
  };
  const removeSupertonicVoiceConfig = (index: number) => {
    setSupertonicVoiceConfigs(supertonicVoiceConfigs.filter((_, i) => i !== index));
  };
  const addSupertonicVoiceConfig = (languageCode: string, languageName: string) => {
    setSupertonicVoiceConfigs([
      ...supertonicVoiceConfigs,
      { languageCode, languageName, voiceId: supertonicVoices[0]?.id ?? 'M1' },
    ]);
  };
  const usedSupertonicVoiceCodes = new Set(
    supertonicVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableSupertonicLanguages = supertonicLanguages.filter(
    (language) => !usedSupertonicVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const narration = useSubtitleNarration({
    t,
    visibleSubtitles: subtitlesFromSelectedTrack,
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
  const s2sLanguageOptions = SUBTITLE_LANGUAGE_OPTIONS_GROQ
    .filter((option) => option.value !== 'auto')
    .map((option) => ({ value: option.value, label: option.label }));
  const s2s = useS2sNarration({
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
    onApplyNarrationSegments,
    onPopulateEmptySubtitles: (sourceSegments, targetSegments, targetLanguage) => {
      if (!segment) return;
      onUpdateSegment(populateEmptyS2sSubtitleTracks(
        segment,
        sourceSegments,
        targetSegments,
        targetLanguage,
      ));
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
        <div className="narration-mode-row grid grid-cols-2 gap-1 rounded-lg bg-surface-container-high/50 p-1">
          <button
            type="button"
            onClick={() => setNarrationMode('subtitles')}
            disabled={visibleSubtitles.length === 0}
            className="narration-mode-subtitles ui-chip-button h-7 rounded-md text-[11px] font-medium disabled:opacity-45"
            data-active={narrationMode === 'subtitles'}
          >
            {t.narrationModeSubtitles}
          </button>
          <button
            type="button"
            onClick={() => setNarrationMode('s2s')}
            className="narration-mode-s2s ui-chip-button h-7 rounded-md text-[11px] font-medium"
            data-active={narrationMode === 's2s'}
          >
            {t.narrationModeS2s}
          </button>
        </div>

        {narrationMode === 's2s' && (
          <div className="narration-panel-s2s rounded-xl border border-outline/35 bg-surface-container-high/45 p-2.5">
            <div className="narration-s2s-title mb-2 text-[11px] font-semibold text-on-surface">
              {t.narrationModeS2s}
            </div>
            <div className="narration-s2s-source-row mb-2 flex items-center gap-2">
              <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
                {t.subtitleSource}
              </span>
              <PanelSelect
                value={selectedSource}
                options={s2sSourceOptions}
                onChange={(value) => onSourceChange(value as SubtitleSource)}
                triggerClassName="narration-s2s-source-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-s2s-source-menu"
              />
            </div>
            <div className="narration-s2s-language-row mb-2 flex items-center gap-2">
              <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
                {t.narrationS2sTarget}
              </span>
              <PanelSelect
                value={s2sTargetLanguage}
                options={s2sLanguageOptions}
                onChange={setS2sTargetLanguage}
                triggerClassName="narration-s2s-language-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-s2s-language-menu"
              />
            </div>
            <div className="narration-s2s-grouping mb-2 space-y-1.5">
              <div className="narration-s2s-grouping-header flex items-center justify-between gap-2">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationGrouping}
                </span>
                <span className="narration-s2s-grouping-value text-[10px] font-semibold text-on-surface-variant">
                  {groupTextBudget}
                </span>
              </div>
              <Slider
                min={MIN_NARRATION_GROUP_TEXT_BUDGET}
                max={MAX_NARRATION_GROUP_TEXT_BUDGET}
                step={5}
                value={groupTextBudget}
                onChange={(value) => setGroupTextBudget(
                  Math.max(
                    MIN_NARRATION_GROUP_TEXT_BUDGET,
                    Math.min(MAX_NARRATION_GROUP_TEXT_BUDGET, Math.round(value)),
                  ),
                )}
                className="narration-s2s-grouping-slider"
                disabled={s2s.isGenerating}
              />
            </div>
            <div className="narration-s2s-actions grid grid-cols-2 gap-1.5">
              <button
                type="button"
                disabled={!s2s.canGenerate}
                onClick={s2s.handleGenerate}
                data-tone="primary"
                data-emphasis="strong"
                className="narration-s2s-generate-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium"
              >
                {selectedSubtitleRange ? t.subtitleGenerateForRange : t.subtitleNarrationGenerate}
              </button>
              <button
                type="button"
                disabled={!s2s.isGenerating}
                onClick={s2s.handleCancel}
                data-tone="danger"
                className="narration-s2s-cancel-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium"
              >
                {t.subtitleNarrationCancel}
              </button>
            </div>
            <div className="narration-s2s-status mt-2 truncate text-[10px] leading-4 text-on-surface-variant">
              {s2s.status?.message ?? t.subtitleNarrationIdleHint}
            </div>
          </div>
        )}

        {narrationMode === 'subtitles' && (
          <>
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
          <label className="narration-panel-read-unsplit mb-2 flex cursor-pointer items-center gap-2 text-[11px] font-medium text-on-surface">
            <Checkbox
              checked={readUnsplitSubtitles}
              onChange={(event) => setReadUnsplitSubtitles(event.target.checked)}
            />
            {t.narrationReadUnsplitSubtitles}
          </label>
          <div className="narration-panel-grouping mb-2 space-y-1.5">
            <div className="narration-panel-grouping-header flex items-center justify-between gap-2">
              <span className="text-[11px] font-medium text-on-surface-variant">
                {t.narrationGrouping}
              </span>
              <div className="narration-panel-grouping-meta flex items-center gap-1.5">
                <span className="narration-panel-grouping-value text-[10px] font-semibold text-on-surface-variant">
                  {groupTextBudget} · {narration.narrationGroupCount} {t.narrationGroupingGroups}
                </span>
                {groupTextBudget !== DEFAULT_NARRATION_GROUP_TEXT_BUDGET ? (
                  <button
                    type="button"
                    onClick={() => setGroupTextBudget(DEFAULT_NARRATION_GROUP_TEXT_BUDGET)}
                    className="narration-panel-grouping-reset ui-chip-button flex h-6 w-6 items-center justify-center rounded-md"
                    title={t.subtitleGeminiPromptReset}
                    aria-label={t.subtitleGeminiPromptReset}
                  >
                    <RotateCcw className="h-3 w-3" />
                  </button>
                ) : null}
              </div>
            </div>
            <Slider
              min={MIN_NARRATION_GROUP_TEXT_BUDGET}
              max={MAX_NARRATION_GROUP_TEXT_BUDGET}
              step={5}
              value={groupTextBudget}
              onChange={(value) => setGroupTextBudget(
                Math.max(
                  MIN_NARRATION_GROUP_TEXT_BUDGET,
                  Math.min(MAX_NARRATION_GROUP_TEXT_BUDGET, Math.round(value)),
                ),
              )}
              onPointerDown={() => setIsGroupSliderDragging(true)}
              onPointerUp={() => setIsGroupSliderDragging(false)}
              onPointerCancel={() => setIsGroupSliderDragging(false)}
              onBlur={() => setIsGroupSliderDragging(false)}
              className="narration-panel-grouping-slider"
              disabled={narration.narrationTargetCount <= 1}
            />
          </div>
          <div className="narration-panel-actions grid grid-cols-2 gap-1.5">
            <button
              type="button"
              disabled={!narration.canGenerateNarration || !selectedMethodSupported}
              onClick={narration.handleGenerateNarration}
              data-tone="primary"
              data-emphasis="strong"
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

          </>
        )}

        <div className="narration-panel-tts rounded-xl border border-outline/30 bg-surface-container-high/40 p-2.5">
          <div className="narration-panel-tts-header mb-2 text-[11px] font-semibold text-on-surface">
            {t.narrationTtsTitle}
          </div>

          <div className="narration-panel-row mb-2 flex items-center gap-2">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
              {t.narrationTtsMethod}
            </span>
            <PanelSelect
              value={effectiveTtsMethod}
              options={narrationMode === 's2s'
                ? [{ value: 'GeminiLive', label: t.narrationTtsMethodGeminiS2s }]
                : providerOptions}
              onChange={(value) => {
                if (narrationMode !== 's2s') update('method', value as NarrationTtsMethod);
              }}
              triggerClassName="narration-method-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-method-menu"
            />
          </div>

          {effectiveTtsMethod === 'GeminiLive' && (
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
              <div className="narration-panel-parallel mb-2 space-y-1.5">
                <div className="narration-panel-parallel-header flex items-center justify-between gap-2">
                  <span className="text-[11px] font-medium text-on-surface-variant">
                    {t.narrationTtsParallelRequests}
                  </span>
                  <span className="narration-panel-parallel-value text-[10px] font-semibold text-on-surface-variant">
                    {narrationMode === 's2s'
                      ? settings.geminiS2sParallelRequests
                      : settings.geminiParallelRequests}
                  </span>
                </div>
                <Slider
                  min={1}
                  max={narrationMode === 's2s' ? 6 : 4}
                  step={1}
                  value={narrationMode === 's2s'
                    ? settings.geminiS2sParallelRequests
                    : settings.geminiParallelRequests}
                  onChange={(value) => {
                    const next = Math.round(value);
                    if (narrationMode === 's2s') {
                      update('geminiS2sParallelRequests', Math.max(1, Math.min(6, next)));
                    } else {
                      update('geminiParallelRequests', Math.max(1, Math.min(4, next)));
                    }
                  }}
                  className="narration-panel-parallel-slider"
                />
                <div className="narration-panel-parallel-warning text-[9px] leading-3 text-on-surface-variant/75">
                  {t.narrationTtsParallelRequestsWarning}
                </div>
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
                {geminiLanguageConditions.map((condition, index) => (
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

          {effectiveTtsMethod === 'GoogleTranslate' && (
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

          {effectiveTtsMethod === 'Kokoro' && (
            <>
              <div className="narration-panel-kokoro-voices mb-2 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsKokoroVoiceConfigs}
                </span>
                {kokoroVoiceConfigs.map((config, index) => {
                  const target = kokoroVoiceLanguageForCondition(config.languageCode);
                  const options = (target
                    ? kokoroVoices.filter((voice) => voice.languageCode === target)
                    : kokoroVoices
                  ).map((voice) => ({ value: voice.id, label: `${voice.id} · ${voice.label}` }));
                  return (
                    <div key={`${config.languageCode}-${index}`} className="narration-panel-kokoro-voice-config flex items-center gap-1.5">
                      <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                        {config.languageName}
                      </span>
                      <PanelSelect
                        value={config.voiceId}
                        options={options}
                        onChange={(value) => updateKokoroVoiceConfig(index, { voiceId: value })}
                        triggerClassName="narration-kokoro-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                        contentClassName="narration-kokoro-voice-menu"
                        searchable={options.length > 8}
                      />
                      <button
                        type="button"
                        onClick={() => removeKokoroVoiceConfig(index)}
                        className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                        title={t.narrationTtsLanguageConditionRemove}
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </div>
                  );
                })}
                {availableKokoroVoiceLanguages.length > 0 && (
                  <PanelSelect
                    value={t.narrationTtsKokoroVoiceConfigAdd}
                    options={availableKokoroVoiceLanguages.map((language) => ({
                      value: language.languageCode,
                      label: language.languageName,
                    }))}
                    onChange={(value) => {
                      const language = availableKokoroVoiceLanguages.find(
                        (item) => item.languageCode === value,
                      );
                      if (language) addKokoroVoiceConfig(language.languageCode, language.languageName);
                    }}
                    triggerClassName="narration-kokoro-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
                    contentClassName="narration-kokoro-voice-add-menu"
                    searchable
                  />
                )}
              </div>
              <div className="narration-panel-row mb-2 flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  {t.narrationTtsSpeed}
                </span>
                <input
                  type="range"
                  min={0.5}
                  max={2}
                  step={0.05}
                  value={settings.kokoroSpeed}
                  onChange={(event) => update('kokoroSpeed', parseFloat(event.target.value))}
                  className="narration-kokoro-speed-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.kokoroSpeed.toFixed(2)}x
                </span>
              </div>
              <div className="narration-panel-row flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  {t.narrationTtsKokoroThreads}
                </span>
                <input
                  type="range"
                  min={1}
                  max={8}
                  step={1}
                  value={settings.kokoroNumThreads}
                  onChange={(event) => update('kokoroNumThreads', parseInt(event.target.value, 10))}
                  className="narration-kokoro-threads-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.kokoroNumThreads}
                </span>
              </div>
            </>
          )}

          {effectiveTtsMethod === 'Supertonic' && (
            <>
              <div className="narration-panel-supertonic-voices mb-2 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  Voice per language
                </span>
                {supertonicVoiceConfigs.map((config, index) => (
                  <div key={`${config.languageCode}-${index}`} className="narration-panel-supertonic-voice-config flex items-center gap-1.5">
                    <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                      {config.languageName}
                    </span>
                    <PanelSelect
                      value={config.voiceId}
                      options={supertonicVoices.map((voice) => ({
                        value: voice.id,
                        label: voice.label,
                      }))}
                      onChange={(value) => updateSupertonicVoiceConfig(index, { voiceId: value })}
                      triggerClassName="narration-supertonic-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                      contentClassName="narration-supertonic-voice-menu"
                    />
                    <button
                      type="button"
                      onClick={() => removeSupertonicVoiceConfig(index)}
                      className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                      title={t.narrationTtsLanguageConditionRemove}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </div>
                ))}
                {availableSupertonicLanguages.length > 0 && (
                  <PanelSelect
                    value="Add language"
                    options={availableSupertonicLanguages.map((language) => ({
                      value: language.languageCode,
                      label: language.languageName,
                    }))}
                    onChange={(value) => {
                      const language = availableSupertonicLanguages.find(
                        (item) => item.languageCode === value,
                      );
                      if (language) addSupertonicVoiceConfig(language.languageCode, language.languageName);
                    }}
                    triggerClassName="narration-supertonic-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
                    contentClassName="narration-supertonic-voice-add-menu"
                    searchable
                  />
                )}
              </div>
              <div className="narration-panel-row mb-2 flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  {t.narrationTtsSpeed}
                </span>
                <input
                  type="range"
                  min={0.5}
                  max={2}
                  step={0.05}
                  value={settings.supertonicSpeed}
                  onChange={(event) => update('supertonicSpeed', parseFloat(event.target.value))}
                  className="narration-supertonic-speed-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.supertonicSpeed.toFixed(2)}x
                </span>
              </div>
              <div className="narration-panel-row mb-2 flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  Steps
                </span>
                <input
                  type="range"
                  min={1}
                  max={20}
                  step={1}
                  value={settings.supertonicNumSteps}
                  onChange={(event) => update('supertonicNumSteps', parseInt(event.target.value, 10))}
                  className="narration-supertonic-steps-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.supertonicNumSteps}
                </span>
              </div>
              <div className="narration-panel-row flex items-center gap-2">
                <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
                  Threads
                </span>
                <input
                  type="range"
                  min={1}
                  max={8}
                  step={1}
                  value={settings.supertonicNumThreads}
                  onChange={(event) => update('supertonicNumThreads', parseInt(event.target.value, 10))}
                  className="narration-supertonic-threads-slider flex-1"
                />
                <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
                  {settings.supertonicNumThreads}
                </span>
              </div>
            </>
          )}

          {effectiveTtsMethod === 'MagpieMultilingual' && (
            <>
              <div className="narration-panel-magpie-voices mb-2 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationTtsKokoroVoiceConfigs}
                </span>
                {magpieVoiceConfigs.map((config, index) => (
                  <div key={`${config.languageCode}-${index}`} className="narration-panel-magpie-voice-config flex items-center gap-1.5">
                    <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                      {config.languageName}
                    </span>
                    <PanelSelect
                      value={config.voiceId}
                      options={magpieVoices.map((voice) => ({
                        value: voice.id,
                        label: voice.label,
                      }))}
                      onChange={(value) => updateMagpieVoiceConfig(index, { voiceId: value })}
                      triggerClassName="narration-magpie-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                      contentClassName="narration-magpie-voice-menu"
                    />
                    <button
                      type="button"
                      onClick={() => removeMagpieVoiceConfig(index)}
                      className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                      title={t.narrationTtsLanguageConditionRemove}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </div>
                ))}
                {availableMagpieVoiceLanguages.length > 0 && (
                  <PanelSelect
                    value={t.narrationTtsKokoroVoiceConfigAdd}
                    options={availableMagpieVoiceLanguages.map((language) => ({
                      value: language.languageCode,
                      label: language.languageName,
                    }))}
                    onChange={(value) => {
                      const language = availableMagpieVoiceLanguages.find(
                        (item) => item.languageCode === value,
                      );
                      if (language) addMagpieVoiceConfig(language.languageCode, language.languageName);
                    }}
                    triggerClassName="narration-magpie-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
                    contentClassName="narration-magpie-voice-add-menu"
                    searchable
                  />
                )}
              </div>
            </>
          )}

          {effectiveTtsMethod === 'StepAudioEditX' && (
            <>
              <div className="narration-panel-step-audio-reference mb-2 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  Reference voice
                </span>
                <PanelSelect
                  value={settings.stepAudioReferenceVoiceId}
                  options={[
                    { value: '', label: 'Bundled default reference' },
                    ...stepAudioVoices.map((voice) => ({
                      value: voice.id,
                      label: voice.label || 'Untitled reference',
                    })),
                  ]}
                  onChange={(value) => update('stepAudioReferenceVoiceId', value)}
                  triggerClassName="narration-step-audio-reference-select h-8 rounded-lg px-2.5 text-[11px]"
                  contentClassName="narration-step-audio-reference-menu"
                  searchable
                />
              </div>
            </>
          )}

          {effectiveTtsMethod === 'VieneuTts' && (
            <>
              <div className="narration-panel-vieneu-reference mb-2 flex flex-col gap-1.5">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  Reference voice
                </span>
                <PanelSelect
                  value={settings.vieneuReferenceVoiceId}
                  options={[
                    { value: '', label: 'Model default voice' },
                    ...referenceVoices.map((voice) => ({
                      value: voice.id,
                      label: voice.label || 'Untitled reference',
                    })),
                  ]}
                  onChange={(value) => update('vieneuReferenceVoiceId', value)}
                  triggerClassName="narration-vieneu-reference-select h-8 rounded-lg px-2.5 text-[11px]"
                  contentClassName="narration-vieneu-reference-menu"
                  searchable
                />
              </div>
            </>
          )}

          {effectiveTtsMethod === 'EdgeTTS' && (
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
                {edgeVoiceConfigs.map((config, index) => {
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
