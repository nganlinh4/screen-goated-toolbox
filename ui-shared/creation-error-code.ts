// Stable failure codes sent by creation hosts (src/overlay/creation_error.rs).
// A rejected bridge call carries only the code as its message; each app owns
// the localized text for the message key that a code selects.

export const CREATION_ERROR_MESSAGE_KEYS = {
  name_invalid: "errorNameInvalid",
  name_taken: "errorNameTaken",
  result_missing: "errorResultMissing",
  result_busy: "errorResultBusy",
  queue_full: "errorQueueFull",
} as const;

export type CreationErrorCode = keyof typeof CREATION_ERROR_MESSAGE_KEYS | "operation_failed";
export type CreationErrorMessageKey =
  (typeof CREATION_ERROR_MESSAGE_KEYS)[keyof typeof CREATION_ERROR_MESSAGE_KEYS];

export function creationErrorCode(error: unknown): CreationErrorCode {
  const message = error instanceof Error ? error.message : error;
  return typeof message === "string"
    && Object.prototype.hasOwnProperty.call(CREATION_ERROR_MESSAGE_KEYS, message)
    ? message as CreationErrorCode
    : "operation_failed";
}

/** The specific message key for a coded failure, else the action's own key. */
export function creationErrorMessageKey<Fallback extends string>(
  error: unknown,
  fallback: Fallback,
): CreationErrorMessageKey | Fallback {
  const code = creationErrorCode(error);
  return code === "operation_failed" ? fallback : CREATION_ERROR_MESSAGE_KEYS[code];
}
