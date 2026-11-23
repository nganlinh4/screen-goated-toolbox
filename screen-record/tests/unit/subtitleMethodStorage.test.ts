import { describe, expect, it } from 'vitest';
import {
  DEFAULT_SUBTITLE_METHOD_CAPABILITIES,
  normalizeStoredSubtitleMethod,
} from '@/hooks/subtitleGenerationStorage';

describe('stored subtitle method', () => {
  it('keeps every offered method', () => {
    for (const { method } of DEFAULT_SUBTITLE_METHOD_CAPABILITIES) {
      expect(normalizeStoredSubtitleMethod(method)).toBe(method);
    }
  });

  it('falls back to the default for removed offline engines and unknown values', () => {
    for (const stored of ['qwen-local-0-6b', 'qwen-local-1-7b', 'parakeet-tdt-0-6b-v3', '', null]) {
      expect(normalizeStoredSubtitleMethod(stored)).toBe('groq-whisper-accurate');
    }
  });
});
