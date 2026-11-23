/**
 * Frame-accurate playback time for timeline-only projects (no <video> clock).
 * The playback loop publishes every frame; the playhead reads it in its own
 * animation frame, so React state only needs low-frequency updates.
 */
let playbackTime = 0;

export function publishTimelinePlaybackTime(time: number): void {
  playbackTime = time;
}

export function readTimelinePlaybackTime(): number {
  return playbackTime;
}
