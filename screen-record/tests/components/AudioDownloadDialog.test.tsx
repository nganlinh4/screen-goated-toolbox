import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AudioDownloadDialog } from "@/components/dialogs/AudioDownloadDialog";

vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn(async () => "C:\\Users\\user\\Downloads"),
}));

describe("AudioDownloadDialog", () => {
  it("renders format choices and starts download", () => {
    const onDownload = vi.fn();
    const onFormatChange = vi.fn();

    render(
      <AudioDownloadDialog
        show
        onClose={() => {}}
        trackLabel="Narration"
        format="wav"
        onFormatChange={onFormatChange}
        outputDir="C:\\Users\\user\\Downloads"
        onOutputDirChange={() => {}}
        onDownload={onDownload}
      />,
    );

    expect(screen.getByText("Narration")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /mp3/i }));
    fireEvent.click(screen.getByRole("button", { name: /download/i }));

    expect(onFormatChange).toHaveBeenCalledWith("mp3");
    expect(onDownload).toHaveBeenCalledTimes(1);
  });
});
