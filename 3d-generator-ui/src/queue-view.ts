import { locale, t } from "./i18n";
import { ICONS } from "./layout";
import type { AppNodes, AppState, QueueItem, QueueState } from "./types";
import { newestSessionsFirst } from "../../ui-shared/creation-history-order";

type QueueViewOptions = {
  state: AppState;
  nodes: AppNodes;
  batchItems: (batchId: string) => QueueItem[];
  stripExtension: (name: string) => string;
  onSelect: (id: string) => void;
  onOpenReference: (item: QueueItem) => void;
  onRemove: (id: string) => void;
  onRename: (item: QueueItem, name: string) => void;
  onDelete: (item: QueueItem) => void;
  onThumbnailNeeded: (item: QueueItem) => void;
};

function queueStateLabel(value: QueueState) {
  return t(value === "running"
    ? "creating"
    : value === "done"
      ? "complete"
      : value === "failed"
        ? "failed"
        : "queued");
}

function itemQueueLabel(item: QueueItem) {
  if (item.historyId && item.state === "done") return t("savedResult");
  return item.state === "queued" && !item.submitted ? t("draft") : queueStateLabel(item.state);
}

export class ModelQueueView {
  private signature = "";
  private renamingId = "";
  private readonly rows = new Map<string, HTMLElement>();
  private readonly rowSignatures = new Map<string, string>();
  private readonly thumbnailObserver?: IntersectionObserver;

  constructor(private readonly options: QueueViewOptions) {
    if (typeof IntersectionObserver !== "undefined") {
      this.thumbnailObserver = new IntersectionObserver((entries) => {
        entries.filter((entry) => entry.isIntersecting).forEach((entry) => {
          this.thumbnailObserver?.unobserve(entry.target);
          const id = (entry.target as HTMLElement).dataset.itemId;
          const item = this.options.state.items.find((candidate) => candidate.id === id);
          if (item) this.options.onThumbnailNeeded(item);
        });
      }, { root: options.nodes.queueList, rootMargin: "96px" });
    }
  }

  dispose() {
    this.thumbnailObserver?.disconnect();
    this.rows.clear();
    this.rowSignatures.clear();
  }

  invalidate() {
    this.signature = "";
  }

  finishRename() {
    this.renamingId = "";
    this.invalidate();
  }

  render() {
    const { state, nodes } = this.options;
    const signature = JSON.stringify({
      locale: locale(),
      renamingId: this.renamingId,
      items: state.items.map((item) => [
        item.id,
        item.batchId,
        item.submitted,
        item.historyId || "",
        item.createdAtMs || 0,
        item.result?.outputName || "",
      ]),
    });
    if (signature !== this.signature) {
      this.signature = signature;
      this.reconcileRows();
      nodes.queueFooter.textContent =
        state.items.length ? t("jobsCount", { count: state.items.length }) : "";
    }
    this.syncRows();
  }

  private reconcileRows() {
    const { state, nodes, batchItems } = this.options;
    nodes.queueList.querySelector(".queue-empty")?.remove();
    nodes.queueList.querySelectorAll(".batch-label").forEach((label) => label.remove());
    const liveIds = new Set(state.items.map((item) => item.id));
    this.rows.forEach((row, id) => {
      if (liveIds.has(id)) return;
      this.thumbnailObserver?.unobserve(row.querySelector(".queue-thumb") || row);
      row.remove();
      this.rows.delete(id);
      this.rowSignatures.delete(id);
    });
    if (!state.items.length) {
      const empty = document.createElement("div");
      empty.className = "queue-empty";
      empty.innerHTML =
        `<span>${ICONS.image}</span><strong>${t("queueEmpty")}</strong><small>${t("queueEmptyDetail")}</small>`;
      nodes.queueList.append(empty);
      return;
    }
    const currentItems = state.items.filter((item) => !item.historyId);
    const presentationItems = newestSessionsFirst(state.items);
    const batchIds = [...new Set(currentItems.map((item) => item.batchId))];
    const showBatchLabels =
      batchIds.length > 1 || currentItems.some((item) => batchItems(item.batchId).length > 1);
    let previousBatchId = "";
    const fragment = document.createDocumentFragment();
    for (const item of presentationItems) {
      if (!item.historyId && showBatchLabels && item.batchId !== previousBatchId) {
        const batchHeader = document.createElement("div");
        batchHeader.className = "batch-label";
        batchHeader.textContent = t("batchLabel", {
          number: batchIds.indexOf(item.batchId) + 1,
          count: batchItems(item.batchId).length,
        });
        fragment.append(batchHeader);
        previousBatchId = item.batchId;
      }
      const rowSignature = JSON.stringify([
        locale(), item.historyId || "", item.result?.outputName || "", this.renamingId === item.id,
      ]);
      let row = this.rows.get(item.id);
      if (!row || this.rowSignatures.get(item.id) !== rowSignature) {
        const replacement = this.createRow(item);
        row?.replaceWith(replacement);
        row = replacement;
        this.rows.set(item.id, row);
        this.rowSignatures.set(item.id, rowSignature);
      }
      fragment.append(row);
    }
    nodes.queueList.append(fragment);
  }

