import { invoke } from "@/lib/ipc";

export type StartupMilestone =
  | "frontend-module-evaluated"
  | "react-committed"
  | "first-visible-frame"
  | "projects-hydrated";

const reported = new Set<StartupMilestone>();

export function reportStartupMilestone(milestone: StartupMilestone) {
  if (reported.has(milestone)) return;
  reported.add(milestone);
  void invoke("report_startup_milestone", { milestone }).catch(() => {
    reported.delete(milestone);
  });
}
