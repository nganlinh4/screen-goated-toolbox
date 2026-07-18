/**
 * One-off golden generator (NOT a permanent test — filename excluded from the
 * vitest glob by the `.gen.ts` suffix; run manually with
 * `npx vitest run --config vitest.gen.config.ts`). Emits the cross-language
 * render-camera-cursor parity fixture from the CANONICAL TypeScript/preview math.
 *
 * The committed fixture is then asserted from BOTH languages:
 *   - vitest: tests/unit/renderCameraCursorGolden.test.ts (against cameraZoom.ts / cursorDynamics.ts)
 *   - Rust:   #[test] in camera_path.rs + cursor_path/spring.rs/processing.rs
 *
 * Re-run only when the canonical TS math intentionally changes; never hand-edit
 * the fixture numbers.
 */
import { writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { describe, it } from "vitest";
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
import type { BackgroundConfig, MousePosition, VideoSegment } from "@/types/video";

const FIXTURE_DIR = path.resolve(
  __dirname,
  "../../../parity-fixtures/render-camera-cursor",
);

// Mirrors the Rust generate_camera_path SOURCE-time sampler: contain-fit is an
// identity here (canvas dims == cropped source dims), so posX/posY land in [0,1]
// anchor space exactly like the Rust calculate_zoom_state path.
function sampleCameraState(t: number, seg: VideoSegment, view: number) {
  const s = calculateCurrentZoomStateInternal(t, seg, view, view);
  return {
    zoom: round(s.zoomFactor),
    posX: round(s.positionX),
    posY: round(s.positionY),
  };
}

function round(v: number): number {
  // Keep full f64 precision; both sides assert within 1e-6.
  return v;
}

const VIEW = 1000;

function autoSegment(over: Partial<VideoSegment>): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    zoomKeyframes: [],
    textSegments: [],
    smoothMotionPath: [
      { time: 0, x: VIEW / 2, y: VIEW / 2, zoom: 2.0 },
      { time: 10, x: VIEW / 2, y: VIEW / 2, zoom: 2.0 },
    ],
    ...over,
  } as VideoSegment;
}

const blk = (over: Record<string, unknown>) => ({
  id: "zb",
  startTime: 0,
  endTime: 1,
  easeIn: 0.4,
  easeOut: 0.4,
  zoomFactor: 1.5,
  positionX: 0.5,
  positionY: 0.5,
  followCursor: false,
  enabled: true,
  ...over,
});

// Representative raw cursor track for the smoothMousePositions golden. Wire-format
// fields match the Rust MousePosition deserializer (isClicked / cursor_type). It
// mixes a fast diagonal sweep (>2px steps -> dense Catmull-Rom interp), a 0.6s
// segment that trips the 60-frame clamp, a click toggle, and a cursor_type change
// — covering the blur kernel and frame-clamp paths. Crucially the only sub-2px pair
// (idx 4->5, dist ~1.41) ALSO toggles isClicked, so the idle-skip does NOT fire
// here: every window takes the dense Catmull-Rom interp path. The dedicated
// STATIC_DWELL_INPUT below covers the idle-skip branch. This is the "above
// threshold / dense interp" golden case for smoothMousePositions.
const SMOOTH_MOUSE_INPUT: MousePosition[] = [
  { x: 100, y: 100, timestamp: 0.0, isClicked: false, cursor_type: "default" },
  { x: 180, y: 140, timestamp: 0.1, isClicked: false, cursor_type: "default" },
  { x: 320, y: 260, timestamp: 0.25, isClicked: false, cursor_type: "default" },
  { x: 500, y: 300, timestamp: 0.45, isClicked: true, cursor_type: "default" },
  { x: 540, y: 305, timestamp: 1.05, isClicked: true, cursor_type: "pointer" },
  { x: 541, y: 306, timestamp: 1.15, isClicked: false, cursor_type: "pointer" },
  { x: 600, y: 420, timestamp: 1.35, isClicked: false, cursor_type: "pointer" },
  { x: 720, y: 600, timestamp: 1.6, isClicked: false, cursor_type: "default" },
  { x: 760, y: 660, timestamp: 1.8, isClicked: false, cursor_type: "default" },
];

