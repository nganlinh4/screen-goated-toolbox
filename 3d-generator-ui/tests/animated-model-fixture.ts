/** Small embedded skin, morph, and transform tracks for cross-surface acceptance. */
export function animatedModelFixture() {
  const chunks: Uint8Array[] = [];
  const views: object[] = [];
  const accessors: object[] = [];
  let length = 0;
  const add = (values: Float32Array | Uint16Array, type: string, width: number, bounds?: number[][]) => {
    const bytes = new Uint8Array(values.buffer);
    const bufferView = views.length;
    views.push({ buffer: 0, byteOffset: length, byteLength: bytes.length });
    chunks.push(bytes);
    length += bytes.length;
    const padding = (4 - length % 4) % 4;
    if (padding) { chunks.push(new Uint8Array(padding)); length += padding; }
    accessors.push({ bufferView, componentType: values instanceof Float32Array ? 5126 : 5123,
      count: values.length / width, type, ...(bounds ? { min: bounds[0], max: bounds[1] } : {}) });
    return accessors.length - 1;
  };
  const position = add(new Float32Array([-.7, -.5, 0, .7, -.5, 0, 0, .7, 0]), "VEC3", 3,
    [[-.7, -.5, 0], [.7, .7, 0]]);
  const normal = add(new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]), "VEC3", 3);
  const joints = add(new Uint16Array([0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]), "VEC4", 4);
  const weights = add(new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]), "VEC4", 4);
  const bind = add(new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1,
    1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]), "MAT4", 16);
  const times = add(new Float32Array([0, 1, 2]), "SCALAR", 1, [[0], [2]]);
  const movement = add(new Float32Array([-.5, 0, 0, .5, 0, 0, -.5, 0, 0]), "VEC3", 3);
  const morph = add(new Float32Array([0, 0, 0, 0, 0, 0, 0, 0, .5]), "VEC3", 3,
    [[0, 0, 0], [0, 0, .5]]);
  const morphValues = add(new Float32Array([0, 1, 0]), "SCALAR", 1);
  const rotation = add(new Float32Array([0, 0, 0, 1, 0, Math.SQRT1_2, 0, Math.SQRT1_2, 0, 0, 0, 1]), "VEC4", 4);
  const document = {
    asset: { version: "2.0" }, buffers: [{ byteLength: length }], bufferViews: views, accessors,
    meshes: [{ weights: [0], primitives: [{ attributes: { POSITION: position, NORMAL: normal,
      JOINTS_0: joints, WEIGHTS_0: weights }, targets: [{ POSITION: morph }], material: 0 }] }],
    materials: [{ doubleSided: true, pbrMetallicRoughness: {
      baseColorFactor: [.05, .5, .9, 1], metallicFactor: .3, roughnessFactor: .4,
    } }],
    nodes: [{ name: "Surface", mesh: 0, skin: 0, children: [1] },
      { name: "RootJoint", children: [2] }, { name: "TipJoint" }],
    skins: [{ joints: [1, 2], inverseBindMatrices: bind, skeleton: 1 }],
    scenes: [{ nodes: [0] }], scene: 0,
    animations: [
      { name: "Bend", samplers: [{ input: times, output: movement }, { input: times, output: morphValues }],
        channels: [{ sampler: 0, target: { node: 2, path: "translation" } },
          { sampler: 1, target: { node: 0, path: "weights" } }] },
      { name: "Turn", samplers: [{ input: times, output: rotation }],
        channels: [{ sampler: 0, target: { node: 0, path: "rotation" } }] },
    ],
  };
  const json = new TextEncoder().encode(JSON.stringify(document));
  const jsonLength = (json.length + 3) & ~3;
  const bytes = new Uint8Array(28 + jsonLength + length);
  const header = new DataView(bytes.buffer);
  header.setUint32(0, 0x46546c67, true); header.setUint32(4, 2, true);
  header.setUint32(8, bytes.length, true); header.setUint32(12, jsonLength, true);
  header.setUint32(16, 0x4e4f534a, true);
  bytes.fill(32, 20, 20 + jsonLength); bytes.set(json, 20);
  header.setUint32(20 + jsonLength, length, true); header.setUint32(24 + jsonLength, 0x004e4942, true);
  let offset = 28 + jsonLength;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}
