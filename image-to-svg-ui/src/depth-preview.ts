import type { Item } from "./types";

type DepthPreviewOptions = {
  artboard: HTMLElement;
  isSelected: (id: string) => boolean;
  busy: (item: Item) => boolean;
};

function loadImage(url: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Image preview could not be loaded"));
    image.src = url;
  });
}

export class DepthPreviewController {
  private animationFrame = 0;
  private version = 0;
  private resizeObserver?: ResizeObserver;

  constructor(private readonly options: DepthPreviewOptions) {}

  stop() {
    this.version += 1;
    if (this.animationFrame) cancelAnimationFrame(this.animationFrame);
    this.animationFrame = 0;
    this.resizeObserver?.disconnect();
  }

  async show(item: Item) {
    if (!item.sourceUrl || !item.depthUrl) return false;
    const version = ++this.version;
    if (this.animationFrame) cancelAnimationFrame(this.animationFrame);
    const [source, depth] = await Promise.all([
      loadImage(item.sourceUrl),
      loadImage(item.depthUrl),
    ]);
    if (!this.isCurrent(version, item)) return false;
    const scale = Math.min(1, 720 / Math.max(source.naturalWidth, source.naturalHeight));
    const width = Math.max(1, Math.round(source.naturalWidth * scale));
    const height = Math.max(1, Math.round(source.naturalHeight * scale));
    const sourceCanvas = document.createElement("canvas");
    const depthCanvas = document.createElement("canvas");
    sourceCanvas.width = depthCanvas.width = width;
    sourceCanvas.height = depthCanvas.height = height;
    const sourceContext = sourceCanvas.getContext("2d", { willReadFrequently: true })!;
    const depthContext = depthCanvas.getContext("2d", { willReadFrequently: true })!;
    sourceContext.drawImage(source, 0, 0, width, height);
    depthContext.drawImage(depth, 0, 0, width, height);
    const sourcePixels = sourceContext.getImageData(0, 0, width, height);
    const depthPixels = depthContext.getImageData(0, 0, width, height).data;
    const binCount = 6;
    const buffers = Array.from(
      { length: binCount },
      () => new Uint8ClampedArray(sourcePixels.data.length),
    );
    for (let offset = 0; offset < sourcePixels.data.length; offset += 4) {
      const bin = Math.min(binCount - 1, Math.floor(depthPixels[offset] / 256 * binCount));
      buffers[bin].set(sourcePixels.data.subarray(offset, offset + 4), offset);
    }
    const layers = buffers.map((buffer) => {
      const layer = document.createElement("canvas");
      layer.width = width;
      layer.height = height;
      layer.getContext("2d")!.putImageData(new ImageData(buffer, width, height), 0, 0);
      return layer;
    });
    if (!this.isCurrent(version, item)) return false;
    const canvas = document.createElement("canvas");
    canvas.className = "depth-separation-preview";
    canvas.width = width;
    canvas.height = height;
    canvas.setAttribute("role", "img");
    this.options.artboard.replaceChildren(canvas);
    this.fit(canvas, width / height);
    this.animate(canvas, layers, item, version, binCount);
    return true;
  }

  private isCurrent(version: number, item: Item) {
    return version === this.version
      && this.options.isSelected(item.id)
      && this.options.busy(item);
  }

  private fit(element: HTMLElement, ratio: number) {
    this.resizeObserver?.disconnect();
    const fit = () => {
      const maxWidth = this.options.artboard.clientWidth * 0.88;
      const maxHeight = this.options.artboard.clientHeight * 0.82;
      const fittedWidth = Math.min(maxWidth, maxHeight * ratio);
      element.style.width = `${fittedWidth}px`;
      element.style.height = `${fittedWidth / ratio}px`;
    };
    this.resizeObserver = new ResizeObserver(fit);
    this.resizeObserver.observe(this.options.artboard);
    fit();
  }

  private animate(
    canvas: HTMLCanvasElement,
    layers: HTMLCanvasElement[],
    item: Item,
    version: number,
    binCount: number,
  ) {
    const { width, height } = canvas;
    const context = canvas.getContext("2d")!;
    const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const started = performance.now();
    const draw = (now: number) => {
      if (!this.isCurrent(version, item)) return;
      const pulse = reducedMotion ? 0.58 : 0.48 + Math.sin((now - started) / 760) * 0.24;
      const spread = Math.min(width, height) * 0.065 * pulse;
      context.clearRect(0, 0, width, height);
      layers.forEach((layer, index) => {
        const depthPosition = index / (binCount - 1) - 0.5;
        const offsetX = depthPosition * spread * 1.7;
        const offsetY = -depthPosition * spread * 0.48;
        const layerScale = 1 + depthPosition * 0.035 * pulse;
        context.save();
        context.translate(width / 2 + offsetX, height / 2 + offsetY);
        context.scale(layerScale, layerScale);
        context.shadowColor = "rgba(22, 31, 48, 0.28)";
        context.shadowBlur = Math.abs(depthPosition) * 16 * pulse;
        context.drawImage(layer, -width / 2, -height / 2);
        context.restore();
      });
      if (!reducedMotion) this.animationFrame = requestAnimationFrame(draw);
    };
    draw(performance.now());
  }
}
