import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProjectsView } from "@/components/ProjectsView";
import type { Project } from "@/types/video";

vi.mock("@/lib/projectManager", () => ({
  projectManager: {
    getLimit: () => 50,
    setLimit: vi.fn(),
  },
}));

vi.mock("@/components/dialogs", () => ({
  ConfirmDialog: () => null,
}));

const project = {
  id: "project-1",
  name: "Accessible project",
  createdAt: 1,
  lastModified: 1,
  duration: 5,
  segment: { trimStart: 0, trimEnd: 5, trimSegments: [] },
  backgroundConfig: {
    scale: 1,
    borderRadius: 0,
    backgroundType: "solid",
  },
  mousePositions: [],
} as Project;

describe("ProjectsView accessibility", () => {
  it("exposes project cards and compact controls with native semantics", () => {
    const onLoadProject = vi.fn();
    render(
      <ProjectsView
        projects={[project]}
        onLoadProject={onLoadProject}
        onProjectsChange={() => undefined}
        onClearProjects={async () => undefined}
        onDeleteProject={async () => undefined}
        onRenameProject={async () => undefined}
        onClose={() => undefined}
      />,
    );

    const projectButton = screen.getByRole("button", {
      name: "Accessible project",
    });
    expect(screen.getByRole("slider", { name: "Max" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: "Delete project: Accessible project",
      }),
    ).toBeInTheDocument();

    fireEvent.click(projectButton);
    expect(onLoadProject).toHaveBeenCalledWith("project-1");
  });
});
