import { useEffect, useRef, useState } from "react";
import type { Project } from "@/types/video";
import { normalizeProjectLimit, projectManager } from "@/lib/projectManager";
import { selectProjectsBeyondLimit } from "@/lib/projectRetention";
import { notifyUserError } from "@/lib/userNotifications";
import { useSynchronizedState } from "@/hooks/useSynchronizedState";

/** Keyboard and assistive-technology steps commit once the value stops changing. */
export const PROJECT_LIMIT_SETTLE_MS = 800;

export interface PendingLimitReduction {
  limit: number;
  projectIds: string[];
}

interface ProjectLimitControlOptions {
  projects: Pick<Project, "id" | "createdAt" | "lastModified">[];
  currentProjectId?: string | null;
  onProjectsChange: () => void;
  onDeleteProject: (id: string) => Promise<void>;
}

/**
 * Draft/commit state for the project limit slider. Intermediate slider values
 * never reach storage, and a committed limit that would remove projects waits
 * for an explicit confirmation that names how many will be deleted.
 */
export function useProjectLimitControl({
  projects,
  currentProjectId,
  onProjectsChange,
  onDeleteProject,
}: ProjectLimitControlOptions) {
  const [draftLimit, setDraftLimit, draftLimitRef] = useSynchronizedState(() => projectManager.getLimit());
  const [pendingReduction, setPendingReduction] = useState<PendingLimitReduction | null>(null);
  const pointerActiveRef = useRef(false);
  // pointerup, blur and the settle timer can all follow one adjustment; a value
  // already being saved or confirmed must not start a second save.
  const pendingLimitRef = useRef<number | null>(null);
  const settleTimerRef = useRef<number | null>(null);
  const latestRef = useRef({ projects, currentProjectId, onProjectsChange, onDeleteProject });

  useEffect(() => {
    latestRef.current = { projects, currentProjectId, onProjectsChange, onDeleteProject };
  });

  const clearSettleTimer = () => {
    if (settleTimerRef.current !== null) window.clearTimeout(settleTimerRef.current);
    settleTimerRef.current = null;
  };

  const restoreSavedLimit = () => {
    pendingLimitRef.current = null;
    setDraftLimit(projectManager.getLimit());
  };

  const saveLimit = async (value: number) => {
    try {
      await projectManager.setLimit(value);
      latestRef.current.onProjectsChange();
    } catch (error) {
      setDraftLimit(projectManager.getLimit());
      notifyUserError("settingsSaveFailed", error);
    } finally {
      if (pendingLimitRef.current === value) pendingLimitRef.current = null;
    }
  };

  const commitLimit = () => {
    clearSettleTimer();
    pointerActiveRef.current = false;
    const value = draftLimitRef.current;
    if (value === projectManager.getLimit() || value === pendingLimitRef.current) return;
    pendingLimitRef.current = value;
    const { projects: current, currentProjectId: activeId } = latestRef.current;
    const removed = selectProjectsBeyondLimit(current, value, activeId);
    if (removed.length > 0) {
      setPendingReduction({ limit: value, projectIds: removed.map((project) => project.id) });
      return;
    }
    void saveLimit(value);
  };
  const commitLimitRef = useRef(commitLimit);
  useEffect(() => {
    commitLimitRef.current = commitLimit;
  });

  useEffect(() => {
    const syncLimit = (event: MessageEvent) => {
      if (event.data?.type === "sr-set-settings"
        && typeof event.data.projectLimit === "number"
        && !pointerActiveRef.current
        && pendingLimitRef.current === null) {
        clearSettleTimer();
        setDraftLimit(normalizeProjectLimit(event.data.projectLimit));
      }
    };
    window.addEventListener("message", syncLimit);
    return () => {
      window.removeEventListener("message", syncLimit);
      clearSettleTimer();
    };
  }, [setDraftLimit]);

  const confirmReduction = async () => {
    const reduction = pendingReduction;
    setPendingReduction(null);
    if (!reduction) return;
    const { onDeleteProject: deleteProject, onProjectsChange: refresh } = latestRef.current;
    for (const id of reduction.projectIds) await deleteProject(id);
    const remaining = new Set((await projectManager.getProjects()).map((project) => project.id));
    if (reduction.projectIds.some((id) => remaining.has(id))) {
      // A failed delete keeps the old limit, so retention never picks an unnamed project.
      restoreSavedLimit();
      refresh();
      return;
    }
    await saveLimit(reduction.limit);
  };

  const cancelReduction = () => {
    setPendingReduction(null);
    restoreSavedLimit();
  };

  const sliderHandlers = {
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => {
      setDraftLimit(Number(event.target.value));
      if (pointerActiveRef.current) return;
      clearSettleTimer();
      settleTimerRef.current = window.setTimeout(
        () => commitLimitRef.current(),
        PROJECT_LIMIT_SETTLE_MS,
      );
    },
    onPointerDown: () => {
      clearSettleTimer();
      pointerActiveRef.current = true;
    },
    onPointerUp: commitLimit,
    onPointerCancel: () => {
      clearSettleTimer();
      pointerActiveRef.current = false;
      if (pendingLimitRef.current === null) setDraftLimit(projectManager.getLimit());
    },
    onBlur: commitLimit,
  };

  return { draftLimit, sliderHandlers, pendingReduction, confirmReduction, cancelReduction };
}
