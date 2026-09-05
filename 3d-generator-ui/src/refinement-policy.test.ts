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

test("separation choices match the shared control contract", () => {
  const layoutSource = readFileSync(new URL("./layout.ts", import.meta.url), "utf8");
  const contract = JSON.parse(readFileSync(new URL("../../parity-fixtures/image-to-3d/control-contract.json", import.meta.url), "utf8"));
  assert.match(layoutSource, /value="detailed" selected/);
  for (const level of contract.separationLevels) assert.ok(layoutSource.includes(`value="${level}"`));
});

test("revision changes choose a supported topology before evaluating its action button", () => {
  const source = readFileSync(new URL("./presentation.ts", import.meta.url), "utf8");
  assert.ok(source.indexOf("nodes.topologySelect.selectedOptions") < source.indexOf("nodes.refinementButtons.forEach"));
});
