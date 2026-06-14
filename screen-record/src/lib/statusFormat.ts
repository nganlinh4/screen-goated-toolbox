import type { Translations } from '@/i18n';

/**
 * Substitute `{key}` placeholders in a template string with the provided
 * params. Values are coerced via `String(...)` so numbers and strings are both
 * accepted; `null`/`undefined` params are treated as an empty substitution set.
 */
export function formatTemplate(
  template: string,
  params?: Record<string, string | number> | null,
): string {
  let formatted = template;
  for (const [key, value] of Object.entries(params ?? {})) {
    formatted = formatted.split(`{${key}}`).join(String(value));
  }
  return formatted;
}

/**
 * Resolve a localized status message: when `messageKey` names a known
 * translation, format that template with `messageParams`; otherwise fall back
 * to the raw `message`.
 */
export function localizeMessageKey(
  t: Translations,
  status: {
    message: string;
    messageKey?: string | null;
    messageParams?: Record<string, string | number> | null;
  },
): string {
  const key = status.messageKey;
  if (key && key in t) {
    return formatTemplate(t[key as keyof Translations] as string, status.messageParams);
  }
  return status.message;
}
