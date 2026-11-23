import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ProjectsView } from '@/components/ProjectsView';
import { projectManager } from '@/lib/projectManager';
import { idbCreateProjectBundle, idbGet, PROJECTS_STORE } from '@/lib/projectStorage';
import { invoke } from '@/lib/ipc';
import type { Project } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined) }));
// The real dialog module draws canvas previews at import; keep the ConfirmDialog contract only.
vi.mock('@/components/dialogs', () => ({
  ConfirmDialog: ({ show, message, confirmLabel, onConfirm, onCancel }: {
    show: boolean; message: string; confirmLabel: string; onConfirm: () => void; onCancel: () => void;
  }) => show ? (
    <div role="dialog"><p>{message}</p>
      <button type="button" onClick={onCancel}>Cancel</button>
      <button type="button" onClick={onConfirm}>{confirmLabel}</button>
    </div>
  ) : null,
}));

const limitCalls = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === 'set_project_limit');

describe('project limit interaction and backing retention', () => {
  it('saves only committed limits and deletes projects only after an explicit confirmation', async () => {
    await projectManager.setLimit(50);
    projectManager.setActiveProjectId(null);
    for (let index = 0; index < 12; index++) {
      const track = index === 1 ? 'audioSegments' : 'narrationSegments';
      await idbCreateProjectBundle({
        id: `slider-${index}`, name: `Project ${index}`, createdAt: index, lastModified: index,
        duration: 1, segment: { trimStart: 0, trimEnd: 1, trimSegments: [] },
        backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: 'solid' }, mousePositions: [],
        rawVideoPath: index >= 3 ? `recording-${index}.mp4` : undefined,
        composition: index === 1 || index === 2 ? { mode: 'separate', clips: [], [track]: [{
          id: 'take', rawAudioPath: `take-${index}.wav`, name: 'Audio', duration: 1,
          startTime: 0, inPoint: 0, outPoint: 1, addedAt: index,
        }] } : undefined,
      } as Project);
    }
    vi.mocked(invoke).mockClear();
    const changed = vi.fn();
    const deleteProject = vi.fn((id: string) => projectManager.deleteProject(id));
    render(<ProjectsView projects={await projectManager.getProjects()} onLoadProject={() => undefined}
      onProjectsChange={changed} onClearProjects={async () => undefined}
      onDeleteProject={deleteProject} onRenameProject={async () => undefined}
      onClose={() => undefined} currentProjectId="slider-11" />);
    const slider = screen.getByRole('slider', { name: 'Max' });

    // Dragging through lower values touches nothing; the release value is the commit.
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: '10' } });
    fireEvent.change(slider, { target: { value: '12' } });
    expect(limitCalls()).toHaveLength(0);
    fireEvent.pointerUp(slider);
    await waitFor(() => expect(changed).toHaveBeenCalledTimes(1));
    expect(limitCalls()).toEqual([['set_project_limit', { limit: 12 }]]);

    // A cancelled drag restores the saved limit before a trailing blur.
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: '10' } });
    act(() => { fireEvent.pointerCancel(slider); fireEvent.blur(slider); });
    expect(projectManager.getLimit()).toBe(12);
    expect(screen.queryByRole('dialog')).toBeNull();

    // Committing a limit below the project count asks first; cancelling keeps everything.
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: '10' } });
    fireEvent.pointerUp(slider);
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('Lowering the limit to 10 permanently deletes the 2 oldest project(s)');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(slider).toHaveValue('12');
    expect(projectManager.getLimit()).toBe(12);
    expect(limitCalls()).toHaveLength(1);
    expect(deleteProject).not.toHaveBeenCalled();
    expect(await idbGet(PROJECTS_STORE, 'slider-0')).not.toBeNull();
    expect(await idbGet(PROJECTS_STORE, 'slider-1')).not.toBeNull();

    // Confirming deletes exactly the named oldest projects, then saves the limit.
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: '10' } });
    fireEvent.pointerUp(slider);
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(changed).toHaveBeenCalledTimes(2));
    expect(deleteProject.mock.calls.map(([id]) => id)).toEqual(['slider-0', 'slider-1']);
    expect(limitCalls()).toEqual([['set_project_limit', { limit: 12 }], ['set_project_limit', { limit: 10 }]]);
    expect(await idbGet(PROJECTS_STORE, 'slider-0')).toBeNull();
    expect(await idbGet(PROJECTS_STORE, 'slider-1')).toBeNull();
    expect(await idbGet(PROJECTS_STORE, 'slider-2')).not.toBeNull();
    expect((await projectManager.getProjects()).map((project) => project.id)).toHaveLength(10);
  });

  it('never deletes the open project to satisfy a lower limit', async () => {
    const { selectProjectsBeyondLimit } = await import('@/lib/projectRetention');
    const projects = [0, 1, 2].map((index) => ({ id: `p${index}`, createdAt: index, lastModified: index }));
    expect(selectProjectsBeyondLimit(projects, 1, 'p0').map((project) => project.id)).toEqual(['p1', 'p2']);
    expect(selectProjectsBeyondLimit(projects, 3, null)).toEqual([]);
  });
});
