/**
 * Media element synchronization helpers.
 *
 * These functions handle syncing playback state (time, rate, volume,
 * play/pause) across the main video, webcam video, device audio, and
 * mic audio elements.
 */

// ---------------------------------------------------------------------------
// Element validity
// ---------------------------------------------------------------------------

export function hasValidMediaElement(
  element?: HTMLMediaElement,
): boolean {
  return !!(
    element &&
    element.src &&
    element.src !== "" &&
    element.src !== window.location.href
  );
}

// ---------------------------------------------------------------------------
// Time sync
// ---------------------------------------------------------------------------

export function getTimedMediaCurrentTime(
  videoTime: number,
  offsetSec: number,
): number {
  return Math.max(0, videoTime - offsetSec);
}

export function shouldTimedMediaBeActive(
  videoTime: number,
  offsetSec: number,
): boolean {
  return videoTime >= offsetSec - 0.001;
}

export function syncAudioElementTime(
  element: HTMLMediaElement | undefined,
  time: number,
): void {
  if (!element || !hasValidMediaElement(element)) return;
  if (Math.abs(element.currentTime - time) > 0.05) {
    element.currentTime = time;
  }
}

export function syncTimedMediaElementTime(
  element: HTMLMediaElement | undefined,
  videoTime: number,
  offsetSec: number,
): void {
  syncAudioElementTime(
    element,
    getTimedMediaCurrentTime(videoTime, offsetSec),
  );
}

// ---------------------------------------------------------------------------
// Playback rate sync
// ---------------------------------------------------------------------------

export function syncAudioElementPlaybackRate(
  element: HTMLMediaElement | undefined,
  playbackRate: number,
): void {
  if (!element || !hasValidMediaElement(element)) return;
  element.playbackRate = playbackRate;
}

// ---------------------------------------------------------------------------
// Play / pause helpers
// ---------------------------------------------------------------------------

export function playAudioElement(
  element: HTMLMediaElement | undefined,
): Promise<void> | null {
  if (!element || !hasValidMediaElement(element)) return null;
  return element.play().catch(() => {});
}

export function pauseAudioElement(
  element: HTMLMediaElement | undefined,
  pendingPromise: Promise<void> | null,
): void {
  if (!element || !hasValidMediaElement(element)) return;
  if (pendingPromise) {
    pendingPromise.then(() => element.pause()).catch(() => {});
    return;
  }
  element.pause();
}

/**
 * Synchronize a timed media element (webcam or mic) during playback.
 * Returns the new pending play promise (or null if paused/stopped).
 */
export function syncTimedMediaPlayback(
  element: HTMLMediaElement | undefined,
  isValid: boolean,
  pendingPromise: Promise<void> | null,
  videoTime: number,
  offsetSec: number,
): Promise<void> | null {
  if (!element || !isValid) {
    return null;
  }

  const targetTime = getTimedMediaCurrentTime(videoTime, offsetSec);
  if (Math.abs(element.currentTime - targetTime) > 0.05) {
    element.currentTime = targetTime;
  }

  if (!shouldTimedMediaBeActive(videoTime, offsetSec)) {
    pauseAudioElement(element, pendingPromise);
    return null;
  }

  if (element.paused) {
    return playAudioElement(element);
  }

  return pendingPromise;
}

// ---------------------------------------------------------------------------
// Source clearing
// ---------------------------------------------------------------------------

export function clearMediaElementSource(element: HTMLMediaElement): void {
  element.pause();
  element.removeAttribute("src");
  element.load();
}
