import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProcessingOverlay } from "@/components/dialogs";
import { ProjectsView } from "@/components/ProjectsView";
import ko from "@/i18n/ko";
import type { Project } from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: ko, lang: "ko" }),
}));

// The export dialog pulls in canvas background painting that jsdom cannot run.
vi.mock("@/components/dialogs/ExportDialog", () => ({ ExportDialog: () => null }));

vi.mock("@/lib/projectManager", () => ({
  projectManager: {
    getLimit: () => 50,
    setLimit: vi.fn(async () => undefined),
  },
}));

function postDiagnostics(diagnostics: Record<string, unknown>) {
  act(() => {
    window.dispatchEvent(new MessageEvent("message", { data: { type: "sr-export-diagnostics", diagnostics } }));
  });
}

describe("localized status strings", () => {
  it("renders export diagnostics status words from the active language", () => {
    render(<ProcessingOverlay show />);
    postDiagnostics({ turbo: true, codec: "h264", backend: "nvenc", bitrateDeviationPercent: -2.5, fallbackUsed: true });
    const line = document.querySelector(".processing-diagnostics")!.textContent!;
    expect(line).toBe(
      `${ko.exportDiagnosticsTurbo} · h264 · nvenc · ${ko.exportDiagnosticsDeviation.replace("{value}", "2.5%")} · ${ko.exportDiagnosticsFallback}`,
    );
    expect(line).not.toMatch(/Turbo|Standard|fallback|\bdev\b/);

    postDiagnostics({ turbo: false, codec: "h264", backend: "nvenc" });
    expect(document.querySelector(".processing-diagnostics")!.textContent).toContain(ko.exportDiagnosticsStandard);
  });

  it("announces project deletion in the active language", () => {
    const project = {
      id: "p1",
      name: "데모",
      createdAt: 1,
      lastModified: 1,
      duration: 5,
      segment: { trimStart: 0, trimEnd: 5, trimSegments: [] },
      backgroundConfig: { scale: 1, borderRadius: 0, backgroundType: "solid" },
      mousePositions: [],
    } as Project;
    render(
      <ProjectsView
        projects={[project]}
        onLoadProject={() => undefined}
        onProjectsChange={() => undefined}
        onClearProjects={async () => undefined}
        onDeleteProject={async () => undefined}
        onRenameProject={async () => undefined}
        onClose={() => undefined}
      />,
    );
    expect(screen.getByRole("button", { name: ko.deleteProjectNamedLabel.replace("{name}", "데모") })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Delete project/ })).toBeNull();
  });
});
