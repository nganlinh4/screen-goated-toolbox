import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { STATUS_FETCH_RETRY_DELAYS_MS, useAsyncJobPoll } from "@/hooks/useAsyncJobPoll";

type Status = { done: boolean };

function setup(fetchStatus: (jobId: string) => Promise<Status>) {
  const onComplete = vi.fn();
  const onError = vi.fn();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  const hook = renderHook(() => useAsyncJobPoll<Status>({
    jobId: "job-1", fetchStatus, isTerminal: (status) => status.done,
    onComplete, onError, intervalFor: () => 100,
  }));
  return { ...hook, onComplete, onError };
}

describe("useAsyncJobPoll", () => {
  afterEach(() => { vi.useRealTimers(); });

  it("keeps tracking the job across transient status-fetch failures", async () => {
    vi.useFakeTimers();
    const fetchStatus = vi.fn<(jobId: string) => Promise<Status>>()
      .mockRejectedValueOnce(new Error("ipc hiccup"))
      .mockRejectedValueOnce(new Error("ipc hiccup"))
      .mockResolvedValueOnce({ done: false })
      .mockResolvedValueOnce({ done: true });
    const { onComplete, onError } = setup(fetchStatus);
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    expect(onError).not.toHaveBeenCalled();
    expect(onComplete).toHaveBeenCalledWith({ done: true });
    expect(fetchStatus).toHaveBeenCalledTimes(4);
  });

  it("reports the error once retries are exhausted and stops polling", async () => {
    vi.useFakeTimers();
    const fetchStatus = vi.fn<(jobId: string) => Promise<Status>>().mockRejectedValue(new Error("gone"));
    const { onError } = setup(fetchStatus);
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(onError).toHaveBeenCalledOnce();
    expect(fetchStatus).toHaveBeenCalledTimes(STATUS_FETCH_RETRY_DELAYS_MS.length + 1);
  });

  it("does not retry after unmount", async () => {
    vi.useFakeTimers();
    const fetchStatus = vi.fn<(jobId: string) => Promise<Status>>().mockRejectedValue(new Error("gone"));
    const { unmount, onError } = setup(fetchStatus);
    await act(async () => { await Promise.resolve(); });
    unmount();
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(fetchStatus).toHaveBeenCalledOnce();
    expect(onError).not.toHaveBeenCalled();
  });
});
