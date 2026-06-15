/**
 * Options for a single persisted setting.
 *
 * - `parse` converts the raw `localStorage` string (or `null` when the key is
 *   absent) into the typed value, applying any validation / clamping /
 *   defaulting / legacy-key migration the setting requires.
 * - `serialize` converts the typed value into the string to store. Return
 *   `null` to *remove* the key instead of writing it.
 * - `fallback` is currently informational; `parse` is the single source of
 *   truth for the default, so callers can reference `fallback` from inside
 *   `parse` if convenient.
 */
export interface PersistedSettingOptions<T> {
  parse: (raw: string | null) => T;
  serialize: (value: T) => string | null;
  fallback: T;
}

export interface PersistedSetting<T> {
  /** Read + parse the stored value (private-mode safe). */
  getInitial(): T;
  /** Serialize + write the value (private-mode safe). */
  persist(value: T): void;
}

/**
 * Owns the try/catch (private-mode safe), the `localStorage` read+parse, and
 * the write for a single setting keyed by `key`. All validation/migration lives
 * in the per-setting `parse`/`serialize` closures the caller supplies.
 */
export function createPersistedSetting<T>(
  key: string,
  options: PersistedSettingOptions<T>,
): PersistedSetting<T> {
  const { parse, serialize, fallback } = options;

  const getInitial = (): T => {
    try {
      return parse(localStorage.getItem(key));
    } catch {
      // ignore persistence failures (e.g. private mode) and fall back
      return fallback;
    }
  };

  const persist = (value: T): void => {
    try {
      const serialized = serialize(value);
      if (serialized === null) {
        localStorage.removeItem(key);
        return;
      }
      localStorage.setItem(key, serialized);
    } catch {
      // ignore persistence failures
    }
  };

  return { getInitial, persist };
}

