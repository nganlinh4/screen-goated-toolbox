import type { Translations } from "@/i18n";

export const USER_NOTIFICATION_EVENT = "sgt-recorder-user-notification";

export interface UserNotificationDetail {
  id: number;
  messageKey: keyof Translations;
}

let nextNotificationId = 1;

export function notifyUserError(
  messageKey: keyof Translations,
  error?: unknown,
): void {
  if (error !== undefined) console.error(`[Recorder] ${messageKey}`, error);
  const detail = error instanceof Error ? error.message : error;
  const notificationKey = detail === "SGT_SETTINGS_OUTCOME_UNKNOWN"
    ? "settingsOutcomeUnknown"
    : messageKey;
  window.dispatchEvent(new CustomEvent<UserNotificationDetail>(
    USER_NOTIFICATION_EVENT,
    { detail: { id: nextNotificationId++, messageKey: notificationKey } },
  ));
}
