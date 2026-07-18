import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TimelineLabelColumn } from "@/components/timeline/TimelineLabelColumn";
import { getTranslations } from "@/i18n";
import type { VideoSegment } from "@/types/video";

const t = getTranslations("en");

function renderDeviceAudioLabel(
  segment: VideoSegment,
  setSegment = vi.fn(),
) {
  const beginBatch = vi.fn();
  const commitBatch = vi.fn();
  render(
    <TimelineLabelColumn
      t={t}
      segment={segment}
      duration={10}
      showZoom={false}
      showDebug={false}
      setShowDebug={vi.fn()}
      showSpeed={false}
      showImportedAudio={false}
      showDeviceAudio
      showMicAudio={false}
      showWebcam={false}
      showNarration={false}
      showKeystroke={false}
      showPointer={false}
      showTrimLane={false}
      keystrokeTrackLabel="Keys"
      onTriggerImportedAudioPicker={vi.fn()}
      onTriggerSubtitlePicker={vi.fn()}
      canPickImportedAudioFile={false}
      canPickSubtitleFile={false}
      onAudioTrackDownload={vi.fn()}
      currentRawMicAudioPath=""
      isMicAudioAvailable={false}
      isWebcamAvailable={false}
      volumeViewEnabled={false}
      setVolumeViewEnabled={vi.fn()}
      setSegment={setSegment}
      beginBatch={beginBatch}
      commitBatch={commitBatch}
    />,
  );
  return { beginBatch, commitBatch, setSegment };
}

describe("TimelineLabelColumn audio delay controls", () => {
  it("places the Device Audio delay slider after download and updates bounded track state", () => {
    const segment = {
      trimStart: 0,
      trimEnd: 10,
      zoomKeyframes: [],
      textSegments: [],
      subtitleSegments: [],
      deviceAudioAvailable: true,
      deviceAudioOffsetSec: 0.25,
    } satisfies VideoSegment;
    const { beginBatch, commitBatch, setSegment } =
      renderDeviceAudioLabel(segment);

    expect(
      screen.getByRole("button", {
        name: `${t.downloadAudioTrack}: ${t.trackDeviceAudio}`,
      }),
    ).toBeInTheDocument();
    const popover = document.querySelector(
      ".timeline-label-device-audio-delay-popover",
    ) as HTMLElement;
    expect(popover.style.marginLeft).toBe("28px");
    const hoverBridge = document.querySelector(
      ".timeline-label-track-delay-hover-bridge",
    ) as HTMLElement;
    expect(hoverBridge.style.width).toBe("40px");
    expect(hoverBridge).toHaveClass(
      "h-14",
      "bg-transparent",
      "pointer-events-none",
      "group-hover:pointer-events-auto",
      "group-focus-within:pointer-events-auto",
    );

    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("min", "-2");
    expect(slider).toHaveAttribute("max", "2");
    expect(slider).toHaveValue("0.25");

    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "1.75" } });
    fireEvent.pointerUp(slider);

    expect(beginBatch).toHaveBeenCalledTimes(1);
    expect(commitBatch).toHaveBeenCalledTimes(1);
    expect(setSegment).toHaveBeenLastCalledWith({
      ...segment,
      deviceAudioOffsetSec: 1.75,
    });
  });
});
