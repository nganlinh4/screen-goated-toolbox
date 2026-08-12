import { describe, expect, it } from "vitest";

import { normalizeProjectLimit } from "@/lib/projectManager";

describe("screen recorder project retention", () => {
  it("defaults and clamps the persisted user limit", () => {
    expect(normalizeProjectLimit(undefined)).toBe(50);
    expect(normalizeProjectLimit(9)).toBe(10);
    expect(normalizeProjectLimit(57.6)).toBe(58);
    expect(normalizeProjectLimit(101)).toBe(100);
  });
});
