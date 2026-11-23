export type CreationPollJob = { jobId: string; stage: string };

export function creationJobIsTerminal(stage: string): boolean {
  return ["done", "failed", "cancelled"].includes(stage);
}

export type CreationPollSnapshot<T> = {
  jobs: T[];
  previousStages: ReadonlyMap<string, string>;
  reachedTerminal: boolean;
};

export class CreationPollState<T extends CreationPollJob> {
  private revision = 0;
  private pending = false;
  private stages = new Map<string, string>();
  private readonly readPending: () => Promise<boolean>;
  private readonly readJobs: () => Promise<T[]>;

  constructor(
    readPending: () => Promise<boolean>,
    readJobs: () => Promise<T[]>,
  ) {
    this.readPending = readPending;
    this.readJobs = readJobs;
  }

  hasWork(busy: boolean): boolean {
    return busy || this.pending;
  }

  async refresh(): Promise<CreationPollSnapshot<T> | undefined> {
    const revision = ++this.revision;
    const [pending, jobs] = await Promise.all([this.readPending(), this.readJobs()]);
    if (revision !== this.revision) return undefined;
    const previousStages = this.stages;
    const reachedTerminal = jobs.some((job) =>
      creationJobIsTerminal(job.stage) && previousStages.get(job.jobId) !== job.stage);
    this.pending = pending;
    this.stages = new Map(jobs.map((job) => [job.jobId, job.stage]));
    return { jobs, previousStages, reachedTerminal };
  }
}
