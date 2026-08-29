import { VisiblePreviewScheduler, type VisiblePreviewTarget } from "../../ui-shared/visible-preview-scheduler";

type PreviewSource = {
  stage(path: string, maxEdge: number): Promise<string>;
};

export class ImagePreviewHydrator {
  private root: HTMLElement | undefined;
  private readonly stages = new VisiblePreviewScheduler(
    null,
    (_key, element) => this.loadStage(element as HTMLImageElement),
    "40px",
  );

  constructor(
    private readonly source: PreviewSource,
    private readonly fitNaturalFrame: (image: HTMLImageElement) => void,
  ) {}

  bind(root: HTMLElement) {
    this.root = root;
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

  setInteractionActive(active: boolean, settleMilliseconds = 220) {
    this.stages.setInteractionActive(active, settleMilliseconds);
  }

  hold(milliseconds: number) {
    this.stages.hold(milliseconds);
  }

  private async loadStage(element: HTMLImageElement) {
    const path = element.dataset.stagePath;
    if (!path) return;
    const maxEdge = Number(element.dataset.stageEdge) || 1_600;
    const preview = await this.source.stage(path, maxEdge);
    for (const current of this.root?.querySelectorAll<HTMLImageElement>("[data-stage-path]") ?? []) {
      if (current.dataset.stagePath !== path
        || (Number(current.dataset.stageEdge) || 1_600) !== maxEdge) continue;
      current.addEventListener("load", () => {
        if (current.hasAttribute("data-fit-anchor")) this.fitNaturalFrame(current);
        current.removeAttribute("data-preview-pending");
        current.dataset.previewReady = "true";
      }, { once: true });
      current.src = preview;
    }
  }
}
