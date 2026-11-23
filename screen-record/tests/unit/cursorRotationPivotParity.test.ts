import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { getCursorRotationPivot } from "@/lib/renderer/cursorDynamics";
import { buildCursorSlotId, CURSOR_PACKS, CURSOR_RENDER_KINDS } from "@/lib/renderer/cursorModel";

const SHADER_PATH = path.resolve(import.meta.dirname, "../../../src/overlay/screen_record/gpu_export/shader/cursor.rs");

type Pivot = { x: number; y: number };

/** Reads the export shader's kind -> pivot rules from `get_rotation_pivot`. */
function shaderPivotByKindIndex(): (kindIndex: number) => Pivot {
  const source = readFileSync(SHADER_PATH, "utf8");
  const body = source.match(/fn get_rotation_pivot\(type_id: f32\) -> vec2<f32> \{([\s\S]*?)\n\}/)?.[1];
  if (!body) throw new Error("get_rotation_pivot not found in shader/cursor.rs");
  expect(body).toContain("let kind = slot % 12;");
  const rules: Array<{ kinds: number[]; pivot: Pivot }> = [];
  const rule = /if ((?:kind == \d+(?: \|\| )?)+) \{[\s\S]*?return vec2<f32>\(([-\d.]+), ([-\d.]+)\);/g;
  for (const match of body.matchAll(rule)) {
    rules.push({
      kinds: [...match[1].matchAll(/kind == (\d+)/g)].map((kind) => Number(kind[1])),
      pivot: { x: Number(match[2]), y: Number(match[3]) },
    });
  }
  const fallback = [...body.matchAll(/return vec2<f32>\(([-\d.]+), ([-\d.]+)\);/g)].at(-1);
  if (!fallback || rules.length === 0) throw new Error("could not parse shader pivot rules");
  const fallbackPivot = { x: Number(fallback[1]), y: Number(fallback[2]) };
  return (kindIndex) => rules.find((entry) => entry.kinds.includes(kindIndex))?.pivot ?? fallbackPivot;
}

describe("cursor rotation pivot parity", () => {
  it("has exactly the twelve kinds the shader's slot % 12 indexes", () => {
    expect(CURSOR_RENDER_KINDS).toHaveLength(12);
  });

  it("matches the export shader for every pack and cursor kind", () => {
    const shaderPivot = shaderPivotByKindIndex();
    for (const pack of CURSOR_PACKS) {
      CURSOR_RENDER_KINDS.forEach((kind, kindIndex) => {
        const slot = buildCursorSlotId(pack, kindIndex);
        expect(getCursorRotationPivot(`${kind}-${pack}`), `${kind}-${pack}`).toEqual(shaderPivot(slot % 12));
      });
    }
  });

  it("falls back to the default-arrow pivot for unknown types, like the shader", () => {
    expect(getCursorRotationPivot("mystery")).toEqual(shaderPivotByKindIndex()(0));
  });
});
