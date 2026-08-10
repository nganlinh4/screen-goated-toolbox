import { useEffect, useMemo, type Dispatch, type SetStateAction } from 'react';
import { getEffectiveCompositionMode } from '@/lib/projectComposition';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import type { UseSubtitleGenerationParams } from './subtitleGenerationTypes';

interface SubtitleSourceAvailabilityParams {
  composition: UseSubtitleGenerationParams['composition'];
  currentRawMicAudioPath: UseSubtitleGenerationParams['currentRawMicAudioPath'];
  currentRawVideoPath: UseSubtitleGenerationParams['currentRawVideoPath'];
  setSourceType: Dispatch<SetStateAction<SubtitleSource>>;
  sourceType: SubtitleSource;
}

export function useSubtitleSourceAvailability({
  composition,
  currentRawMicAudioPath,
  currentRawVideoPath,
  setSourceType,
  sourceType,
}: SubtitleSourceAvailabilityParams) {
  const canUseVideoSource = useMemo(() => {
    if (composition && getEffectiveCompositionMode(composition) === 'unified') {
      return composition.clips.some((clip) => !!clip.rawVideoPath);
    }
    return !!currentRawVideoPath;
  }, [composition, currentRawVideoPath]);

  const canUseMicSource = useMemo(() => {
    if (composition && getEffectiveCompositionMode(composition) === 'unified') {
      return composition.clips.some((clip) => !!clip.rawMicAudioPath);
    }
    return !!currentRawMicAudioPath;
  }, [composition, currentRawMicAudioPath]);

  const canUseAudioSource = useMemo(
    () => (composition?.audioSegments?.length ?? 0) > 0,
    [composition?.audioSegments?.length],
  );

  useEffect(() => {
    if (sourceType.startsWith('audio:')) {
      const id = sourceType.slice('audio:'.length);
      if (!composition?.audioSegments?.some((segment) => segment.id === id)) {
        setSourceType(composition?.audioSegments?.length ? 'audio' : 'video');
      }
    } else if (sourceType === 'audio' && !canUseAudioSource) {
      setSourceType(canUseVideoSource ? 'video' : canUseMicSource ? 'mic' : 'video');
    }
  }, [
    canUseAudioSource,
    canUseMicSource,
    canUseVideoSource,
    composition?.audioSegments,
    setSourceType,
    sourceType,
  ]);

  return { canUseAudioSource, canUseMicSource, canUseVideoSource };
}