// STATIC-DWELL track that exercises the <2px idle-skip branch of
// smoothMousePositions (cursorDynamics.ts: `if (dist < 2 && p1.isClicked ===
// p2.isClicked && p1.cursor_type === p2.cursor_type) { push {...p1}; continue; }`).
// The cursor dwells around (400,300) with sub-2px jitter, so most windows hit the
// idle-skip (single p1 copy, no dense interp). Deliberate counter-cases prove the
// AND guard: idx 2->3 jitters <2px but TOGGLES isClicked (skip must NOT fire), and
// idx 5->6 jitters <2px but CHANGES cursor_type (skip must NOT fire). The closing
// pair makes a >2px move so a normal interpolated segment also appears. Locked
// cross-language within 1e-6 so the Rust idle-skip port matches the preview exactly.
const STATIC_DWELL_INPUT: MousePosition[] = [
  { x: 400.0, y: 300.0, timestamp: 0.0, isClicked: false, cursor_type: "default" },
  { x: 400.4, y: 300.3, timestamp: 0.1, isClicked: false, cursor_type: "default" },
  { x: 401.0, y: 300.8, timestamp: 0.2, isClicked: false, cursor_type: "default" },
  { x: 401.6, y: 301.2, timestamp: 0.3, isClicked: true, cursor_type: "default" },  // <2px from prev but click toggles -> no skip
  { x: 402.1, y: 301.5, timestamp: 0.4, isClicked: true, cursor_type: "default" },  // <2px, same state -> skip fires
  { x: 402.4, y: 301.8, timestamp: 0.5, isClicked: true, cursor_type: "default" },  // <2px, same state -> skip fires
  { x: 402.9, y: 302.1, timestamp: 0.6, isClicked: true, cursor_type: "pointer" },  // <2px but cursor_type changes -> no skip
  { x: 403.2, y: 302.4, timestamp: 0.7, isClicked: true, cursor_type: "pointer" },  // <2px, same state -> skip fires
  { x: 460.0, y: 360.0, timestamp: 0.8, isClicked: true, cursor_type: "pointer" },  // >2px move -> dense interp
  { x: 470.0, y: 372.0, timestamp: 0.9, isClicked: true, cursor_type: "pointer" },
  { x: 475.0, y: 378.0, timestamp: 1.0, isClicked: true, cursor_type: "pointer" },
];

