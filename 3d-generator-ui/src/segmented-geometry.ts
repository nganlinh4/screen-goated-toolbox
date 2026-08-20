import * as THREE from "three";

const MAX_VIEWER_PARTS = 1_024;
const MAX_SEGMENTATION_VERTICES = 500_000;
const MAX_SEGMENTATION_INDICES = 1_500_000;

class DisjointVertices {
  private parent: Int32Array;
  private rank: Uint8Array;

  constructor(count: number) {
    this.parent = Int32Array.from({ length: count }, (_, index) => index);
    this.rank = new Uint8Array(count);
  }

  find(value: number): number {
    let root = value;
    while (this.parent[root] !== root) root = this.parent[root];
    while (this.parent[value] !== value) {
      const next = this.parent[value];
      this.parent[value] = root;
      value = next;
    }
    return root;
  }

  join(left: number, right: number) {
    let leftRoot = this.find(left);
    let rightRoot = this.find(right);
    if (leftRoot === rightRoot) return;
    if (this.rank[leftRoot] < this.rank[rightRoot]) [leftRoot, rightRoot] = [rightRoot, leftRoot];
    this.parent[rightRoot] = leftRoot;
    if (this.rank[leftRoot] === this.rank[rightRoot]) this.rank[leftRoot] += 1;
  }
}

function disconnectedTriangleIndices(geometry: THREE.BufferGeometry): number[][] {
  const position = geometry.getAttribute("position");
  const index = geometry.getIndex();
  if (
    !position
    || !index
    || position.count > MAX_SEGMENTATION_VERTICES
    || index.count > MAX_SEGMENTATION_INDICES
    || index.count < 6
    || index.count % 3 !== 0
    || geometry.groups.length > 1
  ) return [];

  const vertices = new DisjointVertices(position.count);
  for (let offset = 0; offset < index.count; offset += 3) {
    const first = index.getX(offset);
    const second = index.getX(offset + 1);
    const third = index.getX(offset + 2);
    if (first >= position.count || second >= position.count || third >= position.count) return [];
    vertices.join(first, second);
    vertices.join(first, third);
  }

  const components = new Map<number, number[]>();
  for (let offset = 0; offset < index.count; offset += 3) {
    const first = index.getX(offset);
    const root = vertices.find(first);
    const values = components.get(root) || [];
    values.push(first, index.getX(offset + 1), index.getX(offset + 2));
    components.set(root, values);
    if (components.size > MAX_VIEWER_PARTS) return [];
  }
  return components.size > 1 ? [...components.values()] : [];
}

function componentGeometry(source: THREE.BufferGeometry, indices: number[]) {
  const geometry = new THREE.BufferGeometry();
  for (const name of Object.keys(source.attributes)) geometry.setAttribute(name, source.getAttribute(name));
  const sourceMorphs = source.morphAttributes as Record<
    string,
    Array<THREE.BufferAttribute | THREE.InterleavedBufferAttribute>
  >;
  const targetMorphs = geometry.morphAttributes as typeof sourceMorphs;
  for (const [name, attributes] of Object.entries(sourceMorphs)) {
    targetMorphs[name] = attributes;
  }
  geometry.morphTargetsRelative = source.morphTargetsRelative;
  const maximum = indices.reduce((current, value) => Math.max(current, value), 0);
  const values = maximum <= 65_535 ? new Uint16Array(indices) : new Uint32Array(indices);
  geometry.setIndex(new THREE.BufferAttribute(values, 1));
  return geometry;
}

function copyObjectState(source: THREE.Object3D, target: THREE.Object3D) {
  target.position.copy(source.position);
  target.quaternion.copy(source.quaternion);
  target.scale.copy(source.scale);
  target.matrix.copy(source.matrix);
  target.matrixAutoUpdate = source.matrixAutoUpdate;
  target.visible = source.visible;
  target.layers.mask = source.layers.mask;
  target.renderOrder = source.renderOrder;
  target.userData = { ...source.userData };
}

