import { vi } from 'vitest';
import { invoke } from '@/lib/ipc';
import type { VideoSegment } from '@/types/video';

export const mockNarrationSourceSnapshot = () => ({
  composition: null, segment: { trimStart: 0, trimEnd: 3 } as VideoSegment,
  loadedClipId: null, videoPath: 'source.mp4', micPath: '',
});

let sequence = 0;

export function mockNarrationInvoke(implementation: (command: string, args?: Record<string, unknown>) => Promise<unknown>): void {
  vi.mocked(invoke).mockImplementation((command, args) => {
    if (command === 'allocate_narration_job_id') {
      return Promise.resolve({ jobId: `ticket-test-${String(args?.kind)}-${++sequence}` });
    }
    return implementation(command, args);
  });
}
