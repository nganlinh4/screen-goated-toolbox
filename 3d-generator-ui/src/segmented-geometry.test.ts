import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three";
import { modelGeometryStats } from "./segmented-geometry.ts";

function indexedGeometry(
  position: THREE.BufferAttribute,
  indices: number[],
) {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", position);
  geometry.setIndex(indices);
  return geometry;
}

test("shared vertex buffers are counted once across separated parts", () => {
  const position = new THREE.BufferAttribute(new Float32Array(4 * 3), 3);
  const root = new THREE.Group();
  root.add(
    new THREE.Mesh(indexedGeometry(position, [0, 1, 2])),
    new THREE.Mesh(indexedGeometry(position, [0, 2, 3])),
  );

  assert.deepEqual(modelGeometryStats(root), { vertices: 4, faces: 2 });
});

test("repeated instances of the same primitive do not inflate topology", () => {
  const position = new THREE.BufferAttribute(new Float32Array(3 * 3), 3);
  const geometry = indexedGeometry(position, [0, 1, 2]);
  const root = new THREE.Group();
  root.add(new THREE.Mesh(geometry), new THREE.Mesh(geometry));

  assert.deepEqual(modelGeometryStats(root), { vertices: 3, faces: 1 });
});

test("independent indexed and unindexed geometry remains additive", () => {
  const indexedPosition = new THREE.BufferAttribute(new Float32Array(4 * 3), 3);
  const unindexedPosition = new THREE.BufferAttribute(new Float32Array(6 * 3), 3);
  const unindexed = new THREE.BufferGeometry();
  unindexed.setAttribute("position", unindexedPosition);
  const root = new THREE.Group();
  root.add(
    new THREE.Mesh(indexedGeometry(indexedPosition, [0, 1, 2, 0, 2, 3])),
    new THREE.Mesh(unindexed),
  );

  assert.deepEqual(modelGeometryStats(root), { vertices: 10, faces: 4 });
});

test("a quad model reports its own polygons instead of its render triangles", () => {
  const root = new THREE.Group();
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(12), 3));
  geometry.setIndex([0, 1, 2, 0, 2, 3]);
  root.add(new THREE.Mesh(geometry));
  const edges = new THREE.LineSegments(geometry);
  edges.userData = { sgtQuadWireframe: true, polygonCount: 1, quadCount: 1 };
  root.add(edges);

  const stats = modelGeometryStats(root);
  // Two triangles are what the GPU draws; one quad is what the file contains.
  assert.equal(stats.faces, 2);
  assert.equal(stats.polygons, 1);
  assert.equal(stats.quads, 1);
});

test("a triangle model reports no polygon counts at all", () => {
  const root = new THREE.Group();
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(9), 3));
  root.add(new THREE.Mesh(geometry));

  const stats = modelGeometryStats(root);
  assert.equal(stats.polygons, undefined);
  assert.equal(stats.quads, undefined);
});
