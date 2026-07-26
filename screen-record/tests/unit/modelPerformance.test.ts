import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import {
  MODEL_PERFORMANCE_COLUMNS,
  formatModelLatencyMs,
  intelligenceStatIconName,
} from "@/components/ui/PanelSelect";

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

  it("maps all six intelligence levels to the shared stat scale", () => {
    expect([1, 2, 3, 4, 5, 6].map(intelligenceStatIconName)).toEqual(
      FIXTURE.performance.intelligence_stat_icons,
    );
  });

  it("uses the shared compact prefix columns", () => {
    expect(MODEL_PERFORMANCE_COLUMNS).toEqual({
      intelligenceWidth: FIXTURE.performance_columns.intelligence_width,
      gap: FIXTURE.performance_columns.inter_column_gap,
      latencyWidth: FIXTURE.performance_columns.latency_width,
    });
  });
});
