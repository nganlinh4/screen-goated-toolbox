import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: { startAnimation: vi.fn() } }));

import { doHandleSeeked, doSeek, type ControllerInternals } from "@/lib/videoControllerPlayback";

function controller(): ControllerInternals {
  const video = { currentTime: 0, paused: true, readyState: 4 } as unknown as HTMLVideoElement;
  const c = {
    video,
    state: { isReady: true, isSeeking: false, isPlaying: false, currentTime: 0, duration: 20 },
    renderOptions: undefined,
    isChangingSource: false,
    isGeneratingThumbnail: false,
    pendingSeekTime: null,
    lastRequestedSeekTime: null,
    SEGMENT_EPS: 0.001,
    playRequestSeq: 0,
    recoveryState: { anchorTime: null, anchorExpiresAt: 0, retryCount: 0 },
    resetLogState: {},
    getEffectiveDuration: () => 20,
    getSegmentStartTime: () => 0,
    syncAllMediaToTime: vi.fn(),
    applyAudioTrackVolumes: vi.fn(),
    renderFrame: vi.fn(),
    armPlaybackRecoveryAnchor: vi.fn(),
    setSeeking: vi.fn((seeking: boolean) => { c.state.isSeeking = seeking; }),
    setCurrentTime: vi.fn((time: number) => { c.state.currentTime = time; }),
  } as unknown as ControllerInternals;
  return c;
}

describe("coalesced scrub seeks", () => {
  it("moves webcam and external audio to the final scrub position, not the first", () => {
    const c = controller();
    doSeek(c, 2);          // starts a real seek
    doSeek(c, 5);          // queued while the first is in flight
    doSeek(c, 9);          // replaces the queued target
    c.video.currentTime = 2;
    doHandleSeeked(c);     // first seek lands, queued one starts
    c.video.currentTime = 9;
    doHandleSeeked(c);     // final seek lands; paused, so nothing else resyncs

    expect(c.video.currentTime).toBe(9);
    expect(vi.mocked(c.syncAllMediaToTime).mock.calls.at(-1)).toEqual([9]);
  });
});
