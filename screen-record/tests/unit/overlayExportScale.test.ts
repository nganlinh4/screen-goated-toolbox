import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { bakeOverlayAtlasAndPaths } from "@/lib/renderer/overlayBaker";
import type { KeystrokeState } from "@/lib/renderer/keystrokeTypes";
import {
  resolveRenderScale,
  scaleBackgroundFrameForRender,
} from "@/lib/renderer/renderScale";
import {
  DEFAULT_WEBCAM_CONFIG,
  buildBakedWebcamFrames,
  resolveWebcamLayoutRect,
} from "@/lib/webcam";
import type { BackgroundConfig, TextSegment, VideoSegment } from "@/types/video";

// The editor previews this project on a 2560x1440 canvas.
const PREVIEW = { width: 2560, height: 1440 };
const OUTPUTS = [
  { width: 1280, height: 720 },
  { width: 2560, height: 1440 },
  { width: 3840, height: 2160 },
];

type TextDraw = { text: string; fontPx: number; shadowBlur: number; lineWidth: number };

/** A 2D context whose text metrics scale with the font size, recording text draws. */
function installMeasuringCanvas(draws: TextDraw[]) {
  const fontPx = (font: unknown) => Number(/([\d.]+)px/.exec(String(font))?.[1] ?? 10);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function () {
    const state: Record<string, unknown> = {
      font: "10px sans-serif", shadowBlur: 0, lineWidth: 1, globalAlpha: 1,
      canvas: { style: {} },
    };
    return new Proxy(state, {
      get(target, key: string) {
        if (key === "measureText") {
          return (text: string) => ({ width: Array.from(text).length * fontPx(target.font) * 0.5 });
        }
        if (key === "fillText") {
          return (text: string) => draws.push({
            text,
            fontPx: fontPx(target.font),
            shadowBlur: target.shadowBlur as number,
            lineWidth: target.lineWidth as number,
          });
        }
        if (key in target) return target[key];
        return () => {};
      },
      set(target, key: string, value) {
        target[key] = value;
        return true;
      },
    }) as unknown as CanvasRenderingContext2D;
  } as typeof HTMLCanvasElement.prototype.getContext);
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,AA==");
}

function keystrokeState(): KeystrokeState {
  return {
    keystrokeLanguage: "en",
    renderCache: {
      mode: "off", segmentRef: null, eventsRef: null, visibilityRef: null, duration: 0,
      displayEvents: [], startTimes: [], effectiveEnds: [], keyboardStartTimes: [],
      keyboardIndices: [], mouseStartTimes: [], mouseIndices: [], keyboardMaxDuration: 0,
      mouseMaxDuration: 0, eventSlots: [], eventIdentities: [], keyboardSlotRepresentatives: [],
      mouseSlotRepresentatives: [],
    },
    layoutCache: new Map(),
  } as unknown as KeystrokeState;
}

const caption: TextSegment = {
  id: "caption",
  startTime: 0,
  endTime: 1,
  text: "Caption",
  style: {
    fontSize: 48,
    color: "#ffffff",
    x: 50,
    y: 80,
    shadow: { enabled: true, color: "#000000", blur: 8, offsetX: 2, offsetY: 2, opacity: 1 },
    background: { enabled: true, color: "#000000", opacity: 0.6, paddingX: 16, paddingY: 8, borderRadius: 32 },
  },
};

const segment = {
  trimStart: 0,
  trimEnd: 1,
  trimSegments: [{ id: "trim", startTime: 0, endTime: 1 }],
  textSegments: [caption],
  subtitleSegments: [],
  keystrokeMode: "keyboard",
  keystrokeEvents: [{
    id: "key", type: "keyboard", startTime: 0.2, endTime: 0.6, label: "A", count: 1, modifiers: {},
  }],
  keyboardVisibilitySegments: [{ id: "visible", startTime: 0, endTime: 1 }],
} as unknown as VideoSegment;

describe("export overlay scaling", () => {
  let draws: TextDraw[];
  beforeEach(() => {
    draws = [];
    installMeasuringCanvas(draws);
  });
  afterEach(() => vi.restoreAllMocks());

  it("bakes text and keystroke pills at the preview size relative to every output frame", async () => {
    const results = [];
    for (const output of OUTPUTS) {
      draws.length = 0;
      const payload = await bakeOverlayAtlasAndPaths(
        segment, output.width, output.height, PREVIEW, 60, keystrokeState(),
      );
      const metadata = payload!.atlasMetadata as {
        renderScale: number;
        textEntries: Array<{ pivotX: number; pivotY: number }>;
        keystrokeEntries: Array<{ layoutFontSize: number; layoutMarginBottom: number; layoutHeight: number }>;
      };
      const captionDraw = draws.find((draw) => draw.text === "Caption")!;
      results.push({ output, metadata, captionDraw });
    }

    // Keystroke pills: 38px on the 1440p preview (the clamp is applied on the preview canvas).
    // The native exporter scales its fixed floors and animation offsets by this.
    expect(results.map((r) => r.metadata.renderScale)).toEqual([0.5, 1, 1.5]);
    expect(results.map((r) => r.metadata.keystrokeEntries[0].layoutFontSize)).toEqual([19, 38, 57]);
    expect(results.map((r) => r.metadata.keystrokeEntries[0].layoutMarginBottom)).toEqual([43, 86, 130]);
    // Text: font and shadow are authored in preview pixels and scale with the output.
    expect(results.map((r) => r.captionDraw.fontPx)).toEqual([24, 48, 72]);
    expect(results.map((r) => r.captionDraw.shadowBlur)).toEqual([4, 8, 12]);
    for (const { output, metadata } of results) {
      expect(metadata.textEntries[0].pivotX / output.width).toBeCloseTo(0.5, 6);
      expect(metadata.textEntries[0].pivotY / output.height).toBeCloseTo(0.8, 6);
    }
  });

  it("keeps an exported webcam inset, rounding and shadow proportional to the preview", () => {
    const config = { ...DEFAULT_WEBCAM_CONFIG, visible: true };
    const preview = resolveWebcamLayoutRect(config, PREVIEW.width, PREVIEW.height, 16 / 9, 1);
    for (const output of OUTPUTS) {
      const [frame] = buildBakedWebcamFrames(
        segment, config, output.width, output.height, 16 / 9, () => 1, 60,
        resolveRenderScale(output.height, PREVIEW),
      );
      const ratio = output.height / PREVIEW.height;
      expect(frame.x).toBeCloseTo(preview.x * ratio, 2);
      expect(frame.y).toBeCloseTo(preview.y * ratio, 2);
      expect(frame.width).toBeCloseTo(preview.width * ratio, 2);
      expect(frame.roundnessPx).toBeCloseTo(preview.roundnessPx * ratio, 2);
      expect(frame.shadowPx).toBeCloseTo(preview.shadowPx * ratio, 2);
      expect(frame.renderScale).toBeCloseTo(ratio, 6);
    }
  });

  it("maps the frame corner radius and shadow to output pixels", () => {
    const config = { scale: 90, borderRadius: 24, shadow: 40, backgroundType: "solid" } as BackgroundConfig;
    expect(scaleBackgroundFrameForRender(config, resolveRenderScale(2160, PREVIEW))).toMatchObject({
      borderRadius: 36,
      shadow: 60,
      scale: 90,
    });
    expect(scaleBackgroundFrameForRender(config, resolveRenderScale(1440, PREVIEW))).toBe(config);
  });
});
