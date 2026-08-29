import {
  canSubmitCreationSource,
  type CreationSourceProvenance,
} from "../../ui-shared/creation-source-provenance.ts";

export type StartImageArguments = Record<string, unknown> & {
  outputDir: string;
  prompt: string;
  imagePaths?: string[];
};

export function startImageArguments(
  referencePaths: string[],
  outputDir: string,
  prompt: string,
  provenance: CreationSourceProvenance,
): StartImageArguments {
  if (!canSubmitImageSelection(referencePaths, provenance)) {
    throw new Error("Image references must be imported by the current surface.");
  }
  const args: StartImageArguments = { outputDir, prompt };
  if (referencePaths.length) args.imagePaths = [...referencePaths];
  return args;
}

export function canSubmitImageSelection(
  referencePaths: string[],
  provenance: CreationSourceProvenance,
): boolean {
  return canSubmitCreationSource(provenance, referencePaths.length, true);
}

export class SurfaceSourceRegistry {
  private readonly jobs = new Map<string, string[]>();
  private readonly maximumEntries: number;

  constructor(maximumEntries = 192) {
    this.maximumEntries = maximumEntries;
  }

  remember(jobId: string, referencePaths: string[]): void {
    this.jobs.set(jobId, [...referencePaths]);
    while (this.jobs.size > Math.max(1, this.maximumEntries)) {
      const oldest = this.jobs.keys().next().value;
      if (!oldest) break;
      this.jobs.delete(oldest);
    }
  }

  references(jobId: string): string[] | undefined {
    const paths = this.jobs.get(jobId);
    return paths ? [...paths] : undefined;
  }
}

export type SubmissionTicket = Readonly<{
  id: string;
  sourceKey: string;
}>;

export class ExplicitSubmissionTracker {
  private readonly active = new Map<string, SubmissionTicket>();
  private readonly activeBySource = new Map<string, number>();
  private readonly latestBySource = new Map<string, string>();
  private readonly createId: () => string;
  private sequence = 0;

  constructor(createId: () => string = () => crypto.randomUUID()) {
    this.createId = createId;
  }

  begin(sourceKey: string): SubmissionTicket {
    const ticket = Object.freeze({
      id: `${++this.sequence}:${this.createId()}`,
      sourceKey,
    });
    this.active.set(ticket.id, ticket);
    this.activeBySource.set(sourceKey, (this.activeBySource.get(sourceKey) || 0) + 1);
    this.latestBySource.set(sourceKey, ticket.id);
    return ticket;
  }

  isLatest(ticket: SubmissionTicket): boolean {
    return this.latestBySource.get(ticket.sourceKey) === ticket.id;
  }

  activeIds(): string[] {
    return [...this.active.keys()].sort();
  }

  finish(ticket: SubmissionTicket): void {
    if (!this.active.delete(ticket.id)) return;
    const remaining = (this.activeBySource.get(ticket.sourceKey) || 1) - 1;
    if (remaining > 0) {
      this.activeBySource.set(ticket.sourceKey, remaining);
    } else {
      this.activeBySource.delete(ticket.sourceKey);
      this.latestBySource.delete(ticket.sourceKey);
    }
  }
}

export function selectionAfterSubmission(
  currentKey: string,
  ticket: SubmissionTicket,
  jobId: string,
  isLatest: boolean,
): string {
  return isLatest && currentKey === ticket.sourceKey ? jobId : currentKey;
}
