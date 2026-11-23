import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectsView } from "@/components/ProjectsView";
import { projectManager } from "@/lib/projectManager";
import type { Project } from "@/types/video";
import en from "@/i18n/en";
import { PROJECT_LIMIT_SETTLE_MS } from "@/components/projectsViewLimit";

vi.mock("@/lib/projectManager", () => ({
  projectManager: {
    getLimit: () => 50,
    setLimit: vi.fn(async () => undefined),
  },
}));

vi.mock("@/components/dialogs", () => ({
  ConfirmDialog: ({ show, confirmLabel, onConfirm }: {
    show: boolean; confirmLabel: string; onConfirm: () => void;
  }) => show ? <button type="button" onClick={onConfirm}>{confirmLabel}</button> : null,
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
  afterEach(() => { vi.useRealTimers(); });

  it('does not commit a cancelled draft when blur runs before React restores the input', () => {
    const setLimit = vi.mocked(projectManager.setLimit);
    setLimit.mockClear();
    render(<ProjectsView projects={[project]} onLoadProject={() => undefined}
      onProjectsChange={() => undefined} onClearProjects={async () => undefined}
      onDeleteProject={async () => undefined} onRenameProject={async () => undefined}
      onClose={() => undefined} />);
    const slider = screen.getByRole('slider', { name: 'Max' });
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: '10' } });
    act(() => {
      fireEvent.pointerCancel(slider);
      fireEvent.blur(slider);
    });
    expect(setLimit).not.toHaveBeenCalled();
    expect(slider).toHaveValue('50');
  });
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
        name: en.deleteProjectNamedLabel.replace("{name}", "Accessible project"),
      }),
    ).toBeInTheDocument();

    fireEvent.click(projectButton);
    expect(onLoadProject).toHaveBeenCalledWith("project-1");
  });

  it("commits only the final slider value after a pointer drag", async () => {
    const change = vi.fn();
    const setLimit = vi.mocked(projectManager.setLimit);
    setLimit.mockClear();
    render(
      <ProjectsView projects={[project]} onLoadProject={() => undefined}
        onProjectsChange={change} onClearProjects={async () => undefined}
        onDeleteProject={async () => undefined} onRenameProject={async () => undefined}
        onClose={() => undefined} />,
    );
    const slider = screen.getByRole("slider", { name: "Max" });
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "10" } });
    fireEvent.change(slider, { target: { value: "40" } });
    expect(setLimit).not.toHaveBeenCalled();
    expect(change).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    expect(setLimit).toHaveBeenCalledExactlyOnceWith(40);
    await vi.waitFor(() => expect(change).toHaveBeenCalledOnce());
  });

  it("discards a cancelled slider drag and commits keyboard changes once they settle", () => {
    const setLimit = vi.mocked(projectManager.setLimit);
    setLimit.mockClear();
    vi.useFakeTimers();
    render(
      <ProjectsView projects={[project]} onLoadProject={() => undefined}
        onProjectsChange={() => undefined} onClearProjects={async () => undefined}
        onDeleteProject={async () => undefined} onRenameProject={async () => undefined}
        onClose={() => undefined} />,
    );
    const slider = screen.getByRole("slider", { name: "Max" });
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "10" } });
    fireEvent.pointerCancel(slider);
    expect(slider).toHaveValue("50");
    expect(setLimit).not.toHaveBeenCalled();
    fireEvent.change(slider, { target: { value: "49" } });
    fireEvent.keyUp(slider, { key: "ArrowLeft" });
    fireEvent.change(slider, { target: { value: "48" } });
    fireEvent.keyUp(slider, { key: "ArrowLeft" });
    act(() => { vi.advanceTimersByTime(PROJECT_LIMIT_SETTLE_MS - 1); });
    expect(setLimit).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(1); });
    expect(setLimit).toHaveBeenCalledExactlyOnceWith(48);
  });

  it("explains when retained projects exceed the configured limit", () => {
    const projects = Array.from({ length: 51 }, (_, index) => ({
      ...project, id: `retained-${index}`, name: `Project ${index}`,
    }));
    render(
      <ProjectsView projects={projects} onLoadProject={() => undefined}
        onProjectsChange={() => undefined} onClearProjects={async () => undefined}
        onDeleteProject={async () => undefined} onRenameProject={async () => undefined}
        onClose={() => undefined} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("preserve media");
  });

  function renderView(overrides: Partial<Parameters<typeof ProjectsView>[0]> = {}) {
    const props = {
      projects: [project], onLoadProject: vi.fn(), onProjectsChange: vi.fn(),
      onClearProjects: vi.fn(async () => undefined), onDeleteProject: vi.fn(async () => undefined),
      onRenameProject: vi.fn(async () => undefined), onClose: vi.fn(), ...overrides,
    };
    render(<ProjectsView {...props} />);
    return props;
  }

  it("asks for confirmation before deleting a single project", async () => {
    const props = renderView();
    fireEvent.click(screen.getByRole("button", { name: en.deleteProjectNamedLabel.replace("{name}", "Accessible project") }));
    expect(props.onDeleteProject).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(props.onDeleteProject).toHaveBeenCalledExactlyOnceWith("project-1"));
  });

  it("renames from the keyboard and writes once when Enter is followed by blur", async () => {
    const props = renderView();
    fireEvent.click(screen.getByRole("button", { name: "Rename: Accessible project" }));
    const input = screen.getByDisplayValue("Accessible project");
    fireEvent.change(input, { target: { value: "Renamed" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.blur(input);
    await vi.waitFor(() => expect(props.onRenameProject).toHaveBeenCalledExactlyOnceWith("project-1", "Renamed"));
  });

  it("cancels a rename with Escape without closing Projects or writing", () => {
    const props = renderView();
    fireEvent.click(screen.getByRole("button", { name: "Rename: Accessible project" }));
    const input = screen.getByDisplayValue("Accessible project");
    fireEvent.change(input, { target: { value: "Draft" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByDisplayValue("Draft")).not.toBeInTheDocument();
    expect(props.onRenameProject).not.toHaveBeenCalled();
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it("leaves an Escape that a dialog already handled to that dialog", () => {
    const props = renderView();
    const handled = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    handled.preventDefault();
    act(() => { window.dispatchEvent(handled); });
    expect(props.onClose).not.toHaveBeenCalled();
    act(() => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    expect(props.onClose).toHaveBeenCalled();
  });

  it("closes the overlay after loading a project that has no thumbnail, and only once", async () => {
    let finishLoad: () => void = () => undefined;
    const onLoadProject = vi.fn(() => new Promise<void>((resolve) => { finishLoad = resolve; }));
    const props = renderView({ onLoadProject });
    const projectButton = screen.getByRole("button", { name: "Accessible project" });

    fireEvent.click(projectButton);
    fireEvent.click(projectButton);
    expect(onLoadProject).toHaveBeenCalledTimes(1);
    expect(props.onClose).not.toHaveBeenCalled();

    finishLoad();
    await waitFor(() => expect(props.onClose).toHaveBeenCalledTimes(1));
  });

  it("saves and prunes a lowered limit once when pointerup, keyup and blur all follow", async () => {
    const change = vi.fn();
    const setLimit = vi.mocked(projectManager.setLimit);
    setLimit.mockClear();
    let finishSave: () => void = () => undefined;
    setLimit.mockImplementationOnce(() => new Promise<void>((resolve) => { finishSave = resolve; }));
    renderView({ onProjectsChange: change });
    const slider = screen.getByRole("slider", { name: "Max" });

    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "20" } });
    fireEvent.pointerUp(slider);
    fireEvent.keyUp(slider, { key: "ArrowLeft" });
    fireEvent.blur(slider);
    expect(setLimit).toHaveBeenCalledExactlyOnceWith(20);

    finishSave();
    await waitFor(() => expect(change).toHaveBeenCalledOnce());
  });
});
