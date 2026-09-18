import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { CameraPanel } from "@/components/sidepanel/CameraPanel";
import { TextPanel } from "@/components/sidepanel/TextPanel";
import { getNewRecordingCameraConfig } from "@/lib/recordingDefaults";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import { getDefaultStyle, saveStyleDefault } from "@/lib/stylePreferences";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";

beforeEach(() => localStorage.clear());

it("saves explicit camera edits but does not learn from loading another project", () => {
  const first = render(<CameraPanel webcamConfig={DEFAULT_WEBCAM_CONFIG}
    setWebcamConfig={vi.fn()} webcamAvailable beginBatch={vi.fn()} commitBatch={vi.fn()} />);
  fireEvent.click(screen.getByRole("checkbox", { name: "Mirror", exact: true }));
  expect(getNewRecordingCameraConfig(true)).toMatchObject({ mirror: true, visible: false });
  first.unmount();
  render(<CameraPanel webcamConfig={{ ...DEFAULT_WEBCAM_CONFIG, visible: true }}
    setWebcamConfig={vi.fn()} webcamAvailable beginBatch={vi.fn()} commitBatch={vi.fn()} />);
  expect(getNewRecordingCameraConfig(true)).toMatchObject({ mirror: true, visible: false });
});

it("saves text styling only when edited, independently from subtitle styling", () => {
  const style = defaultSubtitleStyle();
  saveStyleDefault("text", { ...style, fontSize: 100 });
  const update = vi.fn();
  render(<TextPanel segment={{ trimStart: 0, trimEnd: 10, zoomKeyframes: [],
    textSegments: [{ id: "text", text: "Existing text", startTime: 0, endTime: 2, style }] }}
    editingTextId="text" onUpdateSegment={update} beginBatch={vi.fn()} commitBatch={vi.fn()} />);
  expect(getDefaultStyle("text", style).fontSize).toBe(100);
  fireEvent.change(screen.getByRole("slider", { name: /font size/i }), { target: { value: "130" } });
  expect(getDefaultStyle("text", style).fontSize).toBe(130);
  expect(defaultSubtitleStyle().fontSize).toBe(54);
  expect(update).toHaveBeenCalledWith(expect.objectContaining({
    textSegments: [expect.objectContaining({ text: "Existing text", startTime: 0, endTime: 2 })],
  }));
});
