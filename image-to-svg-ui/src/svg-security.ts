const MAX_EMBEDDED_RASTER_PIXELS = 16_000_000;
const MAX_TOTAL_EMBEDDED_RASTER_PIXELS = 32_000_000;
const MAX_EXPANDED_ELEMENTS = 50_000;
const MAX_USE_DEPTH = 64;
const MAX_REFERENCE_OCCURRENCES = 100_000;
const MAX_LOCAL_IDENTIFIER_BYTES = 512;
const EXPANSION_ROOT = "\0sgt-root";
const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
const XLINK_NAMESPACE = "http://www.w3.org/1999/xlink";

export interface SvgExpansionCost {
  elements: number;
  rasterPixels: number;
  uses: string[];
}

export function validateSvgExpansionCosts(costs: Map<string, SvgExpansionCost>): void {
  if ((costs.get(EXPANSION_ROOT)?.uses.length || 0) > MAX_REFERENCE_OCCURRENCES) {
    throw new Error("SVG reference expansion is too complex");
  }
  const visiting = new Set<string>();
  const memo = new Map<string, [number, number]>();
  const expand = (id: string, depth: number): [number, number] => {
    if (depth > MAX_USE_DEPTH || visiting.has(id)) {
      throw new Error("SVG reference expansion is too complex");
    }
    const cached = memo.get(id);
    if (cached) return cached;
    const direct = costs.get(id);
    if (!direct) return [0, 0];
    visiting.add(id);
    let elements = direct.elements;
    let rasterPixels = direct.rasterPixels;
    validateExpandedCost(elements, rasterPixels);
    for (const target of direct.uses) {
      const expanded = expand(target, depth + 1);
      elements += expanded[0];
      rasterPixels += expanded[1];
      validateExpandedCost(elements, rasterPixels);
    }
    visiting.delete(id);
    const result: [number, number] = [elements, rasterPixels];
    memo.set(id, result);
    return result;
  };
  expand(EXPANSION_ROOT, 0);
}

function validateExpandedCost(elements: number, rasterPixels: number): void {
  if (
    !Number.isSafeInteger(elements)
    || !Number.isSafeInteger(rasterPixels)
    || elements > MAX_EXPANDED_ELEMENTS
    || rasterPixels > MAX_TOTAL_EMBEDDED_RASTER_PIXELS
  ) throw new Error("SVG reference expansion is too complex");
}

