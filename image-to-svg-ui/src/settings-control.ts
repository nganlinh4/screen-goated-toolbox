import type { BackgroundMode, Item, Model } from "./types";

type Options = {
  getItems: () => Item[];
  getSelected: () => Item | undefined;
  render: () => void;
};

export class SvgSettingsControl {
  model: Model = "simple";
  backgroundMode: BackgroundMode = "opaque";
  private readonly options: Options;

  constructor(options: Options) {
    this.options = options;
    document.querySelectorAll<HTMLButtonElement>("[data-model]").forEach((button) => {
      button.addEventListener("click", () => {
        this.model = button.dataset.model as Model;
        this.updateDraftBatch((item) => item.model = this.model);
      });
    });
    document.querySelectorAll<HTMLButtonElement>("[data-background]").forEach((button) => {
      button.addEventListener("click", () => {
        this.backgroundMode = normalizeBackgroundMode(button.dataset.background);
        this.updateDraftBatch((item) => item.backgroundMode = this.backgroundMode);
      });
    });
  }

  sync(item?: Item) {
    const disabled = Boolean(item && item.stage !== "draft");
    document.querySelectorAll<HTMLButtonElement>("[data-model]").forEach((button) => {
      const active = (item?.model || this.model) === button.dataset.model;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", String(active));
      button.disabled = disabled;
    });
    document.querySelectorAll<HTMLButtonElement>("[data-background]").forEach((button) => {
      const active = (item?.backgroundMode || this.backgroundMode) === button.dataset.background;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", String(active));
      button.disabled = disabled;
    });
  }

  private updateDraftBatch(update: (item: Item) => void) {
    const selected = this.options.getSelected();
    if (selected?.stage === "draft") {
      this.options.getItems()
        .filter((item) => item.batchId === selected.batchId && item.stage === "draft")
        .forEach(update);
    }
    this.options.render();
  }
}

export function normalizeBackgroundMode(value?: string): BackgroundMode {
  if (value === "auto" || value === "transparent") return value;
  return "opaque";
}
