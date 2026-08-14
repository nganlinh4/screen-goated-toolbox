export class ExportCancellationGeneration {
  private generation = 0;

  begin(): number {
    this.generation += 1;
    return this.generation;
  }

  cancel(): void {
    this.generation += 1;
  }

  isCurrent(generation: number): boolean {
    return generation === this.generation;
  }
}

export class ExportCancelledError extends Error {
  constructor() {
    super("Export cancelled");
    this.name = "ExportCancelledError";
  }
}

export function throwIfExportCancelled(
  isCancelled: (() => boolean) | undefined,
): void {
  if (isCancelled?.()) throw new ExportCancelledError();
}

export type CancellablePreparationResult<T> =
  | { cancelled: true }
  | { cancelled: false; value: T };

export async function startAfterCancellablePreparation<TPrepared, TResult>(
  cancellation: ExportCancellationGeneration,
  generation: number,
  prepare: () => Promise<TPrepared>,
  start: (prepared: TPrepared) => Promise<TResult>,
): Promise<CancellablePreparationResult<TResult>> {
  const prepared = await prepare();
  if (!cancellation.isCurrent(generation)) return { cancelled: true };
  const value = await start(prepared);
  return cancellation.isCurrent(generation)
    ? { cancelled: false, value }
    : { cancelled: true };
}
