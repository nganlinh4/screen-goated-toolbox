import { useCallback, useRef } from 'react';
import { getInitialBackgroundConfig } from '@/lib/appUtils';
import { cloneBackgroundConfig } from '@/lib/backgroundConfig';
import type { BackgroundConfig } from '@/types/video';
import { useSynchronizedState } from './useSynchronizedState';

export function useBackgroundSelection() {
  const [backgroundConfig, setSynchronizedBackgroundConfigState, backgroundConfigRef] =
    useSynchronizedState<BackgroundConfig>(() => cloneBackgroundConfig(getInitialBackgroundConfig()));
  const backgroundSelectionRevisionRef = useRef(0);
  const setBackgroundConfigState = useCallback((value: BackgroundConfig | ((previous: BackgroundConfig) => BackgroundConfig)) => {
    const previous = backgroundConfigRef.current;
    setSynchronizedBackgroundConfigState(value);
    const next = backgroundConfigRef.current;
    if (next.backgroundType !== previous.backgroundType || next.customBackground !== previous.customBackground) {
      backgroundSelectionRevisionRef.current += 1;
    }
  }, [backgroundConfigRef, setSynchronizedBackgroundConfigState]);
  return { backgroundConfig, backgroundConfigRef, backgroundSelectionRevisionRef, setBackgroundConfigState };
}
