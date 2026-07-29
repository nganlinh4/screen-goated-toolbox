import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { ShaderPass } from "three/examples/jsm/postprocessing/ShaderPass.js";
import { LatestOnlyLane } from "./latest-only-lane";
import { meshVertexCount, prepareSegmentedGeometry } from "./segmented-geometry";

export type ShadingMode = "original" | "toon" | "parts";
export type ModelStats = { vertices: number; faces: number };

type MaterialSet = {
  original: THREE.Material | THREE.Material[];
  toon: THREE.Material | THREE.Material[];
  parts: THREE.Material | THREE.Material[];
};

const PART_PALETTE = [0x23b99f, 0xf2bd55, 0x5f9fe8, 0xe77958, 0x9a7bd4, 0x66b878, 0xd16d9e, 0x4db2c8];
const MAX_MODEL_ASSET_BYTES = 100 * 1024 * 1024;

const EdgeShader = {
  uniforms: {
    tDiffuse: { value: null },
    tMetadata: { value: null },
    tDepth: { value: null },
    uTexel: { value: new THREE.Vector2(1, 1) },
    uInk: { value: new THREE.Color(0x081512) },
    uStrength: { value: 1 },
  },
  vertexShader: `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }
  `,
  fragmentShader: `
    uniform sampler2D tDiffuse;
    uniform sampler2D tMetadata;
    uniform sampler2D tDepth;
    uniform vec2 uTexel;
    uniform vec3 uInk;
    uniform float uStrength;
    varying vec2 vUv;

    void main() {
      vec4 color = texture2D(tDiffuse, vUv);
      vec4 center = texture2D(tMetadata, vUv);
      float centerDepth = texture2D(tDepth, vUv).r;
      vec3 centerNormal = center.rgb * 2.0 - 1.0;
      float edge = 0.0;
      vec2 directions[8];
      directions[0] = vec2(-1.0, 0.0); directions[1] = vec2(1.0, 0.0);
      directions[2] = vec2(0.0, -1.0); directions[3] = vec2(0.0, 1.0);
      directions[4] = vec2(-1.0, -1.0); directions[5] = vec2(1.0, -1.0);
      directions[6] = vec2(-1.0, 1.0); directions[7] = vec2(1.0, 1.0);
      for (int i = 0; i < 8; i++) {
        vec2 uv = clamp(vUv + directions[i] * uTexel, vec2(0.0), vec2(1.0));
        vec4 sampleInfo = texture2D(tMetadata, uv);
        float sampleDepth = texture2D(tDepth, uv).r;
        vec3 sampleNormal = sampleInfo.rgb * 2.0 - 1.0;
        float silhouette = step(0.001, abs(step(0.001, center.a) - step(0.001, sampleInfo.a)));
        float surface = step(0.006, abs(center.a - sampleInfo.a));
        float depth = step(0.0012, abs(centerDepth - sampleDepth));
        float normal = step(0.34, distance(centerNormal, sampleNormal));
        edge = max(edge, max(silhouette, max(surface * 0.78, max(depth * 0.82, normal * 0.58))));
      }
      color.rgb = mix(color.rgb, uInk, edge * uStrength);
      gl_FragColor = color;
    }
  `,
};

export class ModelViewer {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(34, 1, 0.01, 100);
  private controls: OrbitControls;
  private composer: EffectComposer;
  private edgePass: ShaderPass;
  private metadataTarget: THREE.WebGLRenderTarget;
  private metadataMaterial: THREE.ShaderMaterial;
  private metadataIds = new WeakMap<THREE.Object3D, number>();
  private result: THREE.Group | null = null;
  private idleObject: THREE.Group;
  private grid = new THREE.GridHelper(4, 20, 0x23b99f, 0x42504f);
  private startedAt = performance.now();
  private modelBlend = 0;
  private hasSegmentedParts = false;
  private resizeObserver: ResizeObserver;
  private shading: ShadingMode = "toon";
  private outline = true;
  private wireframe = false;
  private contentRevision = 0;
  private modelLoads = new LatestOnlyLane<THREE.Group>();
  private frameRequest = 0;
  private interacting = false;
  private interactionEndedAt = 0;
  private interactionListener?: (active: boolean) => void;
  private theme: "light" | "dark" = "dark";
  private hemisphere = new THREE.HemisphereLight(0xe5fbf5, 0x1a2524, 2.15);
  private key = new THREE.DirectionalLight(0xf5fff9, 3.2);
  private rim = new THREE.DirectionalLight(0x54c9b3, 2.0);

