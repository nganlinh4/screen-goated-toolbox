import "./viewer-standalone.css";
import { ModelViewer, type ShadingMode } from "./viewer";
import { viewerToolbarMarkup } from "./viewer-toolbar";

export const VIEWER_DOCUMENT_VERSION = 2;
const PRODUCT_FONT_URL = "/creation-model-viewer/GoogleSansFlex.woff";

const productFont = new FontFace(
  "Google Sans Flex",
  `url(${JSON.stringify(PRODUCT_FONT_URL)}) format("woff")`,
  { style: "normal", weight: "1 1000", stretch: "25% 151%" },
);
document.fonts.add(productFont);
const productFontReady = productFont.load();

type ViewerLabels = {
  originalMaterials: string;
  toonOutline: string;
  partColors: string;
  toggleOutline: string;
  toggleRotation: string;
  toggleGrid: string;
  toggleWireframe: string;
  resetView: string;
  preview: string;
  previewUnavailable: string;
};

type ViewerStartOptions = {
  modelUrl: string;
  segmented: boolean;
  theme: "light" | "dark";
  labels: ViewerLabels;
};

const stage = document.querySelector<HTMLElement>("#viewerStage");
if (!stage) throw new Error("Missing viewer stage");
stage.innerHTML = `
  <canvas id="modelCanvas"></canvas>
  ${viewerToolbarMarkup()}
  <div class="viewer-status" id="viewerStatus" role="status" aria-live="polite"></div>
`;
document.documentElement.dataset.viewerVersion = String(VIEWER_DOCUMENT_VERSION);

function required<T extends Element>(selector: string): T {
  const node = stage?.querySelector<T>(selector);
  if (!node) throw new Error(`Missing viewer node: ${selector}`);
  return node;
}

const canvas = required<HTMLCanvasElement>("#modelCanvas");
const toolbar = required<HTMLElement>("#viewerToolbar");
const status = required<HTMLElement>("#viewerStatus");
const shadingButtons = [...stage.querySelectorAll<HTMLButtonElement>(".shading-tool")];
const outlineButton = required<HTMLButtonElement>("#outlineButton");
const rotateButton = required<HTMLButtonElement>("#rotateButton");
const gridButton = required<HTMLButtonElement>("#gridButton");
const wireButton = required<HTMLButtonElement>("#wireButton");
const fitButton = required<HTMLButtonElement>("#fitButton");
const viewer = new ModelViewer(canvas, stage);

const controlState = {
  outline: true,
  rotate: false,
  grid: false,
  wireframe: false,
};

function syncControls() {
  shadingButtons.forEach((button) => {
    const mode = button.dataset.shading as ShadingMode;
    button.classList.toggle("active", viewer.getShading() === mode);
    button.disabled = mode === "parts" && !viewer.hasParts();
  });
  outlineButton.classList.toggle("active", controlState.outline);
  rotateButton.classList.toggle("active", controlState.rotate);
  gridButton.classList.toggle("active", controlState.grid);
  wireButton.classList.toggle("active", controlState.wireframe);
}

shadingButtons.forEach((button) => button.addEventListener("click", () => {
  viewer.setShading(button.dataset.shading as ShadingMode);
  syncControls();
}));
outlineButton.addEventListener("click", () => {
  controlState.outline = !controlState.outline;
  viewer.setOutline(controlState.outline);
  syncControls();
});
rotateButton.addEventListener("click", () => {
  controlState.rotate = !controlState.rotate;
  viewer.setAutoRotate(controlState.rotate);
  syncControls();
});
gridButton.addEventListener("click", () => {
  controlState.grid = !controlState.grid;
  viewer.setGrid(controlState.grid);
  syncControls();
});
wireButton.addEventListener("click", () => {
  controlState.wireframe = !controlState.wireframe;
  viewer.setWireframe(controlState.wireframe);
  syncControls();
});
fitButton.addEventListener("click", () => viewer.fitView());

let startRevision = 0;
let disposed = false;

async function start(options: ViewerStartOptions) {
  if (disposed || !options.modelUrl.startsWith(`${location.origin}/`)) return;
  await productFontReady.catch(() => undefined);
  const revision = ++startRevision;
  document.documentElement.dataset.theme = options.theme;
  canvas.setAttribute("aria-label", options.labels.preview);
  document.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((node) => {
    const key = node.dataset.i18nTitle as keyof ViewerLabels;
    const label = options.labels[key];
    if (!label) return;
    node.title = label;
    node.setAttribute("aria-label", label);
  });
  viewer.setTheme(options.theme);
  status.hidden = true;
  toolbar.classList.remove("visible");
  try {
    const result = await viewer.setModel(options.modelUrl, options.segmented);
    if (!result || disposed || revision !== startRevision) return;
    toolbar.classList.add("visible");
    syncControls();
  } catch {
    if (disposed || revision !== startRevision) return;
    status.textContent = options.labels.previewUnavailable;
    status.hidden = false;
  }
}

function dispose() {
  if (disposed) return;
  disposed = true;
  ++startRevision;
  viewer.dispose();
}

Object.defineProperty(window, "sgtModelViewer", {
  configurable: false,
  enumerable: false,
  writable: false,
  value: Object.freeze({ start, dispose, version: VIEWER_DOCUMENT_VERSION }),
});
window.addEventListener("pagehide", dispose, { once: true });

declare global {
  interface Window {
    sgtModelViewer: {
      start: (options: ViewerStartOptions) => Promise<void>;
      dispose: () => void;
      version: number;
    };
  }
}
