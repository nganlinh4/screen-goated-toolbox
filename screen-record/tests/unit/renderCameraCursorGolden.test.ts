import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  calculateCurrentZoomStateInternal,
  zoomBlockEnvelope,
} from "@/lib/renderer/cameraZoom";
import {
  catmullRomInterpolate,
  normalizeAngleRad,
  lerpAngleRad,
  smoothDampScalar,
  springStepScalar,
  springStepAngle,
  smoothMousePositions,
} from "@/lib/renderer/cursorDynamics";
import type {
  BackgroundConfig,
  MousePosition,
  VideoSegment,
  ZoomBlock,
} from "@/types/video";

// The TS preview is the CANONICAL source of the render math. This test proves
// the committed cross-language golden was produced by the current TS code; the
// Rust export asserts the same fixture within the same tolerance. If the TS math
// changes intentionally, regenerate with tests/unit/_generateRenderGolden.gen.ts.
// See screen-record/docs/render-parity.md.
const FIXTURE = JSON.parse(
  readFileSync(
    path.resolve(__dirname, "../../../parity-fixtures/render-camera-cursor/golden.json"),
    "utf8",
  ),
);

const TOL = FIXTURE.tolerance as number;

describe("render-camera-cursor golden (TS canonical side)", () => {
  it("uses a tolerance of 1e-6", () => {
    expect(TOL).toBe(1e-6);
  });

  describe("camera path", () => {
    for (const c of FIXTURE.camera.cases as any[]) {
      it(`reproduces case "${c.name}"`, () => {
        const seg = fixtureSegmentToTs(c.segment);
        for (const s of c.samples) {
          const state = calculateCurrentZoomStateInternal(s.t, seg, c.view, c.view);
          expect(state.zoomFactor).toBeCloseToAbs(s.zoom, TOL);
          expect(state.positionX).toBeCloseToAbs(s.posX, TOL);
          expect(state.positionY).toBeCloseToAbs(s.posY, TOL);
        }
      });
    }

    it("reproduces zoomBlockEnvelope samples", () => {
      const env = FIXTURE.camera.zoomBlockEnvelope;
      const block = env.block as ZoomBlock;
      for (const s of env.samples) {
        expect(zoomBlockEnvelope(block, s.t)).toBeCloseToAbs(s.value, TOL);
      }
    });
  });

  describe("cursor primitives", () => {
    it("reproduces catmullRom samples", () => {
      for (const c of FIXTURE.cursorPrimitives.catmullRom) {
        expect(catmullRomInterpolate(c.p0, c.p1, c.p2, c.p3, c.t)).toBeCloseToAbs(c.value, TOL);
      }
    });

    it("reproduces normalizeAngle samples", () => {
      for (const c of FIXTURE.cursorPrimitives.normalizeAngle) {
        expect(normalizeAngleRad(c.angle)).toBeCloseToAbs(c.value, TOL);
      }
    });

    it("reproduces lerpAngle samples", () => {
      for (const c of FIXTURE.cursorPrimitives.lerpAngle) {
        expect(lerpAngleRad(c.from, c.to, c.t)).toBeCloseToAbs(c.value, TOL);
      }
    });

    it("reproduces smoothDampScalar trajectories (incl. overshoot clamp)", () => {
      for (const run of [
        FIXTURE.cursorPrimitives.smoothDampScalar.settle,
        FIXTURE.cursorPrimitives.smoothDampScalar.overshootClamp,
      ]) {
        let value = run.start;
        let velocity = run.initialVelocity;
        for (const step of run.steps) {
          const r = smoothDampScalar(value, run.target, velocity, run.smoothTime, run.maxSpeed, run.dt);
          value = r.value;
          velocity = r.velocity;
          expect(value).toBeCloseToAbs(step.value, TOL);
          expect(velocity).toBeCloseToAbs(step.velocity, TOL);
        }
      }
    });

    it("reproduces springStepScalar trajectories", () => {
      for (const cfg of FIXTURE.cursorPrimitives.springStepScalar) {
        let value = 0;
        let velocity = 0;
        for (const step of cfg.trajectory) {
          const r = springStepScalar(value, cfg.target, velocity, cfg.omega, cfg.zeta, cfg.dt);
          value = r.value;
          velocity = r.velocity;
          expect(value).toBeCloseToAbs(step.value, TOL);
          expect(velocity).toBeCloseToAbs(step.velocity, TOL);
        }
      }
    });

    it("reproduces springStepAngle trajectory across the seam", () => {
      const cfg = FIXTURE.cursorPrimitives.springStepAngle;
      let value = cfg.from;
      let velocity = 0;
      for (const step of cfg.trajectory) {
        const r = springStepAngle(value, cfg.target, velocity, cfg.omega, cfg.zeta, cfg.dt);
        value = r.value;
        velocity = r.velocity;
        expect(value).toBeCloseToAbs(step.value, TOL);
        expect(velocity).toBeCloseToAbs(step.velocity, TOL);
      }
    });

    it("reproduces smoothMousePositions output (box blur + 60-frame clamp + idle-skip)", () => {
      const sm = FIXTURE.cursorPrimitives.smoothMousePositions;
      for (const track of sm.tracks as any[]) {
        const input = track.input as MousePosition[];
        for (const c of track.cases) {
          const bg = { cursorSmoothness: c.smoothness } as unknown as BackgroundConfig;
          const out = smoothMousePositions(
            input.map((p) => ({ ...p })),
            120,
            bg,
          );
          expect(out.length).toBe(c.output.length);
          for (let i = 0; i < c.output.length; i++) {
            const e = c.output[i];
            expect(out[i].x).toBeCloseToAbs(e.x, TOL);
            expect(out[i].y).toBeCloseToAbs(e.y, TOL);
            expect(out[i].timestamp).toBeCloseToAbs(e.timestamp, TOL);
            expect(Boolean(out[i].isClicked)).toBe(e.isClicked);
            expect(out[i].cursor_type ?? "default").toBe(e.cursor_type);
          }
        }
      }
    });
  });
});

// Rebuild a TS VideoSegment from the fixture's minimal segment JSON. The fixture
// uses the same camelCase wire fields the Rust deserializer reads.
function fixtureSegmentToTs(seg: any): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    zoomKeyframes: [],
    textSegments: [],
    crop: seg.crop ?? undefined,
    zoomBlocks: (seg.zoomBlocks ?? []).map((b: any, i: number) => ({
      id: `zb-${i}`,
      startTime: b.startTime,
      endTime: b.endTime,
      easeIn: b.easeIn,
      easeOut: b.easeOut,
      zoomFactor: b.zoomFactor,
      positionX: b.positionX,
      positionY: b.positionY,
      followCursor: !!b.followCursor,
      directTransitionToNext: !!b.directTransitionToNext,
      enabled: b.enabled !== false,
    })),
    zoomInfluencePoints: seg.zoomInfluencePoints ?? [],
    smoothMotionPath: seg.smoothMotionPath ?? [],
  } as unknown as VideoSegment;
}

// Absolute-tolerance comparison matcher (vitest's toBeCloseTo is decimal-digit
// based; we want a flat 1e-6 envelope matching the Rust assertion).
expect.extend({
  toBeCloseToAbs(received: number, expected: number, tol: number) {
    const pass = Math.abs(received - expected) <= tol;
    return {
      pass,
      message: () =>
        `expected ${received} to be within ${tol} of ${expected} (delta ${Math.abs(received - expected)})`,
    };
  },
});

declare module "vitest" {
  interface Assertion<T = any> {
    toBeCloseToAbs(expected: number, tol: number): T;
  }
}
