import { afterEach, describe, expect, it, vi } from "vitest";
import {
  addSubtitleAcrossTracks,
  collectSubtitleIdsForTranslation,
  deleteSubtitleIdsAcrossTracks,
  ensureTranslatedTrack,
  mergeSubtitleSelectionAcrossTracks,
  replaceAudioSubtitlesOnOriginalTrack,
  splitSubtitleAcrossTracks,
  updateSubtitleStylesAcrossTracks,
  updateSubtitleTextsOnActiveTrack,
  updateSubtitleTimingAcrossTracks,
} from "@/lib/subtitleTrackMutations";
import { ORIGINAL_SUBTITLE_TRACK_ID } from "@/lib/subtitleTracks";
import type { SubtitleSegment, SubtitleTrack, VideoSegment } from "@/types/video";

function subtitle(id: string, startTime: number, endTime: number, text: string): SubtitleSegment {
  return {
    id,
    startTime,
    endTime,
    text,
    style: {
      fontSize: 42,
      color: "#ffffff",
      x: 50,
      y: 80,
      background: { enabled: true, color: "#000000", opacity: 0.4, paddingX: 10, paddingY: 6, borderRadius: 8 },
    },
  };
}

function track(id: string, kind: SubtitleTrack["kind"], segments: SubtitleSegment[]): SubtitleTrack {
  return {
    id,
    kind,
    targetLanguage: kind === "translation" ? "vi" : null,
    slotLabel: kind === "translation" ? "A" : null,
    segments,
  };
}

function segment(activeTrackId = ORIGINAL_SUBTITLE_TRACK_ID): VideoSegment {
  const original = [
    subtitle("s1", 0, 1.5, "Hello world"),
    subtitle("s2", 2, 4, "Second line"),
    subtitle("s3", 5, 7, "Third line"),
  ];
  const translation = [
    subtitle("s1", 0, 1.5, "Xin chao"),
    subtitle("s2", 2, 4, "Dong hai"),
    subtitle("s3", 5, 7, "Dong ba"),
  ];
  return {
    trimStart: 0,
    trimEnd: 8,
    zoomKeyframes: [],
    textSegments: [],
    subtitleTracks: [
      track(ORIGINAL_SUBTITLE_TRACK_ID, "original", original),
      track("subtitle-track-translation-vi", "translation", translation),
    ],
    activeSubtitleView: { kind: "track", trackId: activeTrackId },
    subtitleSegments: activeTrackId === ORIGINAL_SUBTITLE_TRACK_ID ? original : translation,
  };
}

