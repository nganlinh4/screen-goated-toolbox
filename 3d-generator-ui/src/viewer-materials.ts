import * as THREE from "three";

export type ViewerMaterialSet = {
  original: THREE.Material | THREE.Material[];
  toon: THREE.Material | THREE.Material[];
  parts: THREE.Material | THREE.Material[];
};

function oneOrMany(materials: THREE.Material[]) {
  return materials.length === 1 ? materials[0] : materials;
}

function makeTwoSided<T extends THREE.Material>(material: T) {
  material.side = THREE.DoubleSide;
  material.needsUpdate = true;
  return material;
}

function createToonMaterial(source: THREE.Material, gradientMap: THREE.Texture) {
  const standard = source as THREE.MeshStandardMaterial;
  return new THREE.MeshToonMaterial({
    color: standard.color?.clone() || new THREE.Color(0xffffff),
    map: standard.map || null,
    normalMap: standard.normalMap || null,
    alphaMap: standard.alphaMap || null,
    aoMap: standard.aoMap || null,
    emissive: standard.emissive?.clone() || new THREE.Color(0x000000),
    emissiveMap: standard.emissiveMap || null,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0,
    vertexColors: standard.vertexColors,
    gradientMap,
  });
}

export function createViewerMaterialSet(
  originals: THREE.Material[],
  partColor: number,
  gradientMap: THREE.Texture,
): ViewerMaterialSet {
  const original = originals.map(makeTwoSided);
  const toon = originals.map((material) => createToonMaterial(material, gradientMap));
  const parts = originals.map(() => new THREE.MeshToonMaterial({
    color: partColor,
    gradientMap,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0,
  }));
  return {
    original: oneOrMany(original),
    toon: oneOrMany(toon),
    parts: oneOrMany(parts),
  };
}
