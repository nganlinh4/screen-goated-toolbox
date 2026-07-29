import { AsyncAssetCache } from "./async-asset-cache.ts";
import type { Asset, Item } from "./types.ts";

type Invoke = <T = unknown>(cmd: string, args?: unknown) => Promise<T>;

export function createSvgAssetLoader(invoke: Invoke) {
  const sourcePreviews = new AsyncAssetCache(8, 24 * 1024 * 1024);
  const vectorPreviews = new AsyncAssetCache(32, 2 * 1024 * 1024);
  const editableSources = new AsyncAssetCache(4, 8 * 1024 * 1024);

  return {
    loadSource: (item: Item) => sourcePreviews.load(item.path, async () => {
      const asset = await invoke<Asset>("image_asset_url", {
        path: item.path,
      });
      return asset.url || "";
    }),
    loadVectorPreview: (item: Item) => {
      if (!item.outputPath) return Promise.resolve("");
      return vectorPreviews.load(item.outputPath, async () => {
        const asset = await invoke<Asset>("svg_asset_url", { path: item.outputPath });
        return asset.url || "";
      });
    },
    loadVectorText: (item: Item) => {
      if (!item.outputPath) return Promise.resolve("");
      return editableSources.load(item.outputPath, async () => {
        const asset = await invoke<Asset>("read_asset", { path: item.outputPath });
        return asset.text || "";
      });
    },
    cacheVector: (item: Item, svg: string) => {
      if (item.outputPath) editableSources.set(item.outputPath, svg);
    },
    invalidateVectorPreview: (item: Item) => {
      if (item.outputPath) vectorPreviews.delete(item.outputPath);
    },
  };
}
