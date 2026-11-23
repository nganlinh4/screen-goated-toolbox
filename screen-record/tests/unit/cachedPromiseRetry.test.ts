import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn() }));

describe("cached startup promises retry after a rejection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("retries the media server lookup after a transient IPC failure", async () => {
    const { invoke } = await import("@/lib/ipc");
    const { getMediaServerInfo } = await import("@/lib/mediaServer");
    vi.mocked(invoke)
      .mockRejectedValueOnce(new Error("host not ready"))
      .mockResolvedValueOnce({ port: 4321, token: "t" });

    await expect(getMediaServerInfo()).rejects.toThrow("host not ready");
    await expect(getMediaServerInfo()).resolves.toEqual({ port: 4321, token: "t" });
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("reopens the project database after a failed open", async () => {
    const db = { name: "db" } as unknown as IDBDatabase;
    let attempt = 0;
    vi.stubGlobal("indexedDB", {
      open: () => {
        attempt += 1;
        const request = {} as IDBOpenDBRequest & { result: IDBDatabase; error: Error | null };
        const fail = attempt === 1;
        queueMicrotask(() => {
          if (fail) {
            request.error = new Error("blocked");
            request.onerror?.(new Event("error"));
          } else {
            request.result = db;
            request.onsuccess?.(new Event("success"));
          }
        });
        return request;
      },
    });
    const { openProjectDB } = await import("@/lib/projectStorage");

    await expect(openProjectDB()).rejects.toThrow("blocked");
    await expect(openProjectDB()).resolves.toBe(db);
    expect(attempt).toBe(2);
  });
});
