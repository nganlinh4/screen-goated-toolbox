import { sanitizeSvg } from "./svg-security";

export const EDITABLE_SELECTOR = "path,rect,circle,ellipse,polygon,polyline,line";
export const EDIT_SURFACE_STYLE = `
  :host { display: block; width: 100%; height: 100%; }
  svg { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
  svg * { pointer-events: none; }
  svg [data-edit-index] { cursor: pointer; pointer-events: all; }
  svg [data-edit-index].selected {
    stroke: var(--cobalt) !important;
    stroke-width: 1.5px !important;
    vector-effect: non-scaling-stroke;
  }
  svg.show-outlines [data-edit-index]:not(.selected) {
    stroke: color-mix(in srgb, var(--cobalt) 72%, transparent) !important;
    stroke-width: 1px !important;
    vector-effect: non-scaling-stroke;
  }
`;

const MAX_EDIT_SOURCE_BYTES = 2 * 1024 * 1024;
const MAX_EDIT_GEOMETRY = 5_000;

export function prepareEditableSvg(text: string) {
  if (new TextEncoder().encode(text).byteLength > MAX_EDIT_SOURCE_BYTES) {
    throw new Error("edit budget exceeded");
  }
  const svg = sanitizeSvg(text);
  if (svg.querySelectorAll(EDITABLE_SELECTOR).length > MAX_EDIT_GEOMETRY) {
    throw new Error("edit budget exceeded");
  }
  const viewBox = (svg.getAttribute("viewBox") || "").trim().split(/[\s,]+/).map(Number);
  const width = viewBox.length === 4 && viewBox[2] > 0
    ? viewBox[2]
    : Number.parseFloat(svg.getAttribute("width") || "");
  const height = viewBox.length === 4 && viewBox[3] > 0
    ? viewBox[3]
    : Number.parseFloat(svg.getAttribute("height") || "");
  const ratio = Number.isFinite(width) && Number.isFinite(height) && width > 0 && height > 0
    ? width / height
    : 1;
  return {
    svg,
    ratio,
    originalWidth: svg.getAttribute("width") || "",
    originalHeight: svg.getAttribute("height") || "",
    pathCount: svg.querySelectorAll("path").length,
  };
}
