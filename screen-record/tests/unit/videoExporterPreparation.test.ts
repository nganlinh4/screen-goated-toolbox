import { describe, expect, it } from "vitest";
import {
  collectNonFiniteNumberPaths,
  collectNullPaths,
  sanitizeNativeExportValue,
} from "@/lib/videoExporterPreparation";

describe("native export value preparation", () => {
  it("keeps large clean payloads by reference", () => {
    const value = {
      mousePositions: Array.from({ length: 100 }, (_, index) => ({
        x: index,
        y: index,
      })),
    };

    expect(sanitizeNativeExportValue(value)).toBe(value);
  });

  it("copies only branches that contain nullable object fields", () => {
    const stable = [{ x: 1 }];
    const value = { stable, optional: null, nested: { keep: true } };
    const sanitized = sanitizeNativeExportValue(value);

    expect(sanitized).toEqual({ stable, nested: { keep: true } });
    expect(sanitized.stable).toBe(stable);
    expect(sanitized.nested).toBe(value.nested);
  });

  it("reports invalid array nulls and non-finite numbers", () => {
    const value = sanitizeNativeExportValue({ values: [null, Number.NaN] });
    expect(collectNullPaths(value)).toEqual(["$.values[0]"]);
    expect(collectNonFiniteNumberPaths(value)).toEqual(["$.values[1]"]);
  });
});
