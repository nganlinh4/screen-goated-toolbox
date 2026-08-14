export interface ProjectDeletionActions {
  closeProject: () => Promise<boolean>;
  deleteProject: (projectId: string) => Promise<void>;
  reloadProjects: () => Promise<unknown>;
}

export async function deleteProjectAfterClosingActive(
  projectId: string,
  activeProjectId: string | null,
  actions: ProjectDeletionActions,
): Promise<boolean> {
  if (projectId === activeProjectId && !(await actions.closeProject())) {
    return false;
  }
  await actions.deleteProject(projectId);
  await actions.reloadProjects();
  return true;
}

export async function clearProjectsAfterClosingActive(
  projectIds: readonly string[],
  activeProjectId: string | null,
  actions: ProjectDeletionActions,
): Promise<boolean> {
  if (activeProjectId && !(await actions.closeProject())) return false;
  for (const projectId of projectIds) {
    await actions.deleteProject(projectId);
  }
  await actions.reloadProjects();
  return true;
}
