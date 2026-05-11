import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildAudioDownloadRequest,
  startAudioTrackDownload,
  type StartAudioDownloadOptions,
} from "@/lib/audioDownload";
import { invoke } from "@/lib/ipc";
import type {
  BackgroundConfig,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";

vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

function segment(overrides: Partial<VideoSegment> = {}): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    trimSegments: [{ id: "trim-a", startTime: 1, endTime: 6 }],
    zoomKeyframes: [],
    textSegments: [],
    speedPoints: [
      { time: 0, speed: 1 },
      { time: 10, speed: 1 },
    ],
    deviceAudioAvailable: true,
    micAudioAvailable: true,
    ...overrides,
  };
}

const background: BackgroundConfig = {
  scale: 1,
  borderRadius: 0,
  backgroundType: "solid",
};

function clip(id: string, overrides: Partial<ProjectCompositionClip> = {}): ProjectCompositionClip {
  return {
    id,
    role: id === "root" ? "root" : "snapshot",
    name: `Clip ${id}`,
    duration: 10,
    segment: segment(),
    backgroundConfig: background,
    mousePositions: [],
    rawVideoPath: `C:/clips/${id}.mp4`,
    rawMicAudioPath: `C:/clips/${id}-mic.wav`,
    ...overrides,
  };
}

function composition(overrides: Partial<ProjectComposition> = {}): ProjectComposition {
  return {
    mode: "separate",
    selectedClipId: "root",
    focusedClipId: "root",
    clips: [clip("root")],
    audioSegments: [],
    audioTrackVolumePoints: [],
    narrationSegments: [],
    narrationTrackVolumePoints: [],
    ...overrides,
  };
}

function options(overrides: Partial<StartAudioDownloadOptions> = {}): StartAudioDownloadOptions {
  return {
    trackKind: "device",
    format: "wav",
    outputDir: "C:/Users/user/Downloads",
    trackLabel: "Device Audio",
    segment: segment(),
    sourceVideoPath: "C:/recordings/root.mp4",
    micAudioPath: "C:/recordings/root-mic.wav",
    videoDuration: 10,
    composition: composition(),
    resolveClipSourcePath: async (clipJob) => clipJob.rawVideoPath ?? "",
    resolveClipMicAudioPath: async (clipJob) => clipJob.rawMicAudioPath ?? "",
    ...overrides,
  };
}

