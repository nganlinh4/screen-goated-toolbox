import { readdir, stat } from "node:fs/promises";
import path from "node:path";

const assetsDirectory = path.resolve("dist", "assets");
const MAX_JAVASCRIPT_CHUNK_BYTES = 700 * 1024;
const MAX_TOTAL_JAVASCRIPT_BYTES = Math.floor(1.75 * 1024 * 1024);
const MAX_CSS_ASSET_BYTES = 200 * 1024;

const entries = await readdir(assetsDirectory, { withFileTypes: true });
const assets = [];
for (const entry of entries) {
  if (!entry.isFile()) continue;
  const filePath = path.join(assetsDirectory, entry.name);
  assets.push({ name: entry.name, bytes: (await stat(filePath)).size });
}

const javascript = assets.filter((asset) => asset.name.endsWith(".js"));
const css = assets.filter((asset) => asset.name.endsWith(".css"));
const violations = [
  ...javascript
    .filter((asset) => asset.bytes > MAX_JAVASCRIPT_CHUNK_BYTES)
    .map((asset) => `${asset.name}: ${asset.bytes} > ${MAX_JAVASCRIPT_CHUNK_BYTES}`),
  ...css
    .filter((asset) => asset.bytes > MAX_CSS_ASSET_BYTES)
    .map((asset) => `${asset.name}: ${asset.bytes} > ${MAX_CSS_ASSET_BYTES}`),
];
const totalJavascriptBytes = javascript.reduce(
  (total, asset) => total + asset.bytes,
  0,
);
if (totalJavascriptBytes > MAX_TOTAL_JAVASCRIPT_BYTES) {
  violations.push(
    `total JavaScript: ${totalJavascriptBytes} > ${MAX_TOTAL_JAVASCRIPT_BYTES}`,
  );
}
if (violations.length > 0) {
  throw new Error(`Recorder bundle budget exceeded:\n${violations.join("\n")}`);
}
console.log(
  `Verified ${javascript.length} JS chunks (${totalJavascriptBytes} bytes) and ${css.length} CSS assets.`,
);
