import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ImportedAudioSegment, SubtitleSourceGroup } from '@/types/video';
import { makeSubtitleSourceGroup } from '@/lib/subtitleSourceGroups';
import { useSettings } from '@/hooks/useSettings';

const VIEWPORT_MARGIN_PX = 8;

interface SubtitleSourceMenuProps {
  /** Pointer position that opened the menu, in client coordinates. */
  x: number;
  y: number;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  audioSegments: ImportedAudioSegment[];
  onAssign: (sourceGroup: SubtitleSourceGroup) => void;
  onClose: () => void;
}

/** Keeps a `width x height` box opened at (x, y) fully inside the viewport. */
export function clampMenuPosition(
  x: number,
  y: number,
  width: number,
  height: number,
  viewportWidth: number,
  viewportHeight: number,
): { left: number; top: number } {
  const maxLeft = Math.max(VIEWPORT_MARGIN_PX, viewportWidth - width - VIEWPORT_MARGIN_PX);
  const maxTop = Math.max(VIEWPORT_MARGIN_PX, viewportHeight - height - VIEWPORT_MARGIN_PX);
  return {
    left: Math.min(Math.max(VIEWPORT_MARGIN_PX, x), maxLeft),
    top: Math.min(Math.max(VIEWPORT_MARGIN_PX, y), maxTop),
  };
}

const ITEM_CLASS =
  'subtitle-source-menu-item flex w-full items-center rounded-md px-2 py-1.5 text-left text-[var(--on-surface-variant)] hover:bg-[var(--ui-surface-3)] hover:text-[var(--on-surface)] focus-visible:bg-[var(--ui-surface-3)] focus-visible:text-[var(--on-surface)] focus-visible:outline-none';

export function SubtitleSourceMenu({
  x,
  y,
  isDeviceAudioAvailable,
  isMicAudioAvailable,
  audioSegments,
  onAssign,
  onClose,
}: SubtitleSourceMenuProps) {
  const { t } = useSettings();
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const rect = menu.getBoundingClientRect();
    setPosition(clampMenuPosition(x, y, rect.width, rect.height, window.innerWidth, window.innerHeight));
    menu.querySelector<HTMLButtonElement>('.subtitle-source-menu-item')?.focus();
  }, [x, y]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
    };
    const handlePointerDown = (event: PointerEvent) => {
      if (menuRef.current?.contains(event.target as Node)) return;
      onClose();
    };
    window.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('pointerdown', handlePointerDown, true);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      window.removeEventListener('pointerdown', handlePointerDown, true);
    };
  }, [onClose]);

  const moveFocus = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    const items = [...(menuRef.current?.querySelectorAll<HTMLButtonElement>('.subtitle-source-menu-item') ?? [])];
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const step = event.key === 'ArrowDown' ? 1 : -1;
    items[(current + step + items.length) % items.length].focus();
  };

  return (
    <div
      ref={menuRef}
      role="menu"
      aria-label={t.subtitleSource}
      className="subtitle-source-menu ui-surface-elevated fixed z-[110] min-w-[190px] rounded-lg p-1.5 text-[11px]"
      style={{ left: position.left, top: position.top }}
      onPointerDown={(event) => event.stopPropagation()}
      onMouseLeave={onClose}
      onKeyDown={moveFocus}
    >
      <button
        type="button"
        role="menuitem"
        className={ITEM_CLASS}
        onClick={() => onAssign({ kind: 'unassigned', assignment: 'manual' })}
      >
        {t.subtitleSourceUnassigned}
      </button>
      {isDeviceAudioAvailable && (
        <button
          type="button"
          role="menuitem"
          className={ITEM_CLASS}
          onClick={() => onAssign({ kind: 'video', assignment: 'manual' })}
        >
          {t.subtitleSourceDeviceAudio}
        </button>
      )}
      {isMicAudioAvailable && (
        <button
          type="button"
          role="menuitem"
          className={ITEM_CLASS}
          onClick={() => onAssign({ kind: 'mic', assignment: 'manual' })}
        >
          {t.subtitleSourceMicrophone}
        </button>
      )}
      {audioSegments.map((audioSegment) => (
        <button
          key={audioSegment.id}
          type="button"
          role="menuitem"
          className={ITEM_CLASS}
          onClick={() => onAssign(makeSubtitleSourceGroup({ kind: 'audio', assignment: 'manual', audioSegment }))}
        >
          {audioSegment.name || t.subtitleSourceAudio}
        </button>
      ))}
    </div>
  );
}