  constructor(private canvas: HTMLCanvasElement, private container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: "high-performance" });
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.setPixelRatio(this.pixelRatio());
    this.camera.position.set(0, 0.1, 3.4);

    this.controls = new OrbitControls(this.camera, this.canvas);
    this.controls.enableDamping = true;
    this.controls.enablePan = true;
    this.controls.minDistance = 0.7;
    this.controls.maxDistance = 10;
    this.controls.autoRotateSpeed = 1.15;
    this.controls.enabled = false;
    this.controls.addEventListener("start", () => {
      this.interacting = true;
      this.interactionListener?.(true);
      this.requestRender();
    });
    this.controls.addEventListener("change", () => this.requestRender());
    this.controls.addEventListener("end", () => {
      this.interacting = false;
      this.interactionEndedAt = performance.now();
      this.interactionListener?.(false);
      this.requestRender();
    });

    this.key.position.set(3, 4, 5);
    this.rim.position.set(-4, 1, -2);
    this.scene.add(this.hemisphere, this.key, this.rim);
    this.grid.position.y = -0.82;
    this.grid.visible = false;
    this.scene.add(this.grid);

    const renderTarget = new THREE.WebGLRenderTarget(1, 1, { depthBuffer: true });
    renderTarget.samples = Math.min(2, this.renderer.capabilities.maxSamples);
    this.composer = new EffectComposer(this.renderer, renderTarget);
    this.composer.addPass(new RenderPass(this.scene, this.camera));
    this.edgePass = new ShaderPass(EdgeShader);
    this.composer.addPass(this.edgePass);
    this.composer.addPass(new OutputPass());

    this.metadataTarget = new THREE.WebGLRenderTarget(1, 1, {
      depthBuffer: true,
      depthTexture: new THREE.DepthTexture(1, 1, THREE.UnsignedIntType),
      minFilter: THREE.NearestFilter,
      magFilter: THREE.NearestFilter,
    });
    this.metadataTarget.texture.colorSpace = THREE.NoColorSpace;
    this.metadataMaterial = new THREE.ShaderMaterial({
      uniforms: { uSurfaceId: { value: 0 } },
      vertexShader: `
        varying vec3 vViewNormal;
        void main() {
          vViewNormal = normalize(normalMatrix * normal);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }
      `,
      fragmentShader: `
        uniform float uSurfaceId;
        varying vec3 vViewNormal;
        void main() {
          if (uSurfaceId <= 0.0) discard;
          gl_FragColor = vec4(normalize(vViewNormal) * 0.5 + 0.5, uSurfaceId);
        }
      `,
      side: THREE.DoubleSide,
      depthTest: true,
      depthWrite: true,
    });
    this.metadataMaterial.onBeforeRender = (_renderer, _scene, _camera, _geometry, object) => {
      this.metadataMaterial.uniforms.uSurfaceId.value = this.metadataIds.get(object) || 0;
    };

    this.idleObject = this.createIdleObject();
    this.scene.add(this.idleObject);
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.setTheme(document.documentElement.dataset.theme === "light" ? "light" : "dark");
    this.resize();
    this.requestRender();
  }

  onInteractionChange(listener: (active: boolean) => void) {
    this.interactionListener = listener;
  }

  private pixelRatio() {
    return Math.min(1.5, Math.max(1, window.devicePixelRatio));
  }

  private createIdleObject() {
    const group = new THREE.Group();
    const geometry = new THREE.IcosahedronGeometry(0.68, 3);
    group.add(
      new THREE.Points(geometry, new THREE.PointsMaterial({ color: 0x23b99f, size: 0.018, transparent: true, opacity: 0.7 })),
      new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ color: 0x66817c, wireframe: true, transparent: true, opacity: 0.2 })),
    );
    group.position.y = 0.12;
    return group;
  }

  setTheme(theme: "light" | "dark") {
    this.theme = theme;
    const background = theme === "light" ? 0xf3f7f6 : 0x141918;
    this.scene.background = new THREE.Color(background);
    this.renderer.setClearColor(background, 1);
    this.edgePass.uniforms.uInk.value.set(theme === "light" ? 0x203b36 : 0x07120f);
    this.hemisphere.color.set(theme === "light" ? 0xffffff : 0xdffbf4);
    this.hemisphere.groundColor.set(theme === "light" ? 0x9bacaa : 0x14211f);
    this.key.color.set(theme === "light" ? 0xfffdf5 : 0xf3fff9);
    this.rim.color.set(theme === "light" ? 0x169b84 : 0x54c9b3);
    this.requestRender();
  }

  showIdle() {
    this.cancelPendingModelLoad();
    this.clearResult();
    this.idleObject.visible = true;
    this.controls.enabled = false;
    this.camera.position.set(0, 0.1, 3.4);
    this.camera.lookAt(0, 0, 0);
    this.requestRender();
  }

  cancelPendingModelLoad() {
    ++this.contentRevision;
    this.modelLoads.invalidate();
  }

  async setModel(dataUrl: string, segmented: boolean): Promise<ModelStats | null> {
    const revision = ++this.contentRevision;
    const object = await this.modelLoads.run(
      (signal) => this.loadModel(dataUrl, signal),
      (stale) => this.disposeGroup(stale),
    );
    if (!object) return null;
    if (revision !== this.contentRevision) {
      this.disposeGroup(object);
      return null;
    }
    try {
      this.idleObject.visible = false;
      prepareSegmentedGeometry(object, segmented);
      const box = new THREE.Box3().setFromObject(object);
      const center = box.getCenter(new THREE.Vector3());
      const size = box.getSize(new THREE.Vector3());
      object.position.sub(center);
      object.scale.setScalar(1.55 / Math.max(size.x, size.y, size.z, 0.001));
      object.updateMatrixWorld(true);

      let meshIndex = 0;
      const stats: ModelStats = { vertices: 0, faces: 0 };
      object.traverse((child) => {
        if (!(child instanceof THREE.Mesh)) return;
        const positions = child.geometry.getAttribute("position");
        stats.vertices += meshVertexCount(child.geometry);
        stats.faces += Math.floor((child.geometry.getIndex()?.count || positions?.count || 0) / 3);
        const originals = Array.isArray(child.material) ? child.material : [child.material];
        const toon = originals.map((material) => this.createToonMaterial(material));
        const parts = originals.map(() => new THREE.MeshToonMaterial({
          color: PART_PALETTE[meshIndex % PART_PALETTE.length],
          gradientMap: this.toonGradient(),
          transparent: true,
          opacity: 0,
        }));
        const set: MaterialSet = {
          original: originals.length === 1 ? originals[0] : originals,
          toon: toon.length === 1 ? toon[0] : toon,
          parts: parts.length === 1 ? parts[0] : parts,
        };
        child.userData.viewerMaterials = set;
        this.metadataIds.set(child, ((meshIndex % 254) + 1) / 255);
        child.castShadow = false;
        child.receiveShadow = false;
        meshIndex += 1;
      });

      this.disposeGroup(this.result);
      const root = new THREE.Group();
      root.add(object);
      this.result = root;
      this.scene.add(root);
      this.hasSegmentedParts = segmented && meshIndex > 1;
      this.shading = this.hasSegmentedParts ? "parts" : "toon";
      this.applyMaterials();
      this.modelBlend = 0;
      this.controls.enabled = true;
      this.resize();
      this.fitView();
      this.requestRender();
      return stats;
    } catch (error) {
      this.disposeGroup(object);
      throw error;
    }
  }

  private async loadModel(dataUrl: string, signal: AbortSignal) {
    const response = await fetch(dataUrl, { cache: "no-store", signal });
    if (!response.ok) throw new Error("Model preview was unavailable");
    const declared = Number(response.headers.get("content-length") || 0);
    if (
      declared !== 0
      && (!Number.isSafeInteger(declared) || declared < 20 || declared > MAX_MODEL_ASSET_BYTES)
    ) {
      throw new Error("Model preview exceeded its size limit");
    }
    const bytes = await response.arrayBuffer();
    if (bytes.byteLength < 20 || bytes.byteLength > MAX_MODEL_ASSET_BYTES) {
      throw new Error("Model preview exceeded its size limit");
    }
    if (signal.aborted) throw new Error("Model preview was superseded");
    return (await new GLTFLoader().parseAsync(bytes, "")).scene;
  }

  private createToonMaterial(material: THREE.Material) {
    const source = material as THREE.MeshStandardMaterial;
    return new THREE.MeshToonMaterial({
      color: source.color?.clone() || new THREE.Color(0xffffff),
      map: source.map || null,
      normalMap: source.normalMap || null,
      alphaMap: source.alphaMap || null,
      aoMap: source.aoMap || null,
      emissive: source.emissive?.clone() || new THREE.Color(0x000000),
      emissiveMap: source.emissiveMap || null,
      side: source.side,
      transparent: true,
      opacity: 0,
      vertexColors: source.vertexColors,
      gradientMap: this.toonGradient(),
    });
  }

  private toonGradient() {
    const cached = this.scene.userData.toonGradient as THREE.DataTexture | undefined;
    if (cached) return cached;
    const texture = new THREE.DataTexture(new Uint8Array([72, 132, 194, 255]), 4, 1, THREE.RedFormat);
    texture.minFilter = THREE.NearestFilter;
    texture.magFilter = THREE.NearestFilter;
    texture.colorSpace = THREE.NoColorSpace;
    texture.needsUpdate = true;
    this.scene.userData.toonGradient = texture;
    return texture;
  }

  setShading(mode: ShadingMode) {
    if (mode === "parts" && !this.hasSegmentedParts) return;
    this.shading = mode;
    this.applyMaterials();
    this.requestRender();
  }

  getShading() { return this.shading; }
  hasParts() { return this.hasSegmentedParts; }

  setOutline(enabled: boolean) {
    this.outline = enabled;
    this.edgePass.uniforms.uStrength.value = enabled ? 0.92 : 0;
    this.resize();
    this.requestRender();
  }

  setAutoRotate(enabled: boolean) {
    this.controls.autoRotate = enabled;
    this.requestRender();
  }

  setGrid(enabled: boolean) {
    this.grid.visible = enabled && Boolean(this.result);
    this.requestRender();
  }

  setWireframe(enabled: boolean) {
    this.wireframe = enabled;
    this.applyMaterials();
    this.requestRender();
  }

  fitView() {
    if (!this.result) return;
    const box = new THREE.Box3().setFromObject(this.result);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z) * 0.62;
    const distance = Math.max(1.7, radius / Math.tan(THREE.MathUtils.degToRad(this.camera.fov * 0.5)) * 1.12);
    this.controls.target.copy(center);
    this.camera.position.set(center.x + distance * 0.12, center.y + distance * 0.04, center.z + distance);
    this.camera.near = Math.max(0.001, distance / 1000);
    this.camera.far = distance * 50;
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.requestRender();
  }

  private applyMaterials() {
    this.result?.traverse((child) => {
      if (!(child instanceof THREE.Mesh)) return;
      const set = child.userData.viewerMaterials as MaterialSet | undefined;
      if (!set) return;
      child.material = set[this.shading];
      const materials = Array.isArray(child.material) ? child.material : [child.material];
      materials.forEach((material) => {
        if ("wireframe" in material) (material as THREE.MeshBasicMaterial).wireframe = this.wireframe;
        material.transparent = true;
        material.opacity = this.modelBlend;
        material.needsUpdate = true;
      });
    });
    this.edgePass.uniforms.uStrength.value = this.outline ? 0.92 : 0;
  }

  private clearResult() {
    this.disposeGroup(this.result);
    this.result = null;
    this.hasSegmentedParts = false;
    this.grid.visible = false;
    this.modelBlend = 0;
    this.resize();
  }

  private disposeGroup(group: THREE.Group | null) {
    if (!group) return;
    this.scene.remove(group);
    const disposed = new Set<THREE.Material>();
    const disposedTextures = new Set<THREE.Texture>();
    const sharedGradient = this.scene.userData.toonGradient as THREE.Texture | undefined;
    group.traverse((child) => {
      if (!(child instanceof THREE.Mesh || child instanceof THREE.Points)) return;
      child.geometry?.dispose();
      const set = child.userData.viewerMaterials as MaterialSet | undefined;
      const materials = set
        ? [set.original, set.toon, set.parts].flatMap((entry) => Array.isArray(entry) ? entry : [entry])
        : (Array.isArray(child.material) ? child.material : [child.material]);
      materials.forEach((material) => {
        if (!disposed.has(material)) {
          disposed.add(material);
          Object.values(material).forEach((value) => {
            if (
              value instanceof THREE.Texture &&
              value !== sharedGradient &&
              !disposedTextures.has(value)
            ) {
              disposedTextures.add(value);
              value.dispose();
            }
          });
          material.dispose();
        }
      });
    });
  }

  private resize() {
    const width = Math.max(1, this.container.clientWidth);
    const height = Math.max(1, this.container.clientHeight);
    const pixelRatio = this.pixelRatio();
    this.renderer.setPixelRatio(pixelRatio);
    this.renderer.setSize(width, height, false);
    this.composer.setPixelRatio(pixelRatio);
    this.composer.setSize(width, height);
    const edgeEnabled = this.outline && Boolean(this.result);
    this.edgePass.enabled = edgeEnabled;
    this.metadataTarget.setSize(
      edgeEnabled ? width * pixelRatio : 1,
      edgeEnabled ? height * pixelRatio : 1,
    );
    this.edgePass.uniforms.uTexel.value.set(0.78 / (width * pixelRatio), 0.78 / (height * pixelRatio));
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.requestRender();
  }

  private renderMetadata() {
    const previousTarget = this.renderer.getRenderTarget();
    const previousOverride = this.scene.overrideMaterial;
    const previousColor = this.renderer.getClearColor(new THREE.Color());
    const previousAlpha = this.renderer.getClearAlpha();
    this.scene.overrideMaterial = this.metadataMaterial;
    this.renderer.setRenderTarget(this.metadataTarget);
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.clear(true, true, true);
    this.renderer.render(this.scene, this.camera);
    this.scene.overrideMaterial = previousOverride;
    this.renderer.setRenderTarget(previousTarget);
    this.renderer.setClearColor(previousColor, previousAlpha);
    this.edgePass.uniforms.tMetadata.value = this.metadataTarget.texture;
    this.edgePass.uniforms.tDepth.value = this.metadataTarget.depthTexture;
  }

  private requestRender = () => {
    if (!this.frameRequest && document.visibilityState === "visible") {
      this.frameRequest = requestAnimationFrame(this.animate);
    }
  };

  private animate = (now = performance.now()) => {
    this.frameRequest = 0;
    if (document.visibilityState !== "visible") return;
    const time = (now - this.startedAt) / 1000;
    if (this.idleObject.visible) {
      this.idleObject.rotation.x = time * 0.08;
      this.idleObject.rotation.y = time * 0.14;
      this.idleObject.position.y = 0.1 + Math.sin(time * 0.9) * 0.025;
    }
    if (this.result && this.modelBlend < 1) {
      this.modelBlend = Math.min(1, this.modelBlend + 0.025);
      const eased = 1 - Math.pow(1 - this.modelBlend, 3);
      this.result.scale.setScalar(0.82 + eased * 0.18);
      this.result.traverse((child) => {
        if (!(child instanceof THREE.Mesh)) return;
        const materials = Array.isArray(child.material) ? child.material : [child.material];
        materials.forEach((material) => { material.opacity = eased; });
      });
    }
    const controlsChanged = this.controls.update();
    const edgeEnabled = this.outline && Boolean(this.result) && !this.interacting;
    this.edgePass.enabled = edgeEnabled;
    if (edgeEnabled) this.renderMetadata();
    this.composer.render();
    if (
      this.idleObject.visible
      || this.modelBlend < 1
      || this.controls.autoRotate
      || this.interacting
      || controlsChanged
      || now - this.interactionEndedAt < 360
    ) {
      this.requestRender();
    }
  };
}
