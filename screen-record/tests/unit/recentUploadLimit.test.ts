import { describe, expect, it } from "vitest";

import { normalizeRecentUploadLimit } from "@/hooks/useBackgroundUpload";

describe("uploaded background retention", () => {
  it("defaults and clamps the persisted user limit", () => {
    expect(normalizeRecentUploadLimit(undefined)).toBe(12);
    expect(normalizeRecentUploadLimit(0)).toBe(4);
    expect(normalizeRecentUploadLimit(13.6)).toBe(14);
    expect(normalizeRecentUploadLimit(99)).toBe(24);
  });
});
