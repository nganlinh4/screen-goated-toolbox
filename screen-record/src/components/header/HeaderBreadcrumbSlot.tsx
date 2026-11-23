import { createContext, useCallback, useContext, useRef, type ReactNode } from 'react';

type ReportContentWidth = (widthPx: number) => void;

const BreadcrumbContentWidthContext = createContext<ReportContentWidth | null>(null);

/** Lets breadcrumb content report its intrinsic width to the header slot that sizes it. */
export function useReportBreadcrumbContentWidth(): ReportContentWidth | null {
  return useContext(BreadcrumbContentWidthContext);
}

interface HeaderBreadcrumbSlotProps {
  hidden: boolean;
  children: ReactNode;
}

/**
 * Owns the header breadcrumb slot and its `--breadcrumb-content-w` variable.
 * The variable lives on the slot itself because custom properties only
 * inherit downward: the slot is what reads it for its width.
 */
export function HeaderBreadcrumbSlot({ hidden, children }: HeaderBreadcrumbSlotProps) {
  const slotRef = useRef<HTMLDivElement>(null);
  const reportContentWidth = useCallback<ReportContentWidth>((widthPx) => {
    slotRef.current?.style.setProperty('--breadcrumb-content-w', `${Math.max(0, Math.ceil(widthPx))}px`);
  }, []);

  return (
    <div
      ref={slotRef}
      className={`header-breadcrumb-slot flex items-center transition-opacity duration-200 ${hidden ? 'opacity-0 pointer-events-none' : 'opacity-100'}`}
      style={{ width: 'min(var(--breadcrumb-content-w, 360px), 70%)' }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <BreadcrumbContentWidthContext.Provider value={reportContentWidth}>
        {children}
      </BreadcrumbContentWidthContext.Provider>
    </div>
  );
}
