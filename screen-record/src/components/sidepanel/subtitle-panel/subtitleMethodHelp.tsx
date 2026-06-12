import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import type { Translations } from '@/i18n';
import { HelpCircle } from '@/components/ui/MaterialIcon';
import type { PanelSelectOption } from '@/components/ui/PanelSelect';
import { Tooltip } from '@/components/ui/Tooltip';
import { getSubtitleLanguageOptionsForMethod } from '@/lib/subtitleLanguageOptions';

const METHOD_LANGUAGE_PREVIEW_LIMIT = 14;

const LOCAL_SUBTITLE_METHODS = new Set<SubtitleMethod>([
  'qwen-local-0-6b',
  'qwen-local-1-7b',
  'parakeet-tdt-0-6b-v3',
]);

const PARAKEET_TDT_0_6B_V3_LANGUAGES = [
  'Bulgarian',
  'Croatian',
  'Czech',
  'Danish',
  'Dutch',
  'English',
  'Estonian',
  'Finnish',
  'French',
  'German',
  'Greek',
  'Hungarian',
  'Italian',
  'Latvian',
  'Lithuanian',
  'Maltese',
  'Polish',
  'Portuguese',
  'Romanian',
  'Slovak',
  'Slovenian',
  'Spanish',
  'Swedish',
  'Russian',
  'Ukrainian',
];

interface SubtitleMethodCapability {
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}

export function subtitleMethodUsesLanguageHint(method: SubtitleMethod): boolean {
  return !(
    method === 'gemini-3-1-flash-lite'
    || method === 'gemini-3-flash-preview'
    || method === 'parakeet-tdt-0-6b-v3'
  );
}

export function subtitleMethodUsesGeminiPrompt(method: SubtitleMethod): boolean {
  return method === 'gemini-3-1-flash-lite' || method === 'gemini-3-flash-preview';
}

export function subtitleMethodUsesGroqVocabulary(method: SubtitleMethod): boolean {
  return method === 'groq-whisper-accurate' || method === 'groq-whisper-large-v3-turbo';
}

export function getSubtitleMethodLabel(method: SubtitleMethod, t: Translations): string {
  switch (method) {
    case 'groq-whisper-large-v3-turbo':
      return t.subtitleMethodGroqWhisperLargeV3Turbo;
    case 'gemini-3-1-flash-lite':
      return t.subtitleMethodGemini3_1FlashLite;
    case 'gemini-3-flash-preview':
      return t.subtitleMethodGemini3FlashPreview;
    case 'qwen-local-1-7b':
      return t.subtitleMethodQwenLocal1_7B;
    case 'qwen-local-0-6b':
      return t.subtitleMethodQwenLocal0_6B;
    case 'parakeet-tdt-0-6b-v3':
      return t.subtitleMethodParakeetTdt0_6BV3;
    case 'groq-whisper-accurate':
    default:
      return t.subtitleMethodGroqWhisperAccurate;
  }
}

function summarizeLanguageSupport(method: SubtitleMethod, t: Translations): string {
  if (method === 'gemini-3-1-flash-lite' || method === 'gemini-3-flash-preview') {
    return t.subtitleMethodHelpGeminiLanguages;
  }

  if (method === 'parakeet-tdt-0-6b-v3') {
    return `${t.subtitleMethodHelpParakeetLanguages}: ${PARAKEET_TDT_0_6B_V3_LANGUAGES.join(', ')}`;
  }

  const languageLabels = getSubtitleLanguageOptionsForMethod(method)
    .filter((option) => option.value !== 'auto')
    .map((option) => option.label);
  const preview = languageLabels.slice(0, METHOD_LANGUAGE_PREVIEW_LIMIT).join(', ');
  const remaining = languageLabels.length - METHOD_LANGUAGE_PREVIEW_LIMIT;

  if (remaining <= 0) {
    return preview || t.subtitleMethodHelpUnknownLanguages;
  }

  const prefix = method === 'qwen-local-0-6b' || method === 'qwen-local-1-7b'
    ? t.subtitleMethodHelpQwenLanguages
    : t.subtitleMethodHelpGroqLanguages;

  return `${prefix}: ${preview}, ${t.subtitleMethodHelpMoreLanguages.replace('{count}', String(remaining))}`;
}

function getMethodHelpContent(method: SubtitleMethod, t: Translations) {
  const runtime = LOCAL_SUBTITLE_METHODS.has(method)
    ? t.subtitleMethodHelpRuntimeLocal
    : t.subtitleMethodHelpRuntimeCloud;

  return (
    <div className="subtitle-method-help-tooltip max-w-[280px] space-y-1 text-left leading-4">
      <div className="font-semibold text-[var(--on-surface)]">{getSubtitleMethodLabel(method, t)}</div>
      <div>
        <span className="text-[var(--on-surface-variant)]">{t.subtitleMethodHelpRuntime}: </span>
        <span>{runtime}</span>
      </div>
      <div>
        <span className="text-[var(--on-surface-variant)]">{t.subtitleMethodHelpLanguages}: </span>
        <span>{summarizeLanguageSupport(method, t)}</span>
      </div>
    </div>
  );
}

function canDisplayUnavailableLocalTool(method: SubtitleMethodCapability): boolean {
  return (
    method.method === 'qwen-local-0-6b'
    || method.method === 'qwen-local-1-7b'
    || method.method === 'parakeet-tdt-0-6b-v3'
  ) && method.reason?.includes('Downloaded Tools') === true;
}

export function buildSubtitleMethodOptions(
  t: Translations,
  methodCapabilities: SubtitleMethodCapability[],
): PanelSelectOption[] {
  return methodCapabilities.map((method) => ({
    value: method.method,
    label: getSubtitleMethodLabel(method.method, t),
    trailing: (
      <Tooltip content={getMethodHelpContent(method.method, t)} side="left" delayDuration={150}>
        <button
          type="button"
          className="subtitle-method-option-help flex h-6 w-6 items-center justify-center rounded-md text-on-surface-variant transition-colors hover:bg-[color-mix(in_srgb,var(--primary-color)_12%,transparent)] hover:text-[var(--primary-color)]"
          aria-label={`${t.subtitleMethodHelpLabel}: ${getSubtitleMethodLabel(method.method, t)}`}
        >
          <HelpCircle className="h-3.5 w-3.5" />
        </button>
      </Tooltip>
    ),
    disabled: !method.available && !canDisplayUnavailableLocalTool(method),
  }));
}
