import { VisiblePreviewScheduler, type VisiblePreviewTarget } from "../../ui-shared/visible-preview-scheduler";

type PreviewSource = {
  thumbnail(path: string): Promise<string>;
  stage(path: string, maxEdge: number): Promise<string>;
};

export class ImagePreviewHydrator {
  private root: HTMLElement | undefined;
  private readonly thumbnails = new VisiblePreviewScheduler(
    null,
    (_key, element) => this.loadThumbnail(element),
  );
  private readonly stages = new VisiblePreviewScheduler(
    null,
    (_key, element) => this.loadStage(element as HTMLImageElement),
    "40px",
  );

  constructor(
    private readonly source: PreviewSource,
    private readonly fitNaturalFrame: (image: HTMLImageElement) => void,
  ) {}

  bind(root: HTMLElement, priorityPaths: string[]) {
    this.root = root;
    const priority = new Set(priorityPaths.filter(Boolean));
    const thumbnailTargets = [...root.querySelectorAll<HTMLElement>("[data-thumb]")]
      .map((element, index): VisiblePreviewTarget => ({
        key: `thumb:${index}:${element.dataset.thumb || ""}`,
        element,
      }));
    this.thumbnails.bind(
      thumbnailTargets,
      thumbnailTargets
        .filter(({ element }) => priority.has(element.dataset.thumb || ""))
        .map(({ key }) => key),
    );

    const stageTargets = [...root.querySelectorAll<HTMLImageElement>("[data-stage-path]")]
      .map((element, index): VisiblePreviewTarget => ({
        key: `stage:${index}:${element.dataset.stageEdge || ""}:${element.dataset.stagePath || ""}`,
        element,
      }));
    this.stages.bind(
      stageTargets,
      stageTargets
        .filter(({ element }) => element.hasAttribute("data-fit-anchor")
          || element.classList.contains("multi-output-image"))
        .map(({ key }) => key),
    );
  }

  private async loadThumbnail(element: HTMLElement) {
    const path = element.dataset.thumb;
    if (!path) return;
    const preview = await this.source.thumbnail(path);
    for (const current of this.root?.querySelectorAll<HTMLElement>("[data-thumb]") ?? []) {
      if (current.dataset.thumb !== path) continue;
      current.style.backgroundImage = `url("${preview}")`;
      current.replaceChildren();
      current.dataset.previewReady = "true";
    }
  }

  private async loadStage(element: HTMLImageElement) {
    const path = element.dataset.stagePath;
    if (!path) return;
    const maxEdge = Number(element.dataset.stageEdge) || 1_600;
    const preview = await this.source.stage(path, maxEdge);
    for (const current of this.root?.querySelectorAll<HTMLImageElement>("[data-stage-path]") ?? []) {
      if (current.dataset.stagePath !== path
        || (Number(current.dataset.stageEdge) || 1_600) !== maxEdge) continue;
      if (current.hasAttribute("data-fit-anchor")) {
        current.addEventListener("load", () => this.fitNaturalFrame(current), { once: true });
      }
      current.src = preview;
      current.removeAttribute("data-preview-pending");
      current.dataset.previewReady = "true";
    }
  }
}
