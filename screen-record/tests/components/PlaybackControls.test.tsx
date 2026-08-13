import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PlaybackControls } from "@/components/VideoPreview";

const baseProps = {
  isPlaying: false,
  isProcessing: false,
  isVideoReady: true,
  isCropping: false,
  currentTime: 1.25,
  duration: 10,
  onTogglePlayPause: vi.fn(),
  onToggleCrop: vi.fn(),
};

describe("PlaybackControls frame download", () => {
  it("places the compact frame action immediately after Crop and reports success", async () => {
    const onDownloadFrame = vi.fn(async () => "C:\\Users\\user\\Downloads\\frame.png");
    render(<PlaybackControls {...baseProps} onDownloadFrame={onDownloadFrame} />);

    const cropButton = screen.getByRole("button", { name: "Crop Video" });
    const frameButton = screen.getByRole("button", { name: "Save current frame" });
    expect(cropButton.nextElementSibling).toBe(frameButton);

    fireEvent.click(frameButton);
    expect(onDownloadFrame).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Frame saved" })).toBeInTheDocument();
    });
  });

  it("disables frame capture while the preview is not ready", () => {
    render(
      <PlaybackControls
        {...baseProps}
        isVideoReady={false}
        onDownloadFrame={vi.fn(async () => "frame.png")}
      />,
    );

    expect(screen.getByRole("button", { name: "Save current frame" })).toBeDisabled();
  });
});
