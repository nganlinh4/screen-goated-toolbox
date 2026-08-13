import { beforeEach, describe, expect, it, vi } from "vitest";
import { buildFrameFileName, saveCurrentFrame } from "@/lib/frameDownload";
import { invoke } from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn(async () => ({ savedPath: "C:\\Users\\user\\Downloads\\frame.png" })),
}));

describe("frame download", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it("builds a safe, timestamped PNG name", () => {
    expect(buildFrameFileName("Demo: Take?", 62.345))
      .toBe("Demo- Take--frame-00-01-02-345.png");
  });

  it("sends the intrinsic composed canvas as PNG to the native save command", async () => {
    const canvas = document.createElement("canvas");
    canvas.width = 1920;
    canvas.height = 1080;
    canvas.toBlob = vi.fn((callback) => {
      callback(new Blob(["png-frame"], { type: "image/png" }));
    });

    const savedPath = await saveCurrentFrame({
      canvas,
      currentTime: 2.5,
      notificationTitle: "Frame saved",
      projectName: "Demo",
    });

    expect(savedPath).toBe("C:\\Users\\user\\Downloads\\frame.png");
    expect(invoke).toHaveBeenCalledWith("save_current_frame", {
      dataUrl: expect.stringMatching(/^data:image\/png;base64,/),
      defaultFileName: "Demo-frame-00-00-02-500.png",
      notificationTitle: "Frame saved",
    });
  });

  it("rejects an empty canvas before invoking the host", async () => {
    const canvas = document.createElement("canvas");
    canvas.width = 0;
    canvas.height = 0;
    await expect(saveCurrentFrame({
      canvas,
      currentTime: 0,
      notificationTitle: "Frame saved",
    })).rejects.toThrow("preview frame is empty");
    expect(invoke).not.toHaveBeenCalled();
  });
});
