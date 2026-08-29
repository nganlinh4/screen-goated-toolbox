import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("standalone document reuses the canonical viewer and complete controls", () => {
  const entry = source("./viewer-entry.ts");
  assert.match(entry, /from "\.\/viewer"/);
  assert.match(entry, /VIEWER_DOCUMENT_VERSION = 2/);
  for (const call of [
    "setShading", "setOutline", "setAutoRotate", "setGrid", "setWireframe", "fitView",
  ]) {
    assert.ok(entry.includes(`viewer.${call}`), `${call} must remain wired`);
  }
  assert.match(entry, /outline: true/);
  assert.match(entry, /wireframe: false/);
  assert.match(entry, /controlState\.outline/);
  assert.match(entry, /controlState\.wireframe/);
});

test("viewer document is versioned and cannot load external resources", () => {
  const document = source("../viewer/index.html");
  assert.match(document, /data-viewer-version="2"/);
  assert.match(document, /default-src 'none'/);
  assert.match(document, /script-src 'self'/);
  assert.match(document, /font-src 'self'/);
  assert.match(document, /connect-src 'self'/);
  assert.match(document, /object-src 'none'/);
  assert.doesNotMatch(document, /https?:\/\//);
});

test("standalone viewer uses the product face without bundling another font", () => {
  const style = source("./viewer-standalone.css");
  const entry = source("./viewer-entry.ts");
  assert.match(style, /font-family: "Google Sans Flex"/);
  assert.match(entry, /\/creation-model-viewer\/GoogleSansFlex\.woff/);
  assert.match(entry, /new FontFace/);
  assert.match(entry, /await productFontReady/);
  assert.doesNotMatch(style, /font: 13px system-ui/);
});

test("viewer build cannot add chunks to the desktop dist contract", () => {
  const config = source("../viewer.vite.config.ts");
  assert.match(config, /viewer-dist\/creation_model_viewer/);
  assert.match(config, /codeSplitting: false/);
  assert.match(config, /entryFileNames: "assets\/viewer\.js"/);
  assert.match(config, /cssCodeSplit: false/);
});
