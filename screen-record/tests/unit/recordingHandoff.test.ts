import { describe, expect, it, vi } from "vitest";
import { RecordingHandoff, waitForRecordingAck } from "@/lib/recordingHandoff";

describe("recording shutdown handoff", () => {
  it("waits for an in-flight start before saving and blocks new starts", async () => {
    const handoff = new RecordingHandoff();
    expect(handoff.beginStart()).toBe(true);
    const save = vi.fn(async () => handoff.stop(async () => true));
    const pending = handoff.prepare(save);
    expect(handoff.beginStart()).toBe(false);
    expect(save).not.toHaveBeenCalled();
    handoff.finishStart(true);
    expect(await pending).toBe(true);
    expect(save).toHaveBeenCalledOnce();
    expect(handoff.beginStart()).toBe(false);
  });

  it("awaits the same in-flight save and allows retry after failure", async () => {
    const handoff = new RecordingHandoff();
    expect(handoff.beginStart()).toBe(true);
    handoff.finishStart(true);
    let resolve!: (saved: boolean) => void;
    const save = vi.fn(() => new Promise<boolean>((done) => { resolve = done; }));
    const first = handoff.stop(save);
    const preparing = handoff.prepare(() => handoff.stop(save));
    expect(save).toHaveBeenCalledOnce();
    resolve(false);
    expect(await first).toBe(false);
    expect(await preparing).toBe(false);
    expect(await handoff.prepare(() => handoff.stop(async () => true))).toBe(true);
  });

  it("allows normal work again after a canceled shutdown", async () => {
    const handoff = new RecordingHandoff();
    expect(await handoff.prepare(async () => false)).toBe(true);
    expect(handoff.beginStart()).toBe(false);
    handoff.cancelPrepare();
    expect(handoff.beginStart()).toBe(true);
    handoff.finishStart(false);
  });

  it("retains a recovered recording when an overlapping start fails", async () => {
    const handoff = new RecordingHandoff();
    expect(handoff.beginStart()).toBe(true);
    handoff.restorePending();
    handoff.finishStart(false);
    const save = vi.fn(async () => handoff.stop(async () => true));
    expect(await handoff.prepare(save)).toBe(true);
    expect(save).toHaveBeenCalledOnce();
  });

  it("bounds an acknowledgement wait so a failed save can be retried", async () => {
    await expect(waitForRecordingAck(new Promise(() => {}), 1)).rejects.toThrow("timed out");
  });

  it("blocks new recordings until a retained-file release is acknowledged", () => {
    const handoff = new RecordingHandoff();
    expect(handoff.beginRelease()).toBe(true);
    handoff.finishRelease(false);
    expect(handoff.beginStart()).toBe(false);
    expect(handoff.beginRelease()).toBe(true);
    handoff.finishRelease(true);
    expect(handoff.beginStart()).toBe(true);
  });
});