describe("subtitle track mutations", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("edits text only on the active track but propagates style and timing edits across tracks", () => {
    const textEdited = updateSubtitleTextsOnActiveTrack(
      segment("subtitle-track-translation-vi"),
      new Set(["s2"]),
      (entry) => ({ ...entry, text: "Ban dich moi" }),
    );

    const originalAfterText = textEdited.subtitleTracks?.find((entry) => entry.id === ORIGINAL_SUBTITLE_TRACK_ID);
    const translationAfterText = textEdited.subtitleTracks?.find((entry) => entry.id === "subtitle-track-translation-vi");
    expect(originalAfterText?.segments.find((entry) => entry.id === "s2")?.text).toBe("Second line");
    expect(translationAfterText?.segments.find((entry) => entry.id === "s2")?.text).toBe("Ban dich moi");

    const styleEdited = updateSubtitleStylesAcrossTracks(textEdited, new Set(["s2"]), (entry) => ({
      ...entry,
      style: { ...entry.style, color: "#00ff00", fontSize: 64 },
    }));
    for (const subtitleTrack of styleEdited.subtitleTracks ?? []) {
      const edited = subtitleTrack.segments.find((entry) => entry.id === "s2");
      expect(edited?.text).toMatch(/Second line|Ban dich moi/);
      expect(edited?.style).toMatchObject({ color: "#00ff00", fontSize: 64 });
    }

    const timingEdited = updateSubtitleTimingAcrossTracks(styleEdited, "s2", (entry) => ({
      ...entry,
      startTime: 2.5,
      endTime: 4.5,
      text: "Should not replace text",
    }));
    for (const subtitleTrack of timingEdited.subtitleTracks ?? []) {
      const edited = subtitleTrack.segments.find((entry) => entry.id === "s2");
      expect(edited).toMatchObject({ startTime: 2.5, endTime: 4.5 });
      expect(edited?.text).not.toBe("Should not replace text");
    }
  });

  it("adds, deletes, splits, and merges subtitles across all concrete tracks", () => {
    vi.spyOn(crypto, "randomUUID").mockReturnValue("split-generated-id");

    const added = addSubtitleAcrossTracks(segment(), subtitle("s4", 7.1, 7.8, "Tail"));
    expect(added.subtitleTracks?.every((subtitleTrack) => subtitleTrack.segments.some((entry) => entry.id === "s4"))).toBe(true);

    const deleted = deleteSubtitleIdsAcrossTracks(added, ["s1"]);
    expect(deleted.subtitleTracks?.every((subtitleTrack) => !subtitleTrack.segments.some((entry) => entry.id === "s1"))).toBe(true);

    const split = splitSubtitleAcrossTracks(deleted, "s2", 3);
    expect(split.newSubtitleId).toBe("split-generated-id");
    for (const subtitleTrack of split.segment.subtitleTracks ?? []) {
      expect(subtitleTrack.segments.some((entry) => entry.id === "s2" && entry.endTime < 3)).toBe(true);
      expect(subtitleTrack.segments.some((entry) => entry.id === "split-generated-id" && entry.startTime > 3)).toBe(true);
    }

    const merged = mergeSubtitleSelectionAcrossTracks(split.segment, { startTime: 2, endTime: 4 });
    expect(merged.mergedId).toBeTruthy();
    for (const subtitleTrack of merged.segment.subtitleTracks ?? []) {
      const inRange = subtitleTrack.segments.filter((entry) => entry.startTime < 4 && entry.endTime > 2);
      expect(inRange).toHaveLength(1);
    }
  });

  it("replaces generated audio subtitles on the original track without touching translations", () => {
    const base = segment();
    const withAudioSources: VideoSegment = {
      ...base,
      subtitleTracks: base.subtitleTracks?.map((subtitleTrack) =>
        subtitleTrack.id === ORIGINAL_SUBTITLE_TRACK_ID
          ? {
              ...subtitleTrack,
              segments: subtitleTrack.segments.map((entry) =>
                entry.id === "s2"
                  ? {
                      ...entry,
                      sourceGroup: { kind: "audio", audioSegmentId: "audio-a" },
                      provenance: {
                        sourceKind: "audio",
                        audioSegmentId: "audio-a",
                        sourceName: "Audio",
                        sourcePath: "C:/audio.wav",
                        sourceLocalStartTime: 0,
                        sourceLocalEndTime: 2,
                      },
                    }
                  : entry,
              ),
            }
          : subtitleTrack,
      ),
    };

    const replaced = replaceAudioSubtitlesOnOriginalTrack(
      withAudioSources,
      new Set(["audio-a"]),
      [{ startTime: 1.8, endTime: 4.2 }],
      [subtitle("replacement", 2, 4, "Fresh transcript")],
    );
    const original = replaced.subtitleTracks?.find((subtitleTrack) => subtitleTrack.id === ORIGINAL_SUBTITLE_TRACK_ID);
    const translation = replaced.subtitleTracks?.find((subtitleTrack) => subtitleTrack.id === "subtitle-track-translation-vi");
    expect(original?.segments.some((entry) => entry.id === "s2")).toBe(false);
    expect(original?.segments.some((entry) => entry.id === "replacement")).toBe(true);
    expect(translation?.segments.some((entry) => entry.id === "s2")).toBe(true);
  });

  it("creates translation tracks and resolves requested ids deterministically", () => {
    const created = ensureTranslatedTrack(segment(), "ja", null, "B");
    expect(created.track).toMatchObject({
      id: "subtitle-track-translation-ja",
      kind: "translation",
      targetLanguage: "ja",
      slotLabel: "B",
    });
    expect(created.segment.subtitleTracks?.some((entry) => entry.id === created.track.id)).toBe(true);
    expect(collectSubtitleIdsForTranslation(created.segment, [], null)).toEqual(["s1", "s2", "s3"]);
    expect(collectSubtitleIdsForTranslation(created.segment, ["s2"], "s3")).toEqual(["s2"]);
    expect(collectSubtitleIdsForTranslation(created.segment, [], "s3")).toEqual(["s3"]);
  });
});