export function svgLocalReferenceTargets(
  attributes: ReadonlyArray<{ name: string; value: string }>,
): string[] {
  const targets: string[] = [];
  let href: string | undefined;
  let xlinkHref: string | undefined;
  for (const attribute of attributes) {
    if (attribute.name === "href") href = attribute.value;
    if (attribute.name === "xlink:href") xlinkHref = attribute.value;
    for (const match of attribute.value.matchAll(/url\s*\(\s*([^)]*?)\s*\)/giu)) {
      const target = match[1].trim().replace(/^['"]|['"]$/g, "");
      if (target.startsWith("#")) {
        validateLocalIdentifier(target.slice(1));
        targets.push(target.slice(1));
      }
    }
  }
  const effectiveHref = [href, xlinkHref].find((value) => value?.trim());
  if (effectiveHref?.trim().startsWith("#")) {
    const target = effectiveHref.trim().slice(1);
    validateLocalIdentifier(target);
    targets.push(target);
  }
  return targets;
}

function validateSvgReferenceExpansion(
  root: Element,
  rasterPixelsByElement: WeakMap<Element, number>,
): void {
  const costs = new Map<string, SvgExpansionCost>([
    [EXPANSION_ROOT, { elements: 0, rasterPixels: 0, uses: [] }],
  ]);
  const collect = (element: Element, ancestorIds: string[]): void => {
    const id = element.getAttribute("id");
    if (id) {
      validateLocalIdentifier(id);
      if (costs.has(id)) throw new Error("SVG contains duplicate identifiers");
      costs.set(id, { elements: 0, rasterPixels: 0, uses: [] });
    }
    const owners = id ? [...ancestorIds, id] : ancestorIds;
    const targets = svgLocalReferenceTargets(
      [...element.attributes].map(({ name, value }) => ({ name, value })),
    );
    const pixels = rasterPixelsByElement.get(element) || 0;
    for (const owner of [EXPANSION_ROOT, ...owners]) {
      const cost = costs.get(owner);
      if (!cost) throw new Error("SVG reference expansion is invalid");
      cost.elements += 1;
      cost.rasterPixels += pixels;
      cost.uses.push(...targets);
      if (owner === EXPANSION_ROOT && cost.uses.length > MAX_REFERENCE_OCCURRENCES) {
        throw new Error("SVG reference expansion is too complex");
      }
      validateExpandedCost(cost.elements, cost.rasterPixels);
    }
    [...element.children].forEach((child) => collect(child, owners));
  };
  collect(root, []);
  validateSvgExpansionCosts(costs);
}

function jpegDimensions(bytes: string): [number, number] | undefined {
  if (bytes.charCodeAt(0) !== 0xff || bytes.charCodeAt(1) !== 0xd8) return undefined;
  let offset = 2;
  const startOfFrame = new Set([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf]);
  while (offset + 3 < bytes.length) {
    if (bytes.charCodeAt(offset) !== 0xff) {
      offset += 1;
      continue;
    }
    while (bytes.charCodeAt(offset) === 0xff) offset += 1;
    const marker = bytes.charCodeAt(offset++);
    if (marker === 0xd9 || marker === 0xda) return undefined;
    if (marker === 0xd8 || marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) continue;
    if (offset + 1 >= bytes.length) return undefined;
    const length = bytes.charCodeAt(offset) * 256 + bytes.charCodeAt(offset + 1);
    if (length < 2 || offset + length > bytes.length) return undefined;
    if (startOfFrame.has(marker) && length >= 7) {
      const height = bytes.charCodeAt(offset + 3) * 256 + bytes.charCodeAt(offset + 4);
      const width = bytes.charCodeAt(offset + 5) * 256 + bytes.charCodeAt(offset + 6);
      return [width, height];
    }
    offset += length;
  }
  return undefined;
}

function embeddedRasterPixels(value: string): number | undefined {
  const match = /^data:image\/(png|jpeg);base64,(.+)$/is.exec(value);
  if (!match || value.length > 2_800_000) return undefined;
  let bytes = "";
  try {
    bytes = atob(match[2]);
  } catch {
    return undefined;
  }
  let dimensions: [number, number] | undefined;
  if (match[1].toLowerCase() === "png") {
    if (bytes.length < 24 || bytes.slice(0, 8) !== "\x89PNG\r\n\x1a\n") return undefined;
    if (pngIsAnimated(bytes)) return undefined;
    const width = bytes.charCodeAt(16) * 0x1000000
      + bytes.charCodeAt(17) * 0x10000 + bytes.charCodeAt(18) * 0x100 + bytes.charCodeAt(19);
    const height = bytes.charCodeAt(20) * 0x1000000
      + bytes.charCodeAt(21) * 0x10000 + bytes.charCodeAt(22) * 0x100 + bytes.charCodeAt(23);
    dimensions = [width, height];
  } else {
    dimensions = jpegDimensions(bytes);
  }
  if (!dimensions || dimensions[0] <= 0 || dimensions[1] <= 0) return undefined;
  const pixels = dimensions[0] * dimensions[1];
  return pixels <= MAX_EMBEDDED_RASTER_PIXELS ? pixels : undefined;
}

function pngIsAnimated(bytes: string): boolean {
  let offset = 8;
  while (offset + 12 <= bytes.length) {
    const length = bytes.charCodeAt(offset) * 0x1000000
      + bytes.charCodeAt(offset + 1) * 0x10000
      + bytes.charCodeAt(offset + 2) * 0x100
      + bytes.charCodeAt(offset + 3);
    const kind = bytes.slice(offset + 4, offset + 8);
    if (kind === "acTL") return true;
    const end = offset + 12 + length;
    if (!Number.isSafeInteger(end) || end > bytes.length || kind === "IEND") break;
    offset = end;
  }
  return false;
}

function isMotionProperty(name: string): boolean {
  return name === "animation"
    || name.startsWith("animation-")
    || name === "-webkit-animation"
    || name.startsWith("-webkit-animation-")
    || name === "transition"
    || name.startsWith("transition-");
}

function validateLocalIdentifier(value: string): void {
  if (
    !value
    || new TextEncoder().encode(value).length > MAX_LOCAL_IDENTIFIER_BYTES
    || /[\s\\%'"`()]/u.test(value)
    || [...value].some((character) => character.charCodeAt(0) < 0x20)
  ) throw new Error("SVG contains an invalid local reference");
}

function validateUrlFunctions(value: string): void {
  let remaining = value.toLowerCase();
  while (remaining.includes("url(")) {
    const start = remaining.indexOf("url(") + 4;
    const end = remaining.indexOf(")", start);
    if (end < 0) throw new Error("SVG contains a malformed resource URL");
    const target = remaining.slice(start, end).trim().replace(/^['"]|['"]$/g, "");
    if (!target.startsWith("#")) throw new Error("SVG cannot load external resources");
    validateLocalIdentifier(target.slice(1));
    remaining = remaining.slice(end + 1);
  }
}

function containsMotionProperty(css: string): boolean {
  return css.split(/[{};]/).some((declaration) => {
    const separator = declaration.indexOf(":");
    return separator >= 0 && isMotionProperty(declaration.slice(0, separator).trim());
  });
}

export function validateSvgCss(css: string, stylesheet: boolean): void {
  const normalized = css.toLowerCase();
  const compact = normalized.replace(/\s/g, "");
  if (
    normalized.includes("\\")
    || normalized.includes("/*")
    || normalized.includes("*/")
    || normalized.includes("<!--")
    || normalized.includes("-->")
    || normalized.includes("@import")
    || normalized.includes("javascript:")
    || normalized.includes("vbscript:")
    || normalized.includes("expression(")
    || normalized.includes("-moz-binding")
    || normalized.includes("behavior:")
    || normalized.includes("image-set(")
    || normalized.includes(":host")
    || normalized.includes("::part")
    || normalized.includes("::slotted")
    || normalized.includes("@keyframes")
    || normalized.includes("@-webkit-keyframes")
    || containsMotionProperty(normalized)
    || compact.includes("filter:")
    || (stylesheet && normalized.includes("url("))
  ) throw new Error("SVG contains unsupported active content");
  validateUrlFunctions(normalized);
}

export function validateSvgAttribute(
  attributeName: string,
  rawValue: string,
  embeddedRaster: boolean,
): void {
  const name = attributeName.toLowerCase();
  const value = rawValue.trim();
  const normalized = value.toLowerCase();
  const localReference = name === "href" || name === "xlink:href";
  if (
    name.startsWith("on")
    || name === "filter"
    || isMotionProperty(name)
    || ["src", "data", "poster", "formaction", "xml:base"].includes(name)
    || normalized.includes("javascript:")
    || normalized.includes("vbscript:")
    || normalized.includes("data:text/html")
    || normalized.includes("@import")
    || normalized.includes("expression(")
    || normalized.includes("-moz-binding")
    || normalized.includes("behavior:")
    || normalized.includes("/*")
    || normalized.includes("*/")
    || normalized.includes("\\")
  ) throw new Error("SVG contains unsupported active content");
  if (localReference && value.startsWith("#")) {
    validateLocalIdentifier(value.slice(1));
  } else if (localReference && !embeddedRaster) {
    throw new Error("SVG cannot load external resources");
  }
  if (name === "style") validateSvgCss(value, false);
  else validateUrlFunctions(value);
}

export function sanitizeSvg(text: string): SVGSVGElement {
  const doc = new DOMParser().parseFromString(text, "image/svg+xml");
  if (
    doc.querySelector("parsererror")
    || doc.documentElement.tagName.toLowerCase() !== "svg"
    || doc.documentElement.namespaceURI !== SVG_NAMESPACE
    || doc.documentElement.getAttribute("xmlns") !== SVG_NAMESPACE
  ) {
    throw new Error("Invalid SVG result");
  }
  const containsProcessingInstruction = (node: Node): boolean =>
    node.nodeType === 7 || [...node.childNodes].some(containsProcessingInstruction);
  if (containsProcessingInstruction(doc)) {
    throw new Error("SVG contains unsupported active content");
  }
  if (doc.querySelector(
    "script,foreignObject,iframe,object,embed,feImage,audio,video,canvas,"
    + "animate,animateMotion,animateTransform,set,discard,include,handler,listener",
  )) throw new Error("SVG contains unsupported active content");
  doc.querySelectorAll("style").forEach((node) => {
    validateSvgCss(node.textContent || "", true);
  });
  let totalEmbeddedRasterPixels = 0;
  const rasterPixelsByElement = new WeakMap<Element, number>();
  doc.querySelectorAll("*").forEach((node) => {
    const localName = node.localName.toLowerCase();
    if (
      node.namespaceURI !== "http://www.w3.org/2000/svg"
      || ["include", "handler", "listener"].includes(localName)
      || localName === "filter"
      || localName.startsWith("fe")
    ) throw new Error("SVG contains unsupported active content");
    for (const attribute of [...node.attributes]) {
      const rawName = attribute.name;
      const name = rawName.toLowerCase();
      if (
        rawName.toLowerCase() === "xmlns"
        || rawName.toLowerCase().startsWith("xmlns:")
      ) {
        if (
          !(
            rawName === "xmlns" && attribute.value.trim() === SVG_NAMESPACE
            || rawName === "xmlns:xlink" && attribute.value.trim() === XLINK_NAMESPACE
          )
        ) throw new Error("SVG contains unsupported active content");
        continue;
      }
      if (
        rawName.includes(":")
        && !["xml:lang", "xml:space"].includes(rawName)
        && !(rawName === "xlink:href" && attribute.namespaceURI === XLINK_NAMESPACE)
      ) throw new Error("SVG contains unsupported active content");
      const rasterPixels = localName === "image" && (rawName === "href" || rawName === "xlink:href")
        ? embeddedRasterPixels(attribute.value.trim())
        : undefined;
      const embeddedRaster = rasterPixels !== undefined;
      if (rasterPixels !== undefined) {
        totalEmbeddedRasterPixels += rasterPixels;
        if (totalEmbeddedRasterPixels > MAX_TOTAL_EMBEDDED_RASTER_PIXELS) {
          throw new Error("SVG embedded images are too large");
        }
        rasterPixelsByElement.set(
          node,
          (rasterPixelsByElement.get(node) || 0) + rasterPixels,
        );
      }
      validateSvgAttribute(name, attribute.value, embeddedRaster);
    }
  });
  validateSvgReferenceExpansion(doc.documentElement, rasterPixelsByElement);
  return document.importNode(doc.documentElement, true) as unknown as SVGSVGElement;
}
