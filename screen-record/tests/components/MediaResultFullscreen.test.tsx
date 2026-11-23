import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MediaResultDialog } from "@/components/dialogs/MediaResultDialog";

vi.mock("@/lib/mediaServer", () => ({
  getMediaServerUrl: vi.fn(async (path: string) => `http://media.test/${encodeURIComponent(path)}`),
}));
vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));

function currentVideo(): HTMLVideoElement {
  const video = document.querySelector("video");
  expect(video).not.toBeNull();
  return video as HTMLVideoElement;
}

describe("MediaResultDialog fullscreen", () => {
  it("keeps the same video element (and so its playback position) across fullscreen toggles", async () => {
    render(
      <MediaResultDialog show onClose={vi.fn()} title="Export" filePath="C:/exports/clip.mp4" onFilePathChange={vi.fn()} />,
    );
    await act(async () => { await Promise.resolve(); });
    const video = currentVideo();
    expect(video.isConnected).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Enter fullscreen" }));
    expect(currentVideo()).toBe(video);
    expect(video.isConnected).toBe(true);
    expect(video.closest(".media-result-fullscreen")).not.toBeNull();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Exit fullscreen" }));
    expect(currentVideo()).toBe(video);
    expect(video.isConnected).toBe(true);
    expect(video.closest(".media-preview-box")).not.toBeNull();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
