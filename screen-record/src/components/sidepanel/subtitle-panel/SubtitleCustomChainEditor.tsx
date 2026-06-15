import { useCallback, useEffect, useRef, useState } from 'react';
import { GripVertical, Plus, Trash2 } from '@/components/ui/MaterialIcon';
import { PanelSelect } from '@/components/ui/PanelSelect';
import type { Translations } from '@/i18n';
import type { SubtitleChainItem, SubtitleTrack } from '@/types/video';
import { getSubtitleTrackLabel } from '@/lib/subtitleTracks';

interface SubtitleCustomChainEditorProps {
  t: Translations;
  tracks: SubtitleTrack[];
  chain: SubtitleChainItem[];
  onChange: (chain: SubtitleChainItem[]) => void;
}

const DELIMITER_PRESETS = [
  { value: ' ', label: '⎵' },
  { value: '\n', label: '↵' },
  { value: ' / ', label: '/' },
  { value: ' | ', label: '|' },
  { value: ' → ', label: '→' },
];

export function SubtitleCustomChainEditor({
  t,
  tracks,
  chain,
  onChange,
}: SubtitleCustomChainEditorProps) {
  const [dragState, setDragState] = useState<{
    activeIndex: number;
    overIndex: number;
    startY: number;
    currentY: number;
  } | null>(null);
  const itemRefs = useRef<Array<HTMLDivElement | null>>([]);
  const trackOptions = tracks.map((track) => ({
    value: track.id,
    label: track.kind === 'original' ? t.subtitleTrackOriginal : getSubtitleTrackLabel(track),
  }));

  const moveItem = useCallback((fromIndex: number, toIndex: number) => {
    if (fromIndex === toIndex || fromIndex < 0 || toIndex < 0 || fromIndex >= chain.length || toIndex >= chain.length) {
      return;
    }
    const next = [...chain];
    const [item] = next.splice(fromIndex, 1);
    next.splice(toIndex, 0, item);
    onChange(next);
  }, [chain, onChange]);

  const removeItem = (index: number) => {
    onChange(chain.filter((_, itemIndex) => itemIndex !== index));
  };

  const updateItem = (index: number, item: SubtitleChainItem) => {
    onChange(chain.map((entry, itemIndex) => (itemIndex === index ? item : entry)));
  };

  const getDropIndex = useCallback((clientY: number, fallbackIndex: number) => {
    const indexedRects = itemRefs.current
      .map((node, index) => ({ index, rect: node?.getBoundingClientRect() ?? null }))
      .filter((entry): entry is { index: number; rect: DOMRect } => entry.rect !== null);

    for (const entry of indexedRects) {
      if (clientY < entry.rect.top + entry.rect.height / 2) {
        return entry.index;
      }
    }

    return indexedRects[indexedRects.length - 1]?.index ?? fallbackIndex;
  }, []);

  useEffect(() => {
    if (!dragState) return undefined;

    const handlePointerMove = (event: PointerEvent) => {
      event.preventDefault();
      setDragState((current) => (current
        ? {
            ...current,
            currentY: event.clientY,
            overIndex: getDropIndex(event.clientY, current.overIndex),
          }
        : current));
    };

    const handlePointerEnd = () => {
      setDragState((current) => {
        if (current && current.overIndex !== current.activeIndex) {
          moveItem(current.activeIndex, current.overIndex);
        }
        return null;
      });
    };

    window.addEventListener('pointermove', handlePointerMove, { passive: false });
    window.addEventListener('pointerup', handlePointerEnd);
    window.addEventListener('pointercancel', handlePointerEnd);

    return () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerEnd);
      window.removeEventListener('pointercancel', handlePointerEnd);
    };
  }, [dragState, getDropIndex, moveItem]);

  return (
    <div className="subtitle-custom-chain-editor mt-2 space-y-2 rounded-xl border border-[var(--ui-border)] bg-[var(--ui-surface-2)] p-2.5">
      <div className="subtitle-custom-chain-header flex items-center justify-between gap-2">
        <span className="text-[11px] font-medium text-on-surface-variant">
          {t.subtitleTrackCustomChain}
        </span>
        <div className="subtitle-custom-chain-actions flex items-center gap-1.5">
          <button
            type="button"
            className="subtitle-custom-chain-add-track ui-action-button flex h-7 items-center gap-1 rounded-lg px-2 text-[11px]"
            onClick={() => onChange([...chain, { type: 'track', trackId: tracks[0]?.id ?? '' }])}
            disabled={tracks.length === 0}
          >
            <Plus className="h-3.5 w-3.5" />
            {t.subtitleTrackAddTrack}
          </button>
          <button
            type="button"
            className="subtitle-custom-chain-add-delimiter ui-action-button flex h-7 items-center gap-1 rounded-lg px-2 text-[11px]"
            onClick={() => onChange([...chain, { type: 'delimiter', value: ' / ' }])}
          >
            <Plus className="h-3.5 w-3.5" />
            {t.subtitleTrackAddDelimiter}
          </button>
        </div>
      </div>

      <div className="subtitle-custom-chain-list space-y-2">
        {chain.map((item, index) => (
          <div
            key={`${item.type}-${index}`}
            ref={(node) => {
              itemRefs.current[index] = node;
            }}
            className={`subtitle-custom-chain-item flex items-center gap-2 rounded-lg border bg-[var(--ui-surface-3)] p-2 transition-transform ${
              dragState?.activeIndex === index
                ? 'border-[var(--primary-color)]'
                : dragState?.overIndex === index
                  ? 'border-[var(--primary-color)]/70'
                  : 'border-[var(--ui-border)]'
            }`}
            style={{
              transform: dragState?.activeIndex === index
                ? `translateY(${dragState.currentY - dragState.startY}px)`
                : undefined,
              zIndex: dragState?.activeIndex === index ? 2 : 1,
              boxShadow: dragState?.activeIndex === index
                ? '0 12px 28px rgba(0, 0, 0, 0.24)'
                : undefined,
              opacity: dragState?.activeIndex === index ? 0.96 : 1,
            }}
            onPointerEnter={() => {
              setDragState((current) => (current
                ? {
                    ...current,
                    overIndex: index,
                  }
                : current));
            }}
          >
            <button
              type="button"
              className="subtitle-custom-chain-drag-handle flex h-7 w-7 shrink-0 cursor-grab items-center justify-center rounded-lg text-on-surface-variant transition-colors hover:bg-[var(--ui-surface-2)] hover:text-on-surface active:cursor-grabbing"
              style={{ touchAction: 'none' }}
              onPointerDown={(event) => {
                if (event.pointerType === 'mouse' && event.button !== 0) {
                  return;
                }
                event.preventDefault();
                event.stopPropagation();
                setDragState({
                  activeIndex: index,
                  overIndex: index,
                  startY: event.clientY,
                  currentY: event.clientY,
                });
              }}
              title={t.dragToReorder}
              aria-label={t.dragToReorder}
            >
              <GripVertical className="h-3.5 w-3.5" />
            </button>
            {item.type === 'track' ? (
              <PanelSelect
                value={item.trackId}
                options={trackOptions}
                onChange={(value) => updateItem(index, { type: 'track', trackId: value })}
                triggerClassName="subtitle-custom-chain-track-select h-8 min-w-0 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="subtitle-custom-chain-track-menu"
              />
            ) : (
              <div className="subtitle-custom-chain-delimiter flex min-w-0 flex-1 items-center gap-2">
                <input
                  value={item.value}
                  onChange={(event) => updateItem(index, { type: 'delimiter', value: event.target.value })}
                  className="subtitle-custom-chain-delimiter-input ui-input h-8 min-w-0 flex-1 rounded-lg px-2.5 text-[11px]"
                  placeholder={t.subtitleTrackDelimiterPlaceholder}
                />
                <PanelSelect
                  value={DELIMITER_PRESETS.some((preset) => preset.value === item.value) ? item.value : '__custom__'}
                  options={[
                    ...DELIMITER_PRESETS.map((preset) => ({
                      value: preset.value,
                      label: preset.label,
                    })),
                    { value: '__custom__', label: t.subtitleTrackDelimiterCustom },
                  ]}
                  onChange={(value) => {
                    if (value === '__custom__') return;
                    updateItem(index, { type: 'delimiter', value });
                  }}
                  triggerClassName="subtitle-custom-chain-delimiter-select h-8 w-[4.5rem] rounded-lg px-2 text-[11px]"
                  contentClassName="subtitle-custom-chain-delimiter-menu"
                />
              </div>
            )}
            <div className="subtitle-custom-chain-row-actions flex items-center gap-1">
              <button
                type="button"
                className="subtitle-custom-chain-remove ui-action-button flex h-7 w-7 items-center justify-center rounded-lg"
                onClick={() => removeItem(index)}
                title={t.subtitleTrackRemoveItem}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
