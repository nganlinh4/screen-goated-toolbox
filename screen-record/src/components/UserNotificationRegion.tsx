import { useEffect, useRef, useState } from "react";
import { X } from "@/components/ui/MaterialIcon";
import { useSettings } from "@/hooks/useSettings";
import {
  notifyUserError,
  USER_NOTIFICATION_EVENT,
  type UserNotificationDetail,
} from "@/lib/userNotifications";

const DISPLAY_MS = 6000;

export function UserNotificationRegion() {
  const { t } = useSettings();
  const [notification, setNotification] = useState<UserNotificationDetail | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    const clearTimer = () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = null;
    };
    const show = (detail: UserNotificationDetail) => {
      clearTimer();
      setNotification(detail);
      timerRef.current = window.setTimeout(() => setNotification(null), DISPLAY_MS);
    };
    const onNotification = (event: Event) => {
      show((event as CustomEvent<UserNotificationDetail>).detail);
    };
    const onWindowError = () => notifyUserError("unexpectedError");
    const onUnhandledRejection = () => notifyUserError("unexpectedError");

    window.addEventListener(USER_NOTIFICATION_EVENT, onNotification);
    window.addEventListener("error", onWindowError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);
    return () => {
      clearTimer();
      window.removeEventListener(USER_NOTIFICATION_EVENT, onNotification);
      window.removeEventListener("error", onWindowError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
    };
  }, []);

  if (!notification) return null;
  return (
    <div
      className="user-notification-region fixed bottom-5 left-1/2 z-[300] flex max-w-md -translate-x-1/2 items-center gap-3 rounded-xl border border-[var(--tertiary-color)]/40 bg-[var(--ui-surface-3)] px-4 py-3 text-sm text-[var(--on-surface)] shadow-xl"
      role="alert"
      aria-live="assertive"
    >
      <span className="user-notification-message">{t[notification.messageKey]}</span>
      <button
        type="button"
        className="user-notification-close ui-icon-button shrink-0 p-1"
        aria-label={t.close}
        onClick={() => setNotification(null)}
      >
        <X className="h-4 w-4" aria-hidden="true" />
      </button>
    </div>
  );
}
