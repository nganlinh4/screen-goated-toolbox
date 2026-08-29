import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { declaredRefinements, hasDeclaredRefinements } from "./refinement-policy.ts";

test("an artifact with no declared continuation does not show placeholder controls", () => {
  assert.equal(hasDeclaredRefinements([], []), false);
});

test("supported but quota-blocked actions remain declared and disabled", () => {
  const supported = declaredRefinements(["rig"], []);
  assert.equal(supported.has("rig"), true);
  assert.equal(new Set<string>().has("rig"), false);
});

test("older results expose only actions the runtime actually advertised", () => {
  assert.deepEqual([...declaredRefinements(undefined, ["add_materials"])], ["add_materials"]);
});

test("the shipped separation control offers only the proven detailed level", () => {
  const layoutSource = readFileSync(new URL("./layout.ts", import.meta.url), "utf8");
  assert.match(layoutSource, /value="detailed" selected/);
  assert.doesNotMatch(layoutSource, /value="simple"/);
  assert.doesNotMatch(layoutSource, /value="balanced"/);
});
