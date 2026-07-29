import assert from "node:assert/strict";
import test from "node:test";

import {
  validateSvgAttribute,
  validateSvgCss,
  validateSvgExpansionCosts,
  svgLocalReferenceTargets,
  type SvgExpansionCost,
} from "./svg-security.ts";

test("stylesheet indirection and CSS motion fail closed", () => {
  assert.throws(() => validateSvgCss("path { fill: url(#paint) }", true));
  assert.throws(() => validateSvgCss("@keyframes pulse { to { opacity: 1 } }", true));
  assert.throws(() => validateSvgCss("animation: pulse 1s infinite", false));
  assert.throws(() => validateSvgCss("transition-property: opacity", false));
  assert.throws(() => validateSvgCss("fill: u\\72l(https://example.invalid/a)", false));
  assert.throws(() => validateSvgCss("filter : blur(2px)", false));
  assert.throws(() => validateSvgCss("f/**/ilter: blur(2px)", false));
  assert.throws(() => validateSvgCss(":host { display: none }", true));
  assert.throws(() => validateSvgCss("path::part(control) { opacity: 0 }", true));
  assert.doesNotThrow(() => validateSvgCss("fill: url(#paint)", false));
});

test("local references reject aliases and resource attributes reject escapes", () => {
  assert.doesNotThrow(() => validateSvgAttribute("href", "#paint", false));
  assert.throws(() => validateSvgAttribute("href", "#%70aint", false));
  assert.throws(() => validateSvgAttribute("fill", "url(#%70aint)", false));
  assert.throws(() => validateSvgAttribute("href", "https://example.invalid/a", false));
  assert.throws(() => validateSvgAttribute("xml:base", "#paint", false));
  assert.throws(() => validateSvgAttribute("fill", "u\\72l(#paint)", false));
});

test("local reference collection preserves URL multiplicity and href precedence", () => {
  assert.deepEqual(
    svgLocalReferenceTargets([
      { name: "xlink:href", value: "#fallback" },
      { name: "fill", value: "url(#paint) url('#paint')" },
      { name: "href", value: "#primary" },
    ]),
    ["paint", "paint", "primary"],
  );
  assert.deepEqual(
    svgLocalReferenceTargets([{ name: "xlink:href", value: "#fallback" }]),
    ["fallback"],
  );
});

test("local reference expansion preserves multiplicity for raster and DAG costs", () => {
  const repeatedRaster = new Map<string, SvgExpansionCost>([
    ["\0sgt-root", {
      elements: 4,
      rasterPixels: 16_000_000,
      uses: ["raster", "raster"],
    }],
    ["raster", { elements: 1, rasterPixels: 16_000_000, uses: [] }],
  ]);
  assert.throws(() => validateSvgExpansionCosts(repeatedRaster));
  repeatedRaster.get("\0sgt-root")!.uses.pop();
  assert.doesNotThrow(() => validateSvgExpansionCosts(repeatedRaster));

  const dag = new Map<string, SvgExpansionCost>([
    ["\0sgt-root", { elements: 1, rasterPixels: 0, uses: ["n0"] }],
  ]);
  for (let index = 0; index < 16; index += 1) {
    dag.set(`n${index}`, {
      elements: 3,
      rasterPixels: 0,
      uses: [`n${index + 1}`, `n${index + 1}`],
    });
  }
  dag.set("n16", { elements: 1, rasterPixels: 0, uses: [] });
  assert.throws(() => validateSvgExpansionCosts(dag));

  const tooManyReferences = new Map<string, SvgExpansionCost>([
    ["\0sgt-root", {
      elements: 1,
      rasterPixels: 0,
      uses: Array.from({ length: 100_001 }, () => "missing"),
    }],
  ]);
  assert.throws(() => validateSvgExpansionCosts(tooManyReferences));
});