  private createRow(item: QueueItem) {
    const { state } = this.options;
    const row = document.createElement("div");
    row.className = `queue-item ${item.id === state.selectedId ? "selected" : ""}`;
    row.dataset.state = item.state;
    row.dataset.itemId = item.id;
    const main = document.createElement("div");
    main.className = "queue-item-main";
    const thumb = document.createElement("button");
    thumb.type = "button";
    thumb.className = "queue-thumb";
    thumb.dataset.itemId = item.id;
    thumb.title = t("viewReference");
    thumb.setAttribute("aria-label", t("viewReference"));
    thumb.addEventListener("click", () => this.options.onOpenReference(item));
    const select = document.createElement("div");
    select.tabIndex = 0;
    select.setAttribute("role", "button");
    select.className = "queue-select";
    const copy = document.createElement("span");
    copy.className = "queue-copy";
    const strong = document.createElement("strong");
    strong.textContent = this.options.stripExtension(item.result?.outputName || item.name);
    const small = document.createElement("small");
    small.textContent = itemQueueLabel(item);
    if (this.renamingId === item.id && item.historyId) this.appendRenameInput(copy, small, item);
    else copy.append(strong, small);
    select.append(copy);
    select.addEventListener("click", () => this.options.onSelect(item.id));
    select.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        this.options.onSelect(item.id);
      }
    });
    main.append(thumb, select);
    const actions = document.createElement("span");
    actions.className = "queue-actions";
    if (item.historyId && item.state === "done") this.appendHistoryActions(actions, item);
    else this.appendRemoveAction(actions, item);
    row.append(main, actions);
    this.syncThumbnail(thumb, item);
    return row;
  }

  private appendRenameInput(copy: HTMLElement, small: HTMLElement, item: QueueItem) {
    const input = document.createElement("input");
    input.className = "queue-rename-input";
    input.value = this.options.stripExtension(item.result?.outputName || item.name);
    input.setAttribute("aria-label", t("renameResult"));
    input.addEventListener("click", (event) => event.stopPropagation());
    input.addEventListener("keydown", (event) => {
      event.stopPropagation();
      if (event.key === "Enter") this.options.onRename(item, input.value);
      else if (event.key === "Escape") {
        this.finishRename();
        this.render();
      }
    });
    input.addEventListener("blur", () => window.setTimeout(() => {
      if (this.renamingId === item.id) {
        this.finishRename();
        this.render();
      }
    }, 80));
    copy.append(input, small);
    window.setTimeout(() => {
      input.focus();
      input.select();
    });
  }

  private appendHistoryActions(actions: HTMLElement, item: QueueItem) {
    const rename = document.createElement("button");
    rename.type = "button";
    rename.innerHTML = ICONS.rename;
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
    remove.innerHTML = ICONS.trash;
    remove.title = t("deleteResult");
    remove.setAttribute("aria-label", t("deleteResult"));
    remove.addEventListener("click", () => this.options.onDelete(item));
    actions.append(rename, remove);
  }

  private appendRemoveAction(actions: HTMLElement, item: QueueItem) {
    const remove = document.createElement("button");
    remove.type = "button";
    remove.innerHTML = ICONS.close;
    remove.title = t("remove");
    remove.setAttribute("aria-label", t("remove"));
    remove.disabled = item.state === "running" || item.state === "done";
    remove.addEventListener("click", () => this.options.onRemove(item.id));
    actions.append(remove);
  }

  private syncRows() {
    const { state, nodes } = this.options;
    const rows = new Map(
      [...nodes.queueList.querySelectorAll<HTMLElement>(".queue-item[data-item-id]")]
        .map((row) => [row.dataset.itemId, row]),
    );
    state.items.forEach((item) => {
      const row = rows.get(item.id);
      if (!row) return;
      const selected = item.id === state.selectedId;
      row.classList.toggle("selected", selected);
      row.querySelector<HTMLElement>(".queue-select")
        ?.setAttribute("aria-current", selected ? "true" : "false");
      row.dataset.state = item.state;
      const label = row.querySelector<HTMLElement>(".queue-copy small");
      if (label) label.textContent = itemQueueLabel(item);
      const thumbnail = row.querySelector<HTMLButtonElement>(".queue-thumb");
      if (thumbnail) this.syncThumbnail(thumbnail, item);
      const remove = row.querySelector<HTMLButtonElement>(".queue-actions button");
      if (remove && !item.historyId) {
        remove.disabled = item.state === "running" || item.state === "done";
      }
    });
  }

  private syncThumbnail(thumbnail: HTMLButtonElement, item: QueueItem) {
    const current = thumbnail.querySelector<HTMLImageElement>("img")?.src;
    if (item.thumbnailUrl) {
      this.thumbnailObserver?.unobserve(thumbnail);
      if (current === item.thumbnailUrl) return;
      const image = document.createElement("img");
      image.alt = "";
      image.src = item.thumbnailUrl;
      thumbnail.replaceChildren(image);
      return;
    }
    if (!current) thumbnail.innerHTML = ICONS.image;
    if (this.thumbnailObserver) this.thumbnailObserver.observe(thumbnail);
    else this.options.onThumbnailNeeded(item);
  }
}
