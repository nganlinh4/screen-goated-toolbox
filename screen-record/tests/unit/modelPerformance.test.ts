import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import { formatModelLatencyMs } from "@/components/ui/PanelSelect";

const FIXTURE = JSON.parse(
  readFileSync(
    path.resolve(__dirname, "../../../parity-fixtures/model-catalog/presentation.json"),
    "utf8",
  ),
);

describe("model performance presentation", () => {
  it("matches the shared latency labels", () => {
    for (const testCase of FIXTURE.performance.latency_format_cases) {
      expect(formatModelLatencyMs(testCase.milliseconds)).toBe(testCase.label);
    }
    expect(formatModelLatencyMs(null)).toBe(FIXTURE.performance.unknown_label);
  });
});
