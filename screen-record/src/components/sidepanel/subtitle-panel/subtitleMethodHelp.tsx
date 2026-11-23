import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import type { Translations } from '@/i18n';
import { HelpCircle } from '@/components/ui/MaterialIcon';
import type { PanelSelectOption } from '@/components/ui/PanelSelect';
import { Tooltip } from '@/components/ui/Tooltip';
import { SUBTITLE_LANGUAGE_OPTIONS } from '@/lib/subtitleLanguageOptions';

const METHOD_LANGUAGE_PREVIEW_LIMIT = 14;

interface SubtitleMethodCapability {
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}

export function subtitleMethodUsesLanguageHint(method: SubtitleMethod): boolean {
  return !(
    method === 'gemini-3-1-flash-lite'
    || method === 'gemini-3-flash-preview'
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
    case 'groq-whisper-accurate':
    default:
      return t.subtitleMethodGroqWhisperAccurate;
  }
}

function summarizeLanguageSupport(method: SubtitleMethod, t: Translations): string {
  if (method === 'gemini-3-1-flash-lite' || method === 'gemini-3-flash-preview') {
    return t.subtitleMethodHelpGeminiLanguages;
  }

  const languageLabels = SUBTITLE_LANGUAGE_OPTIONS
    .filter((option) => option.value !== 'auto')
    .map((option) => option.label);
  const preview = languageLabels.slice(0, METHOD_LANGUAGE_PREVIEW_LIMIT).join(', ');
  const remaining = languageLabels.length - METHOD_LANGUAGE_PREVIEW_LIMIT;

  if (remaining <= 0) {
    return preview || t.subtitleMethodHelpUnknownLanguages;
  }

  return `${t.subtitleMethodHelpGroqLanguages}: ${preview}, ${t.subtitleMethodHelpMoreLanguages.replace('{count}', String(remaining))}`;
}

function getMethodHelpContent(method: SubtitleMethod, t: Translations) {
  return (
    <div className="subtitle-method-help-tooltip max-w-[280px] space-y-1 text-left leading-4">
      <div className="font-semibold text-[var(--on-surface)]">{getSubtitleMethodLabel(method, t)}</div>
      <div>
        <span className="text-[var(--on-surface-variant)]">{t.subtitleMethodHelpRuntime}: </span>
        <span>{t.subtitleMethodHelpRuntimeCloud}</span>
      </div>
      <div>
        <span className="text-[var(--on-surface-variant)]">{t.subtitleMethodHelpLanguages}: </span>
        <span>{summarizeLanguageSupport(method, t)}</span>
      </div>
    </div>
  );
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
    disabled: !method.available,
  }));
}
