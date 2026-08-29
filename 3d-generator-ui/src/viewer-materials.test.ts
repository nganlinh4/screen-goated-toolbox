import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three";
import { createViewerMaterialSet } from "./viewer-materials.ts";

test("every preview shading mode keeps generated surfaces visible from both sides", () => {
  const gradient = new THREE.DataTexture(new Uint8Array([0, 255]), 2, 1, THREE.RedFormat);
  const originals = [
    new THREE.MeshStandardMaterial(),
    new THREE.MeshStandardMaterial({ side: THREE.BackSide }),
  ];
  const set = createViewerMaterialSet(originals, 0x23b99f, gradient);

  for (const mode of [set.original, set.toon, set.parts]) {
    assert.ok(Array.isArray(mode));
    assert.equal(mode.length, originals.length);
    mode.forEach((material) => assert.equal(material.side, THREE.DoubleSide));
  }

  const materials = [set.original, set.toon, set.parts]
    .flatMap((mode) => Array.isArray(mode) ? mode : [mode]);
  new Set(materials).forEach((material) => material.dispose());
  gradient.dispose();
});
