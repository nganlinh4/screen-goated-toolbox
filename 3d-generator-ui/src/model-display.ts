import { LatestOnlyLane } from "./latest-only-lane.ts";
import type { ModelStats } from "./viewer";

type ModelAssetPayload = { url: string };
type Invoke = <T = unknown>(command: string, args?: unknown) => Promise<T>;

type ModelTarget = {
  cancelPendingModelLoad: () => void;
  setModel: (url: string, segmented: boolean) => Promise<ModelStats | null>;
  showIdle: () => void;
};

export type ModelDisplayOutcome =
  | { kind: "idle" }
  | { kind: "model"; stats: ModelStats };

export class ModelDisplayLane {
  private readonly lane = new LatestOnlyLane<ModelDisplayOutcome>();
  private readonly viewer: ModelTarget;
  private readonly invoke: Invoke;

  constructor(viewer: ModelTarget, invoke: Invoke) {
    this.viewer = viewer;
    this.invoke = invoke;
  }

  display(path: string | undefined, segmented: boolean): Promise<ModelDisplayOutcome | null> {
    return this.lane.run(
      async (signal) => {
        if (!path) {
          await this.releaseAsset();
          if (signal.aborted) throw new Error("Model display was superseded");
          this.viewer.showIdle();
          return { kind: "idle" };
        }

        const asset = await this.invoke<ModelAssetPayload>("model_asset_url", { path });
        if (signal.aborted) {
          await this.releaseAsset();
          throw new Error("Model display was superseded");
        }
        const cancelViewer = () => this.viewer.cancelPendingModelLoad();
        signal.addEventListener("abort", cancelViewer, { once: true });
        let shown = false;
        try {
          const stats = await this.viewer.setModel(asset.url, segmented);
          if (!stats || signal.aborted) throw new Error("Model display was superseded");
          shown = true;
          return { kind: "model", stats };
        } finally {
          signal.removeEventListener("abort", cancelViewer);
          if (!shown) await this.releaseAsset();
        }
      },
      () => undefined,
    );
  }

  dispose() {
    this.lane.invalidate();
    this.viewer.cancelPendingModelLoad();
    void this.releaseAsset();
  }

  private releaseAsset() {
    return this.invoke("release_model_asset").catch(() => undefined);
  }
}
