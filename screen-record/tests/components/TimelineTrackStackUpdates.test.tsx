import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { VideoSegment } from "@/types/video";

const captured = vi.hoisted(() => ({ props: {} as Record<string, Record<string, unknown>> }));
function capture(name: string) {
  return (props: Record<string, unknown>) => { captured.props[name] = props; return null; };
}
vi.mock("@/components/timeline/SpeedTrack", () => ({ SpeedTrack: capture("speed") }));
vi.mock("@/components/timeline/MicTrack", () => ({ MicTrack: capture("mic") }));
vi.mock("@/components/timeline/DeviceAudioTrack", () => ({ DeviceAudioTrack: capture("device") }));
vi.mock("@/components/timeline/ZoomTrack", () => ({ ZoomTrack: capture("zoom") }));
vi.mock("@/components/timeline/ImportedAudioTrack", () => ({ ImportedAudioTrack: () => null }));
vi.mock("@/components/timeline/KeystrokeTrack", () => ({ KeystrokeTrack: () => null }));
vi.mock("@/components/timeline/NarrationTrack", () => ({ NarrationTrack: () => null }));
vi.mock("@/components/timeline/PointerTrack", () => ({ PointerTrack: () => null }));
vi.mock("@/components/timeline/SubtitleTrack", () => ({ SubtitleTrack: () => null }));
vi.mock("@/components/timeline/TextTrack", () => ({ TextTrack: () => null }));
vi.mock("@/components/timeline/TrimTrack", () => ({ TrimTrack: () => null }));
vi.mock("@/components/timeline/WebcamVisibilityTrack", () => ({ WebcamVisibilityTrack: () => null }));
vi.mock("@/components/timeline/ZoomDebugOverlay", () => ({ ZoomDebugOverlay: () => null }));

import { TimelineTrackStack } from "@/components/timeline/TimelineTrackStack";

describe("TimelineTrackStack lane updates", () => {
  it("applies a point drag to the latest segment instead of the render-time copy", () => {
    const rendered = { trimStart: 0, trimEnd: 10, subtitleSegments: [] } as unknown as VideoSegment;
    let latest: VideoSegment | null = rendered;
    const setSegment = vi.fn((value: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null)) => {
      latest = typeof value === "function" ? value(latest) : value;
    });
    render(
      <TimelineTrackStack
        {...({} as Record<string, unknown>)}
        segment={rendered} setSegment={setSegment} duration={10}
        showZoom showSpeed showMicAudio showDeviceAudio
        beginBatch={vi.fn()} commitBatch={vi.fn()}
        {...({} as never)}
      />,
    );
    // A subtitle result lands while the user is mid-drag (no re-render yet).
    latest = { ...rendered, subtitleSegments: [{ id: "s1", startTime: 0, endTime: 1, text: "hi" }] } as unknown as VideoSegment;

    (captured.props.speed.onUpdateSpeedPoints as (p: unknown[]) => void)([{ time: 1, speed: 2 }]);
    (captured.props.mic.onUpdateMicAudioPoints as (p: unknown[]) => void)([{ time: 1, volume: 0.5 }]);
    (captured.props.zoom.onUpdateBlocks as (b: unknown[]) => void)([{ id: "z" }]);

    expect(latest).toMatchObject({
      subtitleSegments: [{ id: "s1" }],
      speedPoints: [{ time: 1, speed: 2 }],
      micAudioPoints: [{ time: 1, volume: 0.5 }],
      zoomBlocks: [{ id: "z" }],
    });
  });
});
