import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBackgroundUpload, type BackgroundUploadTarget } from "@/hooks/useBackgroundUpload";
import { USER_NOTIFICATION_EVENT, type UserNotificationDetail } from "@/lib/userNotifications";
import { invoke } from "@/lib/ipc";
import { projectManager } from "@/lib/projectManager";
import type { BackgroundConfig } from "@/types/video";
import type { Project } from "@/types/video";
import { resolveBackgroundUploadTarget } from "@/lib/backgroundUploadTarget";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/projectManager", () => ({
  projectManager: { getManagedCustomBackgroundUrls: vi.fn() },
}));

class TestImage {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  naturalWidth = 100;
  naturalHeight = 100;
  src = "";
  constructor() { images.push(this); }
}

const images: TestImage[] = [];
const initialTarget: BackgroundUploadTarget = {
  projectId: "project-a", clipId: "clip-a", backgroundType: "solid", selectionRevision: 0, clipRevision: 0,
};
const config = { backgroundType: "solid" } as BackgroundConfig;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function startUpload(handle: (event: React.ChangeEvent<HTMLInputElement>) => void) {
  const input = document.createElement("input");
  const file = new File(["image"], "background.png", { type: "image/png" });
  Object.defineProperty(input, "files", { value: [file] });
  act(() => handle({ currentTarget: input, target: input } as React.ChangeEvent<HTMLInputElement>));
  act(() => { void images.at(-1)!.onload!(); });
}

