import assert from "node:assert/strict";
import test from "node:test";
import { createSvgAssetLoader } from "./svg-asset-loader.ts";
import type { Item } from "./types.ts";

function resultItem(): Item {
  return {
    id: "result",
    batchId: "result",
    path: "source.png",
    sourceProvenance: "presentation",
    name: "source.png",
    model: "simple",
    outputDir: "output",
    stage: "done",
    outputPath: "result.svg",
  };
}

test("selection streams an inert SVG and defers editable text until explicit editing", async () => {
  const commands: string[] = [];
  const loader = createSvgAssetLoader(async <T>(command: string) => {
    commands.push(command);
    if (command === "svg_asset_url") return { url: "sgtcreation://result" } as T;
    if (command === "read_asset") return { text: "<svg/>" } as T;
    throw new Error(`unexpected command: ${command}`);
  });
  const item = resultItem();

  assert.equal(await loader.loadVectorPreview(item), "sgtcreation://result");
  assert.deepEqual(commands, ["svg_asset_url"]);
  assert.equal(await loader.loadVectorText(item), "<svg/>");
  assert.deepEqual(commands, ["svg_asset_url", "read_asset"]);
});
