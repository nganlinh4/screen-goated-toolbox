export interface ProjectWriteIntent {
  projectId: string;
  revision: number;
}

/**
 * Serializes project writes and lets editor saves declare "latest state wins".
 * Metadata-only mutations such as renames still share the queue, but do not
 * invalidate editor intents because they merge into the latest stored record.
 */
export class ProjectWriteCoordinator {
  private readonly chains = new Map<string, Promise<void>>();
  private readonly editorRevisions = new Map<string, number>();

  createEditorIntent(projectId: string): ProjectWriteIntent {
    const revision = (this.editorRevisions.get(projectId) ?? 0) + 1;
    this.editorRevisions.set(projectId, revision);
    return { projectId, revision };
  }

  invalidateEditorIntents(projectId: string): void {
    this.editorRevisions.set(
      projectId,
      (this.editorRevisions.get(projectId) ?? 0) + 1,
    );
  }

  isLatest(intent: ProjectWriteIntent): boolean {
    return (
      intent.projectId.length > 0 &&
      this.editorRevisions.get(intent.projectId) === intent.revision
    );
  }

  enqueue<T>(
    projectId: string,
    operation: () => Promise<T>,
    intent?: ProjectWriteIntent,
  ): Promise<{ applied: boolean; value?: T }> {
    const previous = this.chains.get(projectId) ?? Promise.resolve();
    const result = previous
      .catch(() => undefined)
      .then(async () => {
        if (intent && !this.isLatest(intent)) {
          return { applied: false };
        }
        return { applied: true, value: await operation() };
      });
    const tail = result.then(
      () => undefined,
      () => undefined,
    );
    this.chains.set(projectId, tail);
    void tail.finally(() => {
      if (this.chains.get(projectId) === tail) {
        this.chains.delete(projectId);
      }
    });
    return result;
  }
}
