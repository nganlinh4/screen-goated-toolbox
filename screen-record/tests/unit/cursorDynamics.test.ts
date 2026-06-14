import { describe, expect, it } from "vitest";
import {
  catmullRomInterpolate,
  normalizeAngleRad,
  lerpAngleRad,
  smoothDampScalar,
  springStepScalar,
} from "@/lib/renderer/cursorDynamics";

const PI = Math.PI;

describe("catmullRomInterpolate", () => {
  it("passes through p1 at t=0 and p2 at t=1", () => {
    expect(catmullRomInterpolate(0, 10, 30, 40, 0)).toBeCloseTo(10, 9);
    expect(catmullRomInterpolate(0, 10, 30, 40, 1)).toBeCloseTo(30, 9);
  });

  it("interpolates between p1 and p2 in the interior", () => {
    const v = catmullRomInterpolate(0, 10, 30, 40, 0.5);
    expect(v).toBeGreaterThan(10);
    expect(v).toBeLessThan(30);
  });

  it("is constant when all control points are equal", () => {
    expect(catmullRomInterpolate(100, 100, 100, 100, 0.37)).toBeCloseTo(100, 9);
  });
});

describe("normalizeAngleRad", () => {
  it("wraps angles into [-PI, PI]", () => {
    for (const a of [4.0, -4.0, 7.5, -7.5, 3.2, -3.2, 12.0, -12.0]) {
      const n = normalizeAngleRad(a);
      expect(n).toBeGreaterThanOrEqual(-PI - 1e-9);
      expect(n).toBeLessThanOrEqual(PI + 1e-9);
      // Normalized angle differs from the original by a whole number of turns.
      const turns = (a - n) / (2 * PI);
      expect(turns).toBeCloseTo(Math.round(turns), 9);
    }
  });

  it("leaves angles already in range untouched", () => {
    expect(normalizeAngleRad(0)).toBe(0);
    expect(normalizeAngleRad(1.0)).toBeCloseTo(1.0, 12);
    expect(normalizeAngleRad(PI)).toBeCloseTo(PI, 12);
    expect(normalizeAngleRad(-PI)).toBeCloseTo(-PI, 12);
  });
});

describe("lerpAngleRad", () => {
  it("takes the short way across the +-PI seam", () => {
    // 3.0 -> -3.0: the short path is +0.283 rad (through PI), not -6 rad.
    const mid = lerpAngleRad(3.0, -3.0, 0.5);
    // Halfway along the short arc sits at +-PI.
    expect(Math.abs(mid)).toBeCloseTo(PI, 6);
    // Going from -3.0 -> 3.0 takes the mirror short arc.
    expect(Math.abs(lerpAngleRad(-3.0, 3.0, 0.5))).toBeCloseTo(PI, 6);
  });

  it("returns the endpoints at t=0 and t=1", () => {
    expect(lerpAngleRad(0.4, 1.2, 0)).toBeCloseTo(0.4, 9);
    expect(lerpAngleRad(0.4, 1.2, 1)).toBeCloseTo(1.2, 9);
  });

  it("does not move when from == to", () => {
    expect(lerpAngleRad(0.5, 0.5, 0.7)).toBeCloseTo(0.5, 12);
  });
});

describe("smoothDampScalar", () => {
  it("does not overshoot the target on the clamp branch", () => {
    // A large inbound velocity would analytically shoot past the target; the
    // overshoot guard must clamp to the target with zero velocity.
    const r = smoothDampScalar(0, 10, 5000, 0.5, 1e9, 1 / 60);
    expect(r.value).toBeCloseTo(10, 9);
    expect(r.velocity).toBe(0);
  });

  it("settles monotonically toward the target without overshoot", () => {
    let value = 0;
    let velocity = 0;
    const target = 10;
    let prev = value;
    for (let i = 0; i < 200; i++) {
      const r = smoothDampScalar(value, target, velocity, 0.15, 1e9, 1 / 60);
      value = r.value;
      velocity = r.velocity;
      // Never crosses above the target (no overshoot for a rest start).
      expect(value).toBeLessThanOrEqual(target + 1e-6);
      expect(value).toBeGreaterThanOrEqual(prev - 1e-9);
      prev = value;
    }
    expect(value).toBeCloseTo(target, 4);
  });
});

describe("springStepScalar", () => {
  it("short-circuits when already at rest at the target", () => {
    const r = springStepScalar(5, 5, 0, 30, 1.0, 1 / 120);
    expect(r.value).toBe(5);
    expect(r.velocity).toBe(0);
  });

  it("settles toward the target for under/critical/over damped springs", () => {
    for (const zeta of [0.4, 1.0, 1.8]) {
      let value = 0;
      let velocity = 0;
      const target = 100;
      for (let i = 0; i < 600; i++) {
        const r = springStepScalar(value, target, velocity, 30, zeta, 1 / 120);
        value = r.value;
        velocity = r.velocity;
      }
      expect(value).toBeCloseTo(target, 2);
      expect(velocity).toBeCloseTo(0, 2);
    }
  });

  it("overshoots once for an underdamped spring (the wiggle)", () => {
    // zeta < 1 must overshoot the target at least once before settling.
    let value = 0;
    let velocity = 0;
    const target = 100;
    let overshot = false;
    for (let i = 0; i < 120; i++) {
      const r = springStepScalar(value, target, velocity, 30, 0.4, 1 / 120);
      value = r.value;
      velocity = r.velocity;
      if (value > target) overshot = true;
    }
    expect(overshot).toBe(true);
  });

  it("never overshoots for a critically damped spring", () => {
    let value = 0;
    let velocity = 0;
    const target = 100;
    for (let i = 0; i < 600; i++) {
      const r = springStepScalar(value, target, velocity, 30, 1.0, 1 / 120);
      value = r.value;
      velocity = r.velocity;
      expect(value).toBeLessThanOrEqual(target + 1e-6);
    }
  });
});
