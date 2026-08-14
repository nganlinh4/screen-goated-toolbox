import { describe, expect, it, vi } from "vitest";
import {
  ExportCancellationGeneration,
  startAfterCancellablePreparation,
} from "@/lib/exportCancellation";

describe("export cancellation generation", () => {
  it("does not start a native export after cancellation during lazy preparation", async () => {
    let release!: (value: { kind: "prepared" }) => void;
    const preparation = new Promise<{ kind: "prepared" }>((resolve) => {
      release = resolve;
    });
    const cancellation = new ExportCancellationGeneration();
    const generation = cancellation.begin();
    const start = vi.fn(async () => "started");
    const resultPromise = startAfterCancellablePreparation(
      cancellation,
      generation,
      () => preparation,
      start,
    );

    cancellation.cancel();
    release({ kind: "prepared" });

    await expect(resultPromise).resolves.toEqual({ cancelled: true });
    expect(start).not.toHaveBeenCalled();
  });

  it("suppresses a completed result after a later cancellation", async () => {
    const cancellation = new ExportCancellationGeneration();
    const generation = cancellation.begin();
    let finish!: (value: string) => void;
    const started = new Promise<string>((resolve) => { finish = resolve; });
    const resultPromise = startAfterCancellablePreparation(
      cancellation,
      generation,
      async () => "prepared",
      async () => started,
    );
    await Promise.resolve();
    cancellation.cancel();
    finish("done");
    await expect(resultPromise).resolves.toEqual({ cancelled: true });
  });
});