beforeEach(() => {
  images.length = 0;
  localStorage.clear();
  vi.stubGlobal("Image", TestImage);
  vi.stubGlobal("URL", { createObjectURL: vi.fn(() => "blob:upload"), revokeObjectURL: vi.fn() });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage: vi.fn(), imageSmoothingEnabled: false, imageSmoothingQuality: "low",
  } as unknown as CanvasRenderingContext2D);
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/jpeg;base64,dGVzdA==");
  vi.mocked(projectManager.getManagedCustomBackgroundUrls).mockResolvedValue([]);
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "save_uploaded_bg_data_url" ? "background://saved" : undefined,
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("background upload ownership", () => {
  it("does not claim an editor while a selected clip is loading", () => {
    const project = { id: "project-a", composition: { selectedClipId: "clip-b", focusedClipId: "clip-b" } } as Project;
    const input = { project, loadedClipId: "clip-a", isTransitioning: false, background: config,
      selectionRevision: 0, clipRevision: 1 };
    expect(resolveBackgroundUploadTarget(input)).toBeNull();
    expect(resolveBackgroundUploadTarget({ ...input, loadedClipId: "clip-b", isTransitioning: true })).toBeNull();
    expect(resolveBackgroundUploadTarget({ ...input, loadedClipId: "clip-b" })?.clipId).toBe("clip-b");
  });

  it("keeps a late upload available without applying it to another clip", async () => {
    const saved = deferred<string>();
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "save_uploaded_bg_data_url" ? saved.promise : undefined,
    );
    let target: BackgroundUploadTarget | null = { ...initialTarget };
    const apply = vi.fn();
    const notice = vi.fn();
    window.addEventListener(USER_NOTIFICATION_EVENT, notice);
    const { result, unmount } = renderHook(() => useBackgroundUpload({
      backgroundConfig: config, composition: null, setBackgroundConfig: vi.fn(),
      getUploadTarget: () => target, applyUploadedBackground: apply,
    }));
    startUpload(result.current.handleBackgroundUpload);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_uploaded_bg_data_url", expect.anything()));
    target = { ...initialTarget, clipId: "clip-b" };
    await act(async () => saved.resolve("background://saved"));
    await waitFor(() => expect(result.current.recentUploads).toContain("background://saved"));
    expect(apply).not.toHaveBeenCalled();
    expect((notice.mock.calls.at(-1)?.[0] as CustomEvent<UserNotificationDetail>).detail.messageKey)
      .toBe("backgroundUploadTargetChanged");
    unmount();
    window.removeEventListener(USER_NOTIFICATION_EVENT, notice);
  });

  it("applies while a retention scan waits and retains the upload when the scan completes", async () => {
    const scan = deferred<string[]>();
    vi.mocked(projectManager.getManagedCustomBackgroundUrls).mockReturnValueOnce(scan.promise);
    const apply = vi.fn();
    const { result } = renderHook(() => useBackgroundUpload({
      backgroundConfig: config, composition: null, setBackgroundConfig: vi.fn(),
      getUploadTarget: () => initialTarget, applyUploadedBackground: apply,
    }));
    startUpload(result.current.handleBackgroundUpload);
    await waitFor(() => expect(apply).toHaveBeenCalledWith("background://saved"));
    await act(async () => scan.resolve([]));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("reconcile_uploaded_backgrounds", {
      retainedUrls: ["background://saved"],
    }));
  });

  it("waits for an already-dispatched retention operation before saving", async () => {
    const reconcile = deferred<void>();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "reconcile_uploaded_backgrounds") return reconcile.promise;
      if (command === "save_uploaded_bg_data_url") return "background://saved";
      return undefined;
    });
    const apply = vi.fn();
    const { result } = renderHook(() => useBackgroundUpload({
      backgroundConfig: config, composition: null, setBackgroundConfig: vi.fn(),
      getUploadTarget: () => initialTarget, applyUploadedBackground: apply,
    }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("reconcile_uploaded_backgrounds", expect.anything()));
    startUpload(result.current.handleBackgroundUpload);
    await act(async () => { await Promise.resolve(); });
    expect(invoke).not.toHaveBeenCalledWith("save_uploaded_bg_data_url", expect.anything());
    await act(async () => reconcile.resolve());
    await waitFor(() => expect(apply).toHaveBeenCalledWith("background://saved"));
  });

  it("does not overwrite a later selection even if its value was changed back", async () => {
    const saved = deferred<string>();
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "save_uploaded_bg_data_url" ? saved.promise : undefined,
    );
    let target = { ...initialTarget };
    const apply = vi.fn();
    const { result } = renderHook(() => useBackgroundUpload({
      backgroundConfig: config, composition: null, setBackgroundConfig: vi.fn(),
      getUploadTarget: () => target, applyUploadedBackground: apply,
    }));
    startUpload(result.current.handleBackgroundUpload);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_uploaded_bg_data_url", expect.anything()));
    target = { ...target, selectionRevision: 2 };
    await act(async () => saved.resolve("background://saved"));
    await waitFor(() => expect(result.current.recentUploads).toContain("background://saved"));
    expect(apply).not.toHaveBeenCalled();
  });

  it("reports a save failure and releases the processing indicator", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "save_uploaded_bg_data_url") throw new Error("storage unavailable");
      return undefined;
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const notice = vi.fn();
    window.addEventListener(USER_NOTIFICATION_EVENT, notice);
    const apply = vi.fn();
    const { result, unmount } = renderHook(() => useBackgroundUpload({
      backgroundConfig: config, composition: null, setBackgroundConfig: vi.fn(),
      getUploadTarget: () => initialTarget, applyUploadedBackground: apply,
    }));
    startUpload(result.current.handleBackgroundUpload);
    await waitFor(() => expect(notice).toHaveBeenCalled());
    expect((notice.mock.calls.at(-1)?.[0] as CustomEvent<UserNotificationDetail>).detail.messageKey)
      .toBe("backgroundUploadFailed");
    expect(result.current.isBackgroundUploadProcessing).toBe(false);
    expect(result.current.recentUploads).toEqual([]);
    expect(apply).not.toHaveBeenCalled();
    unmount();
    window.removeEventListener(USER_NOTIFICATION_EVENT, notice);
    consoleError.mockRestore();
  });
});
