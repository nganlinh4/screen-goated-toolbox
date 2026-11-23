import type { Item, JobStatus, Stage } from "./types";
import { releaseDispatchClaim } from "./queue-dispatch.ts";
import { svgStatusChangesPresentation } from "./poll-presentation.ts";
import { creationJobIsTerminal } from "../../ui-shared/creation-poll-state.ts";

export type SvgInputSnapshot = Readonly<Pick<Item, "id" | "path" | "name" | "model" | "backgroundMode" | "outputDir">>;
type Options = {
  items: () => Item[];
  invoke: <T>(command: string, args?: unknown) => Promise<T>;
  validate: (input: SvgInputSnapshot, signal: AbortSignal, current: () => boolean) => Promise<boolean>;
  accepted: (item: Item, status: JobStatus) => void;
  render: () => void;
  failedText: () => string;
  unconfirmedText: () => string;
};
type Attempt = { input: SvgInputSnapshot; abort: AbortController; starting: boolean };
const working = (stage: Stage) => stage !== "draft" && !creationJobIsTerminal(stage);

export class SvgSubmissionController {
  private readonly options: Options;
  private readonly attempts = new Map<Item, Attempt>();
  private readonly cancellation = new WeakSet<Item>();
  private readonly cancelling = new Map<Item, Promise<void>>();
  private disposed = false;

  constructor(options: Options) { this.options = options; }

  private owns(item: Item, attempt: Attempt): boolean {
    const snapshot = attempt.input;
    return !this.disposed && this.attempts.get(item) === attempt && this.options.items().includes(item)
      && item.id === snapshot.id && item.path === snapshot.path && item.model === snapshot.model
      && item.backgroundMode === snapshot.backgroundMode && item.outputDir === snapshot.outputDir;
  }

  async dispatch(item: Item): Promise<void> {
    if (this.disposed || this.attempts.has(item) || item.stage !== "queued" || !item.submitted || item.jobId) return;
    const attempt: Attempt = { input: Object.freeze({ id:item.id,path:item.path,name:item.name,
      model:item.model,backgroundMode:item.backgroundMode,outputDir:item.outputDir }),
      abort:new AbortController(),starting:false };
    this.attempts.set(item, attempt);
    const current = () => this.owns(item, attempt) && !attempt.abort.signal.aborted && item.stage === "queued";
    try {
      const valid = await this.options.validate(attempt.input, attempt.abort.signal, current);
      if (!current()) return;
      if (!valid) { releaseDispatchClaim(item); item.stage = "draft"; return; }
      attempt.starting = true;
      const status = await this.options.invoke<JobStatus>("start_job", {
        imagePath:attempt.input.path,outputDir:attempt.input.outputDir,
        model:attempt.input.model,backgroundMode:attempt.input.backgroundMode,
      });
      if (!status || typeof status.jobId !== "string" || !status.jobId || typeof status.stage !== "string") throw new Error(this.options.failedText());
      if (!this.owns(item, attempt)) {
        if (!creationJobIsTerminal(status.stage)) await this.options.invoke("cancel_job", {jobId:status.jobId});
        return;
      }
      item.jobId = status.jobId;
      if (attempt.abort.signal.aborted || this.cancellation.has(item)) {
        if (creationJobIsTerminal(status.stage)) {
          if (this.applyStatus(item, status)) this.options.accepted(item, status);
        } else await this.cancelNative(item, status.jobId);
      } else if (this.applyStatus(item, status)) this.options.accepted(item, status);
    } catch {
      if (this.owns(item, attempt)) {
        if (this.cancellation.has(item) && item.jobId) {
          item.stage = "cancelling";
          item.error = this.options.unconfirmedText();
        } else if (!attempt.abort.signal.aborted || attempt.starting) {
          releaseDispatchClaim(item);
          item.stage = "failed";
          item.error = attempt.abort.signal.aborted ? this.options.unconfirmedText() : this.options.failedText();
        }
      }
    } finally {
      if (!this.disposed && this.attempts.get(item) === attempt && this.options.items().includes(item)
        && item.id === attempt.input.id && item.stage === "queued" && !item.jobId && !this.owns(item, attempt)) {
        releaseDispatchClaim(item);
        item.stage = "draft";
      }
      if (this.attempts.get(item) === attempt) this.attempts.delete(item);
      if (!this.disposed) this.options.render();
    }
  }

  async cancel(item: Item): Promise<void> {
    if (this.disposed || !this.options.items().includes(item) || !working(item.stage)) return;
    const attempt = this.attempts.get(item);
    const nativePending = Boolean(item.jobId || attempt?.starting);
    if (nativePending) this.cancellation.add(item);
    attempt?.abort.abort();
    item.stage = nativePending ? "cancelling" : "cancelled";
    item.error = undefined;
    this.options.render();
    if (item.jobId) await this.cancelNative(item, item.jobId);
  }

  private async cancelNative(item: Item, jobId: string): Promise<void> {
    const previous = this.cancelling.get(item);
    if (previous) return previous;
    const operation = (async () => {
      try {
        const statuses = await this.options.invoke<JobStatus[]>("cancel_job", {jobId});
        const status = Array.isArray(statuses) ? statuses.find(value => value.jobId === jobId) : undefined;
        if (!status) throw new Error(this.options.unconfirmedText());
        if (this.applyStatus(item, status) && creationJobIsTerminal(status.stage)) this.options.accepted(item, status);
      } catch {
        if (!this.disposed && this.options.items().includes(item) && item.jobId === jobId && !creationJobIsTerminal(item.stage)) {
          item.stage = "cancelling";
          item.error = this.options.unconfirmedText();
        }
        throw new Error(this.options.unconfirmedText());
      } finally { if (!this.disposed) this.options.render(); }
    })();
    this.cancelling.set(item, operation);
    try { await operation; }
    finally { if (this.cancelling.get(item) === operation) this.cancelling.delete(item); }
  }

  applyStatus(item: Item, incoming: JobStatus): { stageChanged: boolean; presentationChanged: boolean } | null {
    if (this.disposed || !this.options.items().includes(item) || item.jobId !== incoming.jobId || creationJobIsTerminal(item.stage)) return null;
    const status = this.cancellation.has(item) && working(incoming.stage)
      ? {...incoming,stage:"cancelling" as const,progressKey:undefined} : incoming;
    const change = {stageChanged:item.stage !== status.stage,presentationChanged:svgStatusChangesPresentation(item,status)};
    item.stage = status.stage;
    item.progress = status.progressRatio;
    item.progressText = status.progressText;
    item.outputPath = status.outputPath;
    item.outputName = status.outputName;
    item.error = status.error;
    item.progressKey = status.progressKey;
    item.phase = status.phase;
    if (creationJobIsTerminal(status.stage)) this.cancellation.delete(item);
    return change;
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    for (const attempt of this.attempts.values()) attempt.abort.abort();
  }
}
