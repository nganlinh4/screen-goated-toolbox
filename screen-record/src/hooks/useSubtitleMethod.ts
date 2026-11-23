import { useEffect, useState } from 'react';
import {
  DEFAULT_SUBTITLE_METHOD_CAPABILITIES, getInitialSubtitleMethod, persistSubtitleMethod,
} from './subtitleGenerationStorage';
import type { SubtitleMethod } from './subtitleGenerationTypes';

export function useSubtitleMethod() {
  const [subtitleMethod, setSubtitleMethod] = useState<SubtitleMethod>(getInitialSubtitleMethod);
  useEffect(() => { persistSubtitleMethod(subtitleMethod); }, [subtitleMethod]);
  const selectedMethodCapability = DEFAULT_SUBTITLE_METHOD_CAPABILITIES
    .find((entry) => entry.method === subtitleMethod);
  const canUseSelectedSubtitleMethod = selectedMethodCapability?.available !== false;
  const selectedSubtitleMethodReason = selectedMethodCapability?.available === false
    ? selectedMethodCapability.reason ?? 'This subtitle method is unavailable.' : null;
  return {
    subtitleMethod, setSubtitleMethod, canUseSelectedSubtitleMethod, selectedSubtitleMethodReason,
    subtitleMethodCapabilities: DEFAULT_SUBTITLE_METHOD_CAPABILITIES,
  };
}
