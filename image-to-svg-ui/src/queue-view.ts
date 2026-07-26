import { t } from "./i18n";
import { SVG_ICONS } from "./layout";
import type { Asset, Item, Stage } from "./types";
import { VisiblePreviewScheduler } from "../../ui-shared/visible-preview-scheduler";

type QueueViewOptions = {
  queueList: HTMLElement;
  sourceThumb: HTMLElement;
  getItems: () => Item[];
  getSelectedId: () => string;
  stageLabel: (stage: Stage) => string;
  invoke: <T>(cmd: string, args?: unknown) => Promise<T>;
  onSelect: (item: Item) => void;
  onRename: (item: Item, name: string) => void;
  onDelete: (item: Item) => void;
};

export class SvgQueueView {
  private signature = "";
  private renamingId = "";
  private readonly scheduler: VisiblePreviewScheduler;

  constructor(private readonly options: QueueViewOptions) {
    this.scheduler = new VisiblePreviewScheduler(options.queueList, async (itemId) => {
      const item = options.getItems().find((candidate) => candidate.id === itemId);
      if (!item || item.thumbnailUrl || !item.path) return;
      item.thumbnailUrl = (await options.invoke<Asset>("read_image_preview", {
        path: item.path,
        maxEdge: 128,
      })).dataUrl;
      const row = [...options.queueList.querySelectorAll<HTMLElement>("[data-item-id]")]
        .find((candidate) => candidate.dataset.itemId === item.id);
      const thumb = row?.querySelector<HTMLElement>(".queue-thumb");
      if (thumb && item.thumbnailUrl) {
        const image = document.createElement("img");
        image.src = item.thumbnailUrl;
        image.alt = "";
        thumb.replaceChildren(image);
        if (row) row.dataset.previewReady = "true";
      }
      if (item.id === options.getSelectedId() && item.thumbnailUrl) {
        options.sourceThumb.innerHTML = `<img src="${item.thumbnailUrl}" alt="" />`;
      }
    });
  }

  holdPreviews(milliseconds: number) {
    this.scheduler.hold(milliseconds);
  }

  setInteractionActive(active: boolean) {
    this.scheduler.setInteractionActive(active);
  }

  invalidate() {
    this.signature = "";
  }

  finishRename() {
    this.renamingId = "";
    this.invalidate();
  }

  render() {
    const { getItems, getSelectedId, queueList } = this.options;
    const items = getItems();
    const selectedId = getSelectedId();
    const signature = JSON.stringify({
      renamingId: this.renamingId,
      items: items.map((item) => [
        item.id,
        item.batchId,
        item.name,
        item.outputName || "",
        item.historyId || "",
      ]),
    });
    if (signature !== this.signature) {
      this.signature = signature;
      queueList.replaceChildren();
      if (!items.length) {
        const empty = document.createElement("div");
        empty.className = "queue-empty";
        empty.textContent = t("emptyQueue");
        queueList.append(empty);
        this.scheduler.bind([]);
      } else {
        items.forEach((item) => queueList.append(this.createRow(item)));
        this.scheduleHydration();
      }
    }
    this.syncRows();
  }

  private createRow(item: Item) {
    const { getSelectedId, stageLabel } = this.options;
    const row = document.createElement("div");
    row.className = `queue-item ${item.id === getSelectedId() ? "selected" : ""}`;
    row.dataset.itemId = item.id;
    const main = document.createElement("div");
    main.className = "queue-item-main";
    main.tabIndex = 0;
    main.setAttribute("role", "button");
    const thumb = document.createElement("span");
    thumb.className = "queue-thumb";
    thumb.innerHTML = item.thumbnailUrl ? `<img src="${item.thumbnailUrl}" alt=""/>` : SVG_ICONS.image;
    const copy = document.createElement("span");
    copy.className = "queue-copy";
    const strong = document.createElement("strong");
    strong.textContent = item.outputName || item.name;
    const small = document.createElement("small");
    small.textContent = item.historyId ? t("savedResult") : stageLabel(item.stage);
    if (this.renamingId === item.id && item.historyId) this.appendRenameInput(copy, small, item);
    else copy.append(strong, small);
    main.append(thumb, copy);
    main.addEventListener("click", () => this.options.onSelect(item));
    main.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        main.click();
      }
    });
    const stateDot = document.createElement("i");
    stateDot.className = `queue-state state ${item.stage}`;
    const actions = document.createElement("span");
    actions.className = "queue-actions";
    if (item.historyId) this.appendHistoryActions(actions, item);
    row.append(main, stateDot, actions);
    return row;
  }

  private appendRenameInput(copy: HTMLElement, small: HTMLElement, item: Item) {
    const input = document.createElement("input");
    input.className = "queue-rename-input";
    input.value = (item.outputName || item.name).replace(/\.svg$/i, "");
    input.setAttribute("aria-label", t("renameResult"));
    input.addEventListener("click", (event) => event.stopPropagation());
    input.addEventListener("keydown", (event) => {
      event.stopPropagation();
      if (event.key === "Enter") this.options.onRename(item, input.value);
      else if (event.key === "Escape") {
        this.renamingId = "";
        this.invalidate();
        this.render();
      }
    });
    input.addEventListener("blur", () => window.setTimeout(() => {
      if (this.renamingId === item.id) {
        this.renamingId = "";
        this.invalidate();
        this.render();
      }
    }, 80));
    copy.append(input, small);
    window.setTimeout(() => {
      input.focus();
      input.select();
    });
  }

  private appendHistoryActions(actions: HTMLElement, item: Item) {
    const rename = document.createElement("button");
    rename.type = "button";
    rename.innerHTML = SVG_ICONS.rename;
    rename.title = t("renameResult");
    rename.setAttribute("aria-label", t("renameResult"));
    rename.addEventListener("click", () => {
      this.renamingId = item.id;
      this.invalidate();
      this.render();
    });
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "danger";
    remove.innerHTML = SVG_ICONS.trash;
    remove.title = t("deleteResult");
    remove.setAttribute("aria-label", t("deleteResult"));
    remove.addEventListener("click", () => this.options.onDelete(item));
    actions.append(rename, remove);
  }

  private syncRows() {
    const rows = new Map(
      [...this.options.queueList.querySelectorAll<HTMLElement>("[data-item-id]")]
        .map((row) => [row.dataset.itemId, row]),
    );
    this.options.getItems().forEach((item) => {
      const row = rows.get(item.id);
      if (!row) return;
      const selected = item.id === this.options.getSelectedId();
      row.classList.toggle("selected", selected);
      row.querySelector<HTMLElement>(".queue-item-main")
        ?.setAttribute("aria-current", selected ? "true" : "false");
      const label = row.querySelector<HTMLElement>(".queue-copy small");
      if (label) label.textContent = item.historyId ? t("savedResult") : this.options.stageLabel(item.stage);
      const stateDot = row.querySelector<HTMLElement>(".queue-state");
      if (stateDot) stateDot.className = `queue-state state ${item.stage}`;
    });
    this.scheduler.prioritize(this.options.getSelectedId());
  }

  private scheduleHydration() {
    const targets = [...this.options.queueList.querySelectorAll<HTMLElement>("[data-item-id]")]
      .flatMap((element) => {
        const key = element.dataset.itemId || "";
        const item = this.options.getItems().find((candidate) => candidate.id === key);
        return item && !item.thumbnailUrl && item.path ? [{ key, element }] : [];
      });
    const selectedId = this.options.getSelectedId();
    this.scheduler.bind(targets, selectedId ? [selectedId] : []);
  }
}