function expandMesh(mesh: THREE.Mesh, components: number[][]) {
  const parent = mesh.parent;
  if (!parent) return;
  const siblingIndex = parent.children.indexOf(mesh);
  const group = new THREE.Group();
  group.name = mesh.name;
  copyObjectState(mesh, group);
  for (const child of [...mesh.children]) group.add(child);
  components.forEach((indices, index) => {
    const part = new THREE.Mesh(componentGeometry(mesh.geometry, indices), mesh.material);
    part.name = `part_${String(index + 1).padStart(3, "0")}`;
    part.castShadow = mesh.castShadow;
    part.receiveShadow = mesh.receiveShadow;
    part.frustumCulled = false;
    part.userData = { ...mesh.userData };
    group.add(part);
  });
  parent.remove(mesh);
  parent.add(group);
  parent.children.splice(parent.children.indexOf(group), 1);
  parent.children.splice(siblingIndex, 0, group);
  mesh.geometry.dispose();
}

export function prepareSegmentedGeometry(root: THREE.Object3D, segmented: boolean) {
  const meshes: THREE.Mesh[] = [];
  root.traverse((child) => {
    if (child instanceof THREE.Mesh && !(child instanceof THREE.SkinnedMesh)) meshes.push(child);
  });
  const expandDisconnectedParts = segmented && meshes.length === 1;
  for (const mesh of meshes) {
    if (!mesh.geometry.getAttribute("normal")) mesh.geometry.computeVertexNormals();
    if (!expandDisconnectedParts) continue;
    const components = disconnectedTriangleIndices(mesh.geometry);
    if (components.length > 1) expandMesh(mesh, components);
  }
}

export type GeometryStats = {
  vertices: number;
  faces: number;
  /// Present only for a quad model, where the file's own faces are polygons
  /// and the triangle count is an artefact of rendering rather than the mesh.
  polygons?: number;
  quads?: number;
};

/// The runtime marks the primitive carrying the source file's face loops and
/// records how many of those faces there are, because a triangle count says
/// nothing true about a quad mesh.
const QUAD_WIREFRAME_MARKER = "sgtQuadWireframe";

function polygonStats(root: THREE.Object3D) {
  let polygons: number | undefined;
  let quads: number | undefined;
  root.traverse((child) => {
    const data = child.userData as Record<string, unknown> | undefined;
    if (!data || data[QUAD_WIREFRAME_MARKER] !== true) return;
    if (typeof data.polygonCount === "number") polygons = data.polygonCount;
    if (typeof data.quadCount === "number") quads = data.quadCount;
  });
  return { polygons, quads };
}

export function modelGeometryStats(root: THREE.Object3D): GeometryStats {
  const stats: GeometryStats = { vertices: 0, faces: 0 };
  const countedPositions = new WeakSet<THREE.BufferAttribute | THREE.InterleavedBufferAttribute>();
  const countedUnindexedPositions = new WeakSet<THREE.BufferAttribute | THREE.InterleavedBufferAttribute>();
  const countedIndexedPrimitives = new WeakMap<
    THREE.BufferAttribute | THREE.InterleavedBufferAttribute,
    WeakSet<THREE.BufferAttribute>
  >();

  root.traverse((child) => {
    if (!(child instanceof THREE.Mesh)) return;
    const position = child.geometry.getAttribute("position");
    if (!position) return;
    if (!countedPositions.has(position)) {
      countedPositions.add(position);
      stats.vertices += position.count;
    }

    const index = child.geometry.getIndex();
    if (!index) {
      if (!countedUnindexedPositions.has(position)) {
        countedUnindexedPositions.add(position);
        stats.faces += Math.floor(position.count / 3);
      }
      return;
    }

    let countedIndices = countedIndexedPrimitives.get(position);
    if (!countedIndices) {
      countedIndices = new WeakSet<THREE.BufferAttribute>();
      countedIndexedPrimitives.set(position, countedIndices);
    }
    if (!countedIndices.has(index)) {
      countedIndices.add(index);
      stats.faces += Math.floor(index.count / 3);
    }
  });


  const { polygons, quads } = polygonStats(root);
  if (polygons !== undefined) stats.polygons = polygons;
  if (quads !== undefined) stats.quads = quads;
  return stats;
}
