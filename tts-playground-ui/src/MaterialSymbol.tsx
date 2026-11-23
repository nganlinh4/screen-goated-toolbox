import { materialSymbolPath, type MaterialSymbolName } from "../../ui-shared/material-symbols";

/** A glyph from the shared Material Symbols catalog (ui-shared/material-symbols). */
export function MaterialSymbol({ name, className }: { name: MaterialSymbolName; className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden="true" focusable="false">
      <path d={materialSymbolPath(name)} />
    </svg>
  );
}
