import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@/lib/ipc";
import {
  EXPORT_FRAME_CHUNK_SIZE,
  stageFrameIterableInChunks,
  stageOverlayPayload,
} from "@/lib/exportStaging";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

describe("stageOverlayPayload", () => {
  beforeEach(() => invokeMock.mockResolvedValue(undefined));

  it("stages atlas metadata for a composition-scoped export", async () => {
    await stageOverlayPayload({
      atlasBase64: "data:image/png;base64,AA==",
      atlasWidth: 128,
      atlasHeight: 64,
      frames: [],
      atlasMetadata: { textEntries: [{ id: "caption" }] },
    }, { sessionId: "session", jobId: "clip" });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "stage_export_data", {
      sessionId: "session",
      jobId: "clip",
      dataType: "atlas",
      base64: "data:image/png;base64,AA==",
      width: 128,
      height: 64,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "stage_export_data", {
      sessionId: "session",
      jobId: "clip",
      dataType: "overlay_atlas_metadata",
      data: { textEntries: [{ id: "caption" }] },
    });
  });

  it("keeps the legacy frame fallback for metadata-free payloads", async () => {
    await stageOverlayPayload({
      atlasBase64: "atlas",
      atlasWidth: 1,
      atlasHeight: 1,
      frames: [{ frameIndex: 0, quads: [] }],
      atlasMetadata: null,
    });

    expect(invokeMock).toHaveBeenLastCalledWith("stage_export_data", {
      dataType: "overlay_frames_chunk",
      data: [{ frameIndex: 0, quads: [] }],
    });
  });

  it("consumes generated frames in bounded chunks", async () => {
    function* frames() {
      for (let index = 0; index < EXPORT_FRAME_CHUNK_SIZE + 2; index += 1) {
        yield { index };
      }
    }

    await stageFrameIterableInChunks(frames(), "webcam", {
      sessionId: "session",
    });

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock.mock.calls[0][1]).toMatchObject({
      dataType: "webcam",
      sessionId: "session",
    });
    expect(
      (invokeMock.mock.calls[0][1] as { data: unknown[] }).data,
    ).toHaveLength(EXPORT_FRAME_CHUNK_SIZE);
    expect(
      (invokeMock.mock.calls[1][1] as { data: unknown[] }).data,
    ).toHaveLength(2);
  });
});
