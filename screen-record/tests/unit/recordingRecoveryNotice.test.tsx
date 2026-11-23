import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { RecordingRecoveryNotice } from "@/components/RecordingRecoveryNotice";
import { invoke } from "@/lib/ipc";
import { recordingHandoff } from "@/lib/recordingHandoff";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn() }));
vi.mock("@/hooks/useSettings", () => ({ useSettings: () => ({ t: {
  failedRecordingFiles: "Files retained", failedRecordingWait: "Waiting",
  failedRecordingKeep: "Keep files",
  recordingRetryDescription: "Recording not saved", recordingRetryButton: "Retry saving",
} }) }));

afterEach(async () => {
  vi.clearAllMocks();
  recordingHandoff.cancelPrepare();
  await recordingHandoff.stop(async () => true);
});

it("shows retained paths, waits for writers, and preserves the retry after a lost response", async () => {
  const failure = { id: 7, error: "Encoder failed", directory: "C:/recordings", paths: ["C:/recordings/video.mp4"], canRelease: true };
  let attempts = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_failed_recording") return failure as never;
    if (command === "release_failed_recording" && ++attempts === 1) throw new Error("response lost");
    return null as never;
  });
  const abandon = vi.fn();
  window.addEventListener("sr-abandon-recording", abandon);
  render(<RecordingRecoveryNotice />);
  expect(await screen.findByText(failure.paths[0])).toBeTruthy();
  fireEvent.click(screen.getByText("Keep files"));
  await waitFor(() => expect(attempts).toBe(1));
  expect(screen.getByText("Keep files")).toBeTruthy();
  fireEvent.click(screen.getByText("Keep files"));
  await waitFor(() => expect(abandon).toHaveBeenCalledOnce());
  expect(invoke).toHaveBeenCalledWith("release_failed_recording", { id: 7 });
  expect(vi.mocked(invoke).mock.calls.every(([command]) => !command.includes("delete"))).toBe(true);
  window.removeEventListener("sr-abandon-recording", abandon);
});

it("does not offer dismissal while recording files are still being written", async () => {
  vi.mocked(invoke).mockResolvedValue({ id: 8, error: "Slow", directory: "C:/recordings", paths: [], canRelease: false } as never);
  render(<RecordingRecoveryNotice />);
  expect(await screen.findByText("Waiting")).toBeTruthy();
  expect(screen.queryByText("Keep files")).toBeNull();
});

it("offers a visible retry after an incomplete stop", () => {
  vi.mocked(invoke).mockResolvedValue(null as never);
  const retry = vi.fn();
  window.addEventListener("sr-recover-recording", retry);
  render(<RecordingRecoveryNotice />);
  act(() => window.dispatchEvent(new Event("sr-recording-retry-required")));
  fireEvent.click(screen.getByText("Retry saving"));
  expect(retry).toHaveBeenCalledOnce();
  act(() => window.dispatchEvent(new Event("sr-recording-retry-cleared")));
  expect(screen.queryByText("Retry saving")).toBeNull();
  window.removeEventListener("sr-recover-recording", retry);
});
