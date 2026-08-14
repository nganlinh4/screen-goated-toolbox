import { describe, expect, it, vi } from "vitest";
import {
  clearProjectsAfterClosingActive,
  deleteProjectAfterClosingActive,
} from "@/lib/projectLifecycleActions";

describe("project lifecycle actions", () => {
  it("does not delete an active project when the editor cannot close", async () => {
    const closeProject = vi.fn(async () => false);
    const deleteProject = vi.fn(async () => undefined);
    const reloadProjects = vi.fn(async () => undefined);

    await expect(deleteProjectAfterClosingActive("active", "active", {
      closeProject,
      deleteProject,
      reloadProjects,
    })).resolves.toBe(false);
    expect(deleteProject).not.toHaveBeenCalled();
    expect(reloadProjects).not.toHaveBeenCalled();
  });

  it("closes the editor before deleting the active project", async () => {
    const order: string[] = [];
    await deleteProjectAfterClosingActive("active", "active", {
      closeProject: async () => { order.push("close"); return true; },
      deleteProject: async () => { order.push("delete"); },
      reloadProjects: async () => { order.push("reload"); },
    });
    expect(order).toEqual(["close", "delete", "reload"]);
  });

  it("does not clear projects when closing the active editor fails", async () => {
    const deleteProject = vi.fn(async () => undefined);
    const result = await clearProjectsAfterClosingActive(
      ["active", "other"],
      "active",
      {
        closeProject: async () => false,
        deleteProject,
        reloadProjects: async () => undefined,
      },
    );
    expect(result).toBe(false);
    expect(deleteProject).not.toHaveBeenCalled();
  });
});
