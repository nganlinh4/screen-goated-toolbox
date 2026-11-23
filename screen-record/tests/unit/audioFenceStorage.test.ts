import 'fake-indexeddb/auto';
import { expect, it, vi } from 'vitest';
import { projectManager } from '@/lib/projectManager';
import { beginAudioImportWrite, finishAudioImportWrite, type AudioImportWriteFence } from '@/lib/audioImportWriteFence';
import { buildCompositionAssetKey, idbCreateProjectBundle, idbGet, idbWriteCompositionAssetBundle, PROJECTS_STORE } from '@/lib/projectStorage';
import { createSyntheticProjectFixture } from '@/testHarness/syntheticProject';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => undefined), logToHost: vi.fn() }));

it.each(['record', 'metadata', 'media', 'asset-save', 'asset-delete', 'combined', 'project-delete'] as const)(
  'aborts an in-flight %s transaction when an audio fence starts', async (kind) => {
    const project = { ...createSyntheticProjectFixture(), id: `audio-fence-${kind}` };
    const key = buildCompositionAssetKey(project.id, 'clip');
    await projectManager.getProjects();
    await idbCreateProjectBundle(project);
    await idbWriteCompositionAssetBundle(key, { customBackground: 'original' });
    let fence: AudioImportWriteFence | undefined;
    const targetStore = kind === 'record' || kind === 'metadata' || kind === 'media' || kind === 'project-delete' ? PROJECTS_STORE : 'composition_custom_backgrounds';
    const trigger = (store: IDBObjectStore) => {
      if (!fence && store.name === targetStore) fence = beginAudioImportWrite(project.id);
    };
    const put = IDBObjectStore.prototype.put;
    const del = IDBObjectStore.prototype.delete;
    const putSpy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, id) {
      const request = id === undefined ? put.call(this, value) : put.call(this, value, id);
      trigger(this);
      return request;
    });
    const deleteSpy = vi.spyOn(IDBObjectStore.prototype, 'delete').mockImplementation(function (id) {
      const request = del.call(this, id);
      if (kind === 'asset-delete' || kind === 'project-delete') trigger(this);
      return request;
    });
    let result: unknown;
    try {
      const operation = kind === 'record' ? projectManager.updateProject(project.id, { duration: 99 })
        : kind === 'metadata' ? projectManager.updateProject(project.id, { name: 'changed' })
        : kind === 'media' ? projectManager.updateProject(project.id, { rawMicAudioPath: 'changed.wav' })
        : kind === 'asset-save' ? projectManager.saveCompositionClipAssets(project.id, 'clip', { customBackground: 'new' })
          : kind === 'asset-delete' ? projectManager.deleteCompositionClipAssets(project.id, 'clip')
            : kind === 'combined' ? projectManager.updateProjectWithCompositionClipAssets(project.id, 'clip', project.composition!, { customBackground: 'new' })
              : projectManager.deleteProject(project.id);
      result = await operation.catch(() => false);
      expect(fence).toBeDefined();
      expect(result).toBe(false);
    } finally {
      putSpy.mockRestore(); deleteSpy.mockRestore();
      if (fence) finishAudioImportWrite(fence, true);
    }
    expect((await idbGet(PROJECTS_STORE, project.id))?.duration).toBe(project.duration);
    expect((await idbGet(PROJECTS_STORE, project.id))?.name).toBe(project.name);
    expect((await idbGet(PROJECTS_STORE, project.id))?.rawMicAudioPath).toBe(project.rawMicAudioPath);
    expect(await idbGet('composition_custom_backgrounds', key)).toBe('original');
  },
);
