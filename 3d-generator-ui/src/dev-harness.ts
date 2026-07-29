import type { AppState, QueueItem, QueueState } from "./types";
import type { ModelViewer } from "./viewer";

type DevHarnessOptions = {
  state: AppState;
  viewer: ModelViewer;
  params: URLSearchParams;
  pathLeaf: (path: string) => string;
  updateUi: () => void;
  processQueue: () => void;
};

export class DevHarness {
  constructor(private readonly options: DevHarnessOptions) {}

  async loadModelPreview(modelUrl: string) {
    const { state, viewer, params, pathLeaf, updateUi } = this.options;
    try {
      const response = await fetch(modelUrl);
      if (!response.ok) throw new Error(`Preview model returned ${response.status}`);
      const objectUrl = URL.createObjectURL(await response.blob());
      const name = pathLeaf(modelUrl);
      const segmented = params.get("segmented") === "1";
      const item: QueueItem = {
        id: "dev_model",
        batchId: "dev_batch",
        path: modelUrl,
        sourceProvenance: "presentation",
        name,
        extension: "GLB",
        polycount: 5000,
        generationMode: "quality",
        autoSegment: segmented,
        submitted: true,
        state: "done",
        result: {
          stage: "done",
          progressText: "",
          outputPath: modelUrl,
          outputName: name,
          isSegmented: segmented,
          canSegment: false,
        },
      };
      state.items.push(item);
      state.selectedId = item.id;
      try {
        const stats = await viewer.setModel(objectUrl, segmented);
        if (stats) item.modelStats = stats;
      } finally {
        URL.revokeObjectURL(objectUrl);
      }
      state.displayedItemId = item.id;
      state.displayedModelPath = modelUrl;
      updateUi();
    } catch {
      state.selectedStatus = {
        stage: "failed",
        progressText: "Preview unavailable.",
        error: "invalid_result",
      };
      updateUi();
    }
  }

  loadBatchPreview() {
    const { state, params, updateUi } = this.options;
    const makeItem = (
      id: string,
      batchId: string,
      name: string,
      itemState: QueueState,
      submitted: boolean,
    ): QueueItem => ({
      id,
      batchId,
      path: name,
      sourceProvenance: "surface-import",
      name,
      extension: "PNG",
      generationMode: batchId === "batch_2" ? "fast" : "quality",
      polycount: batchId === "batch_2" ? 8_200 : 5_000,
      autoSegment: batchId === "batch_2",
      submitted,
      state: itemState,
    });
    state.items.push(
      makeItem("batch_1_a", "batch_1", "atrium-front.png", "running", true),
      makeItem("batch_1_b", "batch_1", "atrium-side.png", "running", true),
      makeItem("batch_2_a", "batch_2", "character-front.png", "queued", false),
      makeItem("batch_2_b", "batch_2", "character-side.png", "queued", false),
      makeItem("batch_2_c", "batch_2", "character-back.png", "queued", false),
    );
    if (params.get("history") === "1") {
      state.items.push({
        ...makeItem("history_a", "history_a", "clinic-reception.png", "done", true),
        sourceProvenance: "presentation",
        historyId: "history_a",
        createdAtMs: Date.now() - 60_000,
        result: {
          stage: "done",
          progressText: "",
          outputPath: "C:\\Models\\clinic-reception.glb",
          outputName: "clinic-reception.glb",
          isSegmented: true,
          canSegment: false,
        },
      }, {
        ...makeItem("history_b", "history_b", "lobby-chair.png", "done", true),
        sourceProvenance: "presentation",
        historyId: "history_b",
        createdAtMs: Date.now() - 120_000,
        result: {
          stage: "done",
          progressText: "",
          outputPath: "C:\\Models\\lobby-chair.glb",
          outputName: "lobby-chair.glb",
          isSegmented: false,
          canSegment: false,
        },
      });
    }
    state.selectedId = params.get("history") === "1" ? "history_a" : "batch_2_a";
    state.runningIds.add("batch_1_a");
    state.runningIds.add("batch_1_b");
    state.queueActive = true;
    state.items[0].operationStartedAt = Date.now() - 42_000;
    state.items[0].estimatedTotalMs = 120_000;
    state.items[0].displayedProgress = 0.38;
    state.selectedStatus = {
      jobId: "dev_running",
      stage: "generating",
      phase: "model_creation",
      progressText: "",
      runtimeStatus: "installed",
      progressRatio: 0.38,
      estimatedTotalMs: 120_000,
    };
    updateUi();
  }

  loadParallelHarness() {
    const { state, updateUi, processQueue } = this.options;
    const harness = { starts: [] as string[], active: 0, maxActive: 0, completed: 0 };
    const polls = new Map<string, number>();
    const syncHarness = () => {
      document.documentElement.dataset.parallelStarts = String(harness.starts.length);
      document.documentElement.dataset.parallelActive = String(harness.active);
      document.documentElement.dataset.parallelMax = String(harness.maxActive);
      document.documentElement.dataset.parallelCompleted = String(harness.completed);
    };
    window.__SGT_PARALLEL_TEST__ = harness;
    syncHarness();
    window.invoke = async <T>(cmd: string, args?: unknown): Promise<T> => {
      if (cmd === "start_job") {
        const jobId = `parallel_${harness.starts.length + 1}`;
        harness.starts.push(jobId);
        harness.active += 1;
        harness.maxActive = Math.max(harness.maxActive, harness.active);
        syncHarness();
        polls.set(jobId, 0);
        return {
          jobId,
          stage: "generating",
          progressText: "",
          runtimeStatus: "installed",
        } as T;
      }
      if (cmd === "job_statuses") {
        const statuses = [...polls].map(([jobId, previous]) => {
          const count = previous + 1;
          if (count < 2) {
            polls.set(jobId, count);
            return {
              jobId,
              stage: "generating",
              progressText: "",
              runtimeStatus: "installed",
              progressRatio: 0.5,
            };
          }
          polls.delete(jobId);
          harness.active -= 1;
          harness.completed += 1;
          syncHarness();
          return {
            jobId,
            stage: "done",
            progressText: "",
            runtimeStatus: "installed",
            isSegmented: false,
          };
        });
        return statuses as T;
      }
      if (cmd === "model_asset_url") throw new Error("No fixture model");
      return null as T;
    };
    const batchId = "parallel_batch";
    state.items.push(
      {
        id: "parallel_a",
        batchId,
        path: "parallel-a.png",
        sourceProvenance: "surface-import",
        name: "parallel-a.png",
        extension: "PNG",
        generationMode: "quality",
        polycount: 5_000,
        autoSegment: false,
        submitted: true,
        state: "queued",
      },
      {
        id: "parallel_b",
        batchId,
        path: "parallel-b.png",
        sourceProvenance: "surface-import",
        name: "parallel-b.png",
        extension: "PNG",
        generationMode: "fast",
        polycount: 5_000,
        autoSegment: false,
        submitted: true,
        state: "queued",
      },
    );
    state.selectedId = "parallel_a";
    updateUi();
    void processQueue();
  }
}
