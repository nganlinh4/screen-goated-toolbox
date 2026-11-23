import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MediaResultDialog } from "@/components/dialogs/MediaResultDialog";

const media = vi.hoisted(() => ({
  getMediaServerUrl: vi.fn(async (path: string) => `http://media.test/${encodeURIComponent(path)}`),
}));
vi.mock("@/lib/mediaServer", () => ({ getMediaServerUrl: media.getMediaServerUrl }));
vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@/components/dialogs/MediaResultPlayers", () => ({
  CustomAudioPlayer: () => null,
  CustomVideoPlayer: ({ src }: { src: string }) => <div data-testid="player" data-src={src} />,
}));

function renderDialog(filePath: string, onClose = vi.fn()) {
  const view = render(
    <MediaResultDialog show onClose={onClose} title="Export" filePath={filePath} onFilePathChange={vi.fn()} />,
  );
  return { ...view, onClose };
}

describe("MediaResultDialog", () => {
  it("cancels an active rename with Escape instead of closing the dialog", async () => {
    const { onClose } = renderDialog("C:/exports/clip.mp4");
    await act(async () => { await Promise.resolve(); });
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    const input = screen.getByDisplayValue("clip.mp4");
    fireEvent.change(input, { target: { value: "draft.mp4" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByDisplayValue("draft.mp4")).not.toBeInTheDocument();
    expect(screen.getByText("clip.mp4")).toBeInTheDocument();
  });

  it("requests a fresh stream URL after the file path changes", async () => {
    media.getMediaServerUrl.mockClear();
    const view = renderDialog("C:/exports/clip.mp4");
    await act(async () => { await Promise.resolve(); });
    view.rerender(
      <MediaResultDialog show onClose={view.onClose} title="Export" filePath="C:/exports/renamed.mp4" onFilePathChange={vi.fn()} />,
    );
    await act(async () => { await Promise.resolve(); });
    expect(media.getMediaServerUrl.mock.calls.map(([path]) => path)).toEqual([
      "C:/exports/clip.mp4", "C:/exports/renamed.mp4",
    ]);
  });
});