describe("audio download native request builder", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "probe_video_metadata") {
        return { duration: 20 };
      }
      if (command === "start_audio_download") {
        return { status: "success", path: "C:/out/audio.wav", format: "wav" };
      }
      throw new Error(`Unexpected command ${command}`);
    });
  });

  it("builds single-clip requests with trimmed duration and disabled device audio respected", async () => {
    const request = await buildAudioDownloadRequest(options({
      segment: segment({
        deviceAudioAvailable: false,
        trimSegments: [
          { id: "a", startTime: 2, endTime: 5 },
          { id: "b", startTime: 7, endTime: 9 },
        ],
      }),
      videoDuration: 12,
    }));

    expect(request.clips).toHaveLength(1);
    expect(request.clips[0]).toMatchObject({
      clipId: "root",
      sourceVideoPath: "C:/recordings/root.mp4",
      deviceAudioPath: "",
      micAudioPath: "C:/recordings/root-mic.wav",
      trimStart: 2,
      duration: 5,
    });
    expect(request.clips[0].segment.trimSegments).toEqual([
      { id: "a", startTime: 2, endTime: 5 },
      { id: "b", startTime: 7, endTime: 9 },
    ]);
  });

  it("preserves imported audio, narration, and track envelopes for adjusted whole-track downloads", async () => {
    const request = await buildAudioDownloadRequest(options({
      trackKind: "narration",
      trackLabel: "Narration",
      composition: composition({
        audioSegments: [{
          id: "audio-a",
          rawAudioPath: "C:/audio/bed.wav",
          name: "Music bed",
          duration: 15,
          startTime: 1.5,
          inPoint: 0.25,
          outPoint: 9.75,
          playbackRate: 0.8,
          addedAt: 1,
        }],
        audioTrackVolumePoints: [
          { time: 0, volume: 0.6 },
          { time: 10, volume: 0.25 },
        ],
        narrationSegments: [{
          id: "nar-a",
          rawAudioPath: "C:/tts/line.wav",
          name: "Line",
          duration: 3,
          startTime: 2,
          inPoint: 0,
          outPoint: 3,
          playbackRate: 1.4,
          addedAt: 2,
          sourceSubtitleId: "sub-a",
        }],
        narrationTrackVolumePoints: [
          { time: 0, volume: 1 },
          { time: 8, volume: 0.7 },
        ],
      }),
    }));

    expect(request.trackKind).toBe("narration");
    expect(request.audioSegments?.[0]).toMatchObject({ id: "audio-a", playbackRate: 0.8 });
    expect(request.audioTrackVolumePoints).toEqual([
      { time: 0, volume: 0.6 },
      { time: 10, volume: 0.25 },
    ]);
    expect(request.narrationSegments?.[0]).toMatchObject({
      id: "nar-a",
      playbackRate: 1.4,
      sourceSubtitleId: "sub-a",
    });
    expect(request.narrationTrackVolumePoints).toEqual([
      { time: 0, volume: 1 },
      { time: 8, volume: 0.7 },
    ]);
  });

  it("builds ordered multi-clip jobs from resolved source and mic paths", async () => {
    const request = await buildAudioDownloadRequest(options({
      videoDuration: 0,
      composition: composition({
        clips: [
          clip("a", {
            name: "First",
            segment: segment({
              trimSegments: [{ id: "a-trim", startTime: 2, endTime: 8 }],
            }),
          }),
          clip("b", {
            name: "Second",
            segment: segment({
              trimSegments: [
                { id: "b-trim-a", startTime: 0, endTime: 3 },
                { id: "b-trim-b", startTime: 4, endTime: 6 },
              ],
              deviceAudioAvailable: false,
            }),
          }),
        ],
      }),
      resolveClipSourcePath: async (clipJob) => `C:/resolved/${clipJob.id}.mp4`,
      resolveClipMicAudioPath: async (clipJob) => `C:/resolved/${clipJob.id}-mic.wav`,
    }));

    expect(request.clips.map((entry) => entry.clipId)).toEqual(["a", "b"]);
    expect(request.clips[0]).toMatchObject({
      clipName: "First",
      sourceVideoPath: "C:/resolved/a.mp4",
      micAudioPath: "C:/resolved/a-mic.wav",
      trimStart: 2,
      duration: 6,
    });
    expect(request.clips[1]).toMatchObject({
      clipName: "Second",
      sourceVideoPath: "C:/resolved/b.mp4",
      deviceAudioPath: "",
      micAudioPath: "C:/resolved/b-mic.wav",
      trimStart: 0,
      duration: 5,
    });
    expect(invokeMock).toHaveBeenCalledWith("probe_video_metadata", { path: "C:/resolved/a.mp4" });
    expect(invokeMock).toHaveBeenCalledWith("probe_video_metadata", { path: "C:/resolved/b.mp4" });
  });

  it("invokes the native command with the same sanitized request", async () => {
    await startAudioTrackDownload(options({
      composition: composition({
        audioSegments: [{
          id: "audio-null",
          rawAudioPath: "C:/audio/null.wav",
          name: "Null fields",
          duration: 2,
          startTime: 0,
          inPoint: 0,
          outPoint: 2,
          addedAt: 3,
          playbackRate: null,
        } as unknown as ProjectComposition["audioSegments"][number]],
      }),
    }));

    const nativeCall = invokeMock.mock.calls.find(([command]) => command === "start_audio_download");
    expect(nativeCall).toBeTruthy();
    const request = nativeCall?.[1] as { audioSegments?: Array<{ playbackRate?: unknown }> };
    expect(request.audioSegments?.[0]).not.toHaveProperty("playbackRate");
  });
});