describe("generate render-camera-cursor golden", () => {
  it("writes the fixture", () => {
    mkdirSync(FIXTURE_DIR, { recursive: true });

    // ---- CAMERA cases: clean dual-language golden ----
    // Each case's `segment` is the exact JSON the Rust calculate_zoom_state reads.
    const cameraCases: Array<{
      name: string;
      view: number;
      segment: Record<string, unknown>;
      samples: Array<{ t: number; zoom: number; posX: number; posY: number }>;
    }> = [];

    // 1. Constant auto-zoom path (no blocks).
    {
      const seg = autoSegment({ zoomBlocks: [] });
      const times = [0, 2.5, 5, 7.5, 10];
      cameraCases.push({
        name: "auto_zoom_constant",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // 2. Moving auto path (cursor drifts) + influence ramp.
    {
      const seg = autoSegment({
        smoothMotionPath: [
          { time: 0, x: 300, y: 300, zoom: 1.0 },
          { time: 5, x: 700, y: 400, zoom: 2.5 },
          { time: 10, x: 500, y: 500, zoom: 1.8 },
        ],
        zoomInfluencePoints: [
          { time: 0, value: 0.0 },
          { time: 3, value: 1.0 },
          { time: 10, value: 1.0 },
        ],
        zoomBlocks: [],
      });
      const times = [0, 1.5, 3, 5, 7.5, 10];
      cameraCases.push({
        name: "auto_path_with_influence",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // 3. Manual zoom block over the auto path: ease-in / hold / ease-out.
    {
      const seg = autoSegment({
        zoomBlocks: [
          blk({
            startTime: 2,
            endTime: 6,
            easeIn: 1,
            easeOut: 1,
            zoomFactor: 1.5,
            positionX: 0.2,
            positionY: 0.3,
          }),
        ],
      });
      const times = [1.9, 2.5, 4, 5.5, 6.1];
      cameraCases.push({
        name: "manual_block_over_auto",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // 4. Block -> auto gap -> block: gap must revert to the auto path.
    {
      const seg = autoSegment({
        zoomBlocks: [
          blk({ startTime: 1, endTime: 3, easeIn: 0.4, easeOut: 0.4, zoomFactor: 1.5, positionX: 0.2, positionY: 0.2 }),
          blk({ startTime: 6, endTime: 8, easeIn: 0.4, easeOut: 0.4, zoomFactor: 1.8, positionX: 0.8, positionY: 0.8 }),
        ],
      });
      const times = [2, 4.5, 7];
      cameraCases.push({
        name: "block_gap_block",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // 5. smootherStep envelope boundary sampling (no auto path -> blend with default).
    {
      const seg = autoSegment({
        smoothMotionPath: [],
        zoomBlocks: [
          blk({ startTime: 2, endTime: 8, easeIn: 2, easeOut: 2, zoomFactor: 2.0, positionX: 0.7, positionY: 0.4 }),
        ],
      });
      const times = [2.0, 2.5, 3.0, 4.0, 6.0, 7.5, 8.0];
      cameraCases.push({
        name: "smootherstep_boundary_no_auto",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // 6. followCursor block: anchor tracks the auto path inside the block.
    {
      const seg = autoSegment({
        smoothMotionPath: [
          { time: 0, x: 200, y: 800, zoom: 1.0 },
          { time: 10, x: 900, y: 100, zoom: 1.0 },
        ],
        zoomBlocks: [
          blk({ startTime: 1, endTime: 9, easeIn: 1, easeOut: 1, zoomFactor: 2.2, followCursor: true }),
        ],
      });
      const times = [2, 5, 8];
      cameraCases.push({
        name: "follow_cursor_block",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // 7. Linked manual blocks: hyperbolic direct travel bypasses the auto path.
    {
      const seg = autoSegment({
        zoomBlocks: [
          blk({
            startTime: 1,
            endTime: 3,
            easeIn: 0.4,
            easeOut: 0.4,
            zoomFactor: 1.5,
            positionX: 0.2,
            positionY: 0.2,
            directTransitionToNext: true,
          }),
          blk({
            startTime: 6,
            endTime: 8,
            easeIn: 0.4,
            easeOut: 0.4,
            zoomFactor: 1.8,
            positionX: 0.8,
            positionY: 0.8,
          }),
        ],
      });
      const times = [2.6, 3.5, 4.5, 5.5, 6.4];
      cameraCases.push({
        name: "linked_manual_hyperbolic_travel",
        view: VIEW,
        segment: cameraSegmentJson(seg),
        samples: times.map((t) => ({ t, ...sampleCameraState(t, seg, VIEW) })),
      });
    }

    // Raw zoomBlockEnvelope sampling (pure helper) for an extra fine-grained lock.
    const envBlock = blk({ startTime: 2, endTime: 8, easeIn: 1.5, easeOut: 1 });
    const envSamples = [1.9, 2.0, 2.75, 3.5, 5, 7, 7.5, 8.0, 8.1].map((t) => ({
      t,
      value: zoomBlockEnvelope(envBlock as any, t),
    }));

    // ---- CURSOR PRIMITIVE cases: clean exported-math twins ----
    const catmull = [
      { p0: 0, p1: 10, p2: 30, p3: 40, t: 0 },
      { p0: 0, p1: 10, p2: 30, p3: 40, t: 1 },
      { p0: 0, p1: 10, p2: 30, p3: 40, t: 0.5 },
      { p0: -5, p1: 2, p2: 9, p3: 3, t: 0.25 },
      { p0: 100, p1: 100, p2: 100, p3: 100, t: 0.7 },
    ].map((c) => ({ ...c, value: catmullRomInterpolate(c.p0, c.p1, c.p2, c.p3, c.t) }));

    const PI = Math.PI;
    const normalizeAngle = [4.0, -4.0, 7.5, -7.5, 0.0, PI, -PI, 3.2].map((a) => ({
      angle: a,
      value: normalizeAngleRad(a),
    }));

    const lerpAngle = [
      { from: 3.0, to: -3.0, t: 0.5 }, // across the +PI/-PI seam (short way)
      { from: -3.0, to: 3.0, t: 0.5 },
      { from: 0.0, to: 1.0, t: 0.25 },
      { from: 0.5, to: 0.5, t: 0.7 },
    ].map((c) => ({ ...c, value: lerpAngleRad(c.from, c.to, c.t) }));

    // smoothDampScalar including the overshoot-clamp branch (the just-fixed drift).
    const smoothDamp = simulateSmoothDamp();

    // Analytical spring trajectories (under / critical / over damped).
    const springScalar = [
      { label: "underdamped", omega: 30, zeta: 0.4 },
      { label: "critical", omega: 30, zeta: 1.0 },
      { label: "overdamped", omega: 30, zeta: 1.8 },
    ].map((cfg) => ({
      ...cfg,
      target: 100,
      dt: 1 / 120,
      trajectory: simulateSpring(0, 100, cfg.omega, cfg.zeta, 1 / 120, 60, false),
    }));

    // Angle spring crossing the seam (target normalized into the short arc).
    const springAngle = {
      from: 3.0,
      target: -3.0,
      omega: 25,
      zeta: 0.6,
      dt: 1 / 120,
      trajectory: simulateSpring(3.0, -3.0, 25, 0.6, 1 / 120, 40, true),
    };

    // Full smoothMousePositions pipeline (Catmull-Rom interp -> 3-pass uniform box
    // blur -> distance dedup). Bit-aligned with the Rust export within 1e-6, so it
    // is a shared cross-language golden. Two named tracks lock BOTH branches of the
    // per-window dispatch in cursorDynamics.ts:
    //   - "dense_interp": every window takes the Catmull-Rom interp path (covers the
    //     blur kernel + 60-frame clamp; the lone <2px pair toggles isClicked so the
    //     idle-skip is suppressed).
    //   - "static_dwell": the cursor jitters <2px in place, so most windows take the
    //     idle-skip short-circuit (push {...p1}; continue); embedded counter-cases
    //     (click toggle / cursor_type change at <2px) prove the AND guard.
    // Each track is sampled at smoothness 0 (crisp, windowSize=1), a mid value, and
    // the high end.
    const buildSmoothCases = (input: MousePosition[]) =>
      [0, 5, 10].map((smoothness) => ({
        smoothness,
        output: smoothMousePositions(
          input.map((p) => ({ ...p })),
          120,
          smoothnessBg(smoothness),
        ).map((p) => ({
          x: p.x,
          y: p.y,
          timestamp: p.timestamp,
          isClicked: Boolean(p.isClicked),
          cursor_type: p.cursor_type ?? "default",
        })),
      }));
    const smoothMouse = {
      tracks: [
        { name: "dense_interp", input: SMOOTH_MOUSE_INPUT, cases: buildSmoothCases(SMOOTH_MOUSE_INPUT) },
        { name: "static_dwell", input: STATIC_DWELL_INPUT, cases: buildSmoothCases(STATIC_DWELL_INPUT) },
      ],
    };

    const fixture = {
      version: 1,
      description:
        "Cross-language render golden for camera/cursor WYSIWYG math. TypeScript preview is canonical; Rust export must match within 1e-6. Regenerate with screen-record/tests/unit/_generateRenderGolden.gen.ts; never hand-edit numbers. See screen-record/docs/render-parity.md.",
      tolerance: 1e-6,
      camera: { view: VIEW, cases: cameraCases, zoomBlockEnvelope: { block: envBlock, samples: envSamples } },
      cursorPrimitives: {
        catmullRom: catmull,
        normalizeAngle,
        lerpAngle,
        smoothDampScalar: smoothDamp,
        springStepScalar: springScalar,
        springStepAngle: springAngle,
        smoothMousePositions: smoothMouse,
      },
    };

    writeFileSync(
      path.join(FIXTURE_DIR, "golden.json"),
      JSON.stringify(fixture, null, 2) + "\n",
      "utf8",
    );
  });
});

// Minimal BackgroundConfig whose only field smoothMousePositions reads is
// cursorSmoothness (via getCursorSmoothness). The Rust golden test mirrors this by
// deserializing a BackgroundConfig with the same cursorSmoothness.
function smoothnessBg(smoothness: number): BackgroundConfig {
  return { cursorSmoothness: smoothness } as unknown as BackgroundConfig;
}

// Build the minimal segment JSON the Rust VideoSegment deserializer reads.
function cameraSegmentJson(seg: VideoSegment): Record<string, unknown> {
  const s = seg as any;
  return {
    crop: s.crop ?? null,
    trimSegments: [{ startTime: 0, endTime: 10 }],
    zoomBlocks: (s.zoomBlocks ?? []).map((b: any) => ({
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
    zoomInfluencePoints: s.zoomInfluencePoints ?? [],
    smoothMotionPath: s.smoothMotionPath ?? [],
    cursorVisibilitySegments: null,
  };
}

// Drive smoothDampScalar across steps. Two scenarios:
//  - "settle": ordinary monotonic convergence (no clamp).
//  - "overshoot_clamp": a large initial velocity drives the output PAST the
//    target on the first step, firing the overshoot-clamp branch (output ==
//    target, velocity recomputed to 0). This is exactly the drift the Rust fix
//    aligns to; a step with value==target and velocity==0 must appear.
function simulateSmoothDamp() {
  const dt = 1 / 60;
  const settle = runSmoothDamp({ dt, smoothTime: 0.15, maxSpeed: 1e9, start: 0, target: 10, vel0: 0, n: 80 });
  const overshootClamp = runSmoothDamp({
    dt,
    smoothTime: 0.5,
    maxSpeed: 1e9,
    start: 0,
    target: 10,
    vel0: 5000, // huge inbound velocity -> first step overshoots -> clamp fires
    n: 12,
  });
  return { settle, overshootClamp };
}

function runSmoothDamp(cfg: {
  dt: number;
  smoothTime: number;
  maxSpeed: number;
  start: number;
  target: number;
  vel0: number;
  n: number;
}) {
  let current = cfg.start;
  let velocity = cfg.vel0;
  const steps: Array<{ step: number; value: number; velocity: number }> = [];
  for (let i = 0; i < cfg.n; i++) {
    const r = smoothDampScalar(current, cfg.target, velocity, cfg.smoothTime, cfg.maxSpeed, cfg.dt);
    current = r.value;
    velocity = r.velocity;
    steps.push({ step: i, value: r.value, velocity: r.velocity });
  }
  return {
    smoothTime: cfg.smoothTime,
    maxSpeed: cfg.maxSpeed,
    dt: cfg.dt,
    start: cfg.start,
    target: cfg.target,
    initialVelocity: cfg.vel0,
    steps,
  };
}

function simulateSpring(
  start: number,
  target: number,
  omega: number,
  zeta: number,
  dt: number,
  n: number,
  angle: boolean,
) {
  let value = start;
  let velocity = 0;
  const steps: Array<{ step: number; value: number; velocity: number }> = [];
  for (let i = 0; i < n; i++) {
    const r = angle
      ? springStepAngle(value, target, velocity, omega, zeta, dt)
      : springStepScalar(value, target, velocity, omega, zeta, dt);
    value = r.value;
    velocity = r.velocity;
    steps.push({ step: i, value: r.value, velocity: r.velocity });
  }
  return steps;
}
