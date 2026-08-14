import { useRef, type KeyboardEvent } from "react";
import { motion } from "motion/react";
import { useSettings } from "@/hooks/useSettings";

export type ActivePanel =
  | "zoom"
  | "camera"
  | "background"
  | "cursor"
  | "blur"
  | "audio"
  | "narration"
  | "subtitles"
  | "text";

export const PANEL_TAB_ORDER: ActivePanel[] = [
  "zoom", "camera", "background", "cursor", "blur",
  "audio", "narration", "subtitles", "text",
];

interface SidePanelTabsProps {
  activePanel: ActivePanel;
  onPanelChange: (panel: ActivePanel) => void;
  hiddenTabs?: Set<ActivePanel>;
}

export function SidePanelTabs({ activePanel, onPanelChange, hiddenTabs }: SidePanelTabsProps) {
  const { t } = useSettings();
  const tabRefs = useRef<Partial<Record<ActivePanel, HTMLButtonElement | null>>>({});
  const visibleTabIds = PANEL_TAB_ORDER.filter((id) => !hiddenTabs?.has(id));
  const useCompactLabels = visibleTabIds.length === 7;
  const tabLabel = (id: ActivePanel) => {
    switch (id) {
      case "zoom": return t.tabZoom;
      case "camera": return t.tabCamera;
      case "background": return t.tabBackground;
      case "cursor": return t.tabCursor;
      case "blur": return t.tabBlur;
      case "audio": return t.tabAudio;
      case "narration": return t.tabNarration;
      case "subtitles": return t.tabSubtitles;
      case "text": return t.tabText;
    }
  };
  const compactTabLabel = (id: ActivePanel) => {
    switch (id) {
      case "background": return t.tabBackground === "Nền" ? "Nền" : "Bg";
      case "cursor": return t.tabCursor === "Con Trỏ" ? "C.Trỏ" : t.tabCursor;
      case "audio": return t.tabAudio === "Âm Thanh" ? "Â.Thanh" : t.tabAudio;
      case "narration": return t.tabNarration === "Thuyết Minh" ? "T.Minh" : "Narr.";
      case "subtitles": return t.tabSubtitles === "Phụ Đề" ? "P.Đề" : "Subs";
      default: return tabLabel(id);
    }
  };
  const activateAndFocus = (id: ActivePanel) => {
    onPanelChange(id);
    window.requestAnimationFrame(() => tabRefs.current[id]?.focus());
  };
  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, id: ActivePanel) => {
    const currentIndex = visibleTabIds.indexOf(id);
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % visibleTabIds.length;
    if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + visibleTabIds.length) % visibleTabIds.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = visibleTabIds.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    activateAndFocus(visibleTabIds[nextIndex]);
  };

  return (
    <div className="panel-tabs ui-segmented relative flex flex-nowrap overflow-hidden" role="tablist" aria-label={t.editorTools}>
      {visibleTabIds.map((id) => {
        const fullLabel = tabLabel(id);
        return (
          <button
            key={id}
            ref={(element) => { tabRefs.current[id] = element; }}
            type="button"
            role="tab"
            id={`panel-tab-${id}`}
            aria-controls={`panel-pane-${id}`}
            aria-selected={activePanel === id}
            aria-label={fullLabel}
            tabIndex={activePanel === id ? 0 : -1}
            onKeyDown={(event) => handleKeyDown(event, id)}
            onClick={() => onPanelChange(id)}
            className={`panel-tab-button ui-segmented-button relative flex-1 px-2 py-2 text-[11px] font-medium whitespace-nowrap ${activePanel === id ? "text-[var(--primary-color)]" : ""}`}
          >
            {activePanel === id && (
              <motion.span
                layoutId="side-panel-tab-pill"
                className="panel-tab-pill absolute inset-0 rounded-[10px] border"
                style={{
                  background: "color-mix(in srgb, var(--primary-color) 12%, var(--ui-surface-3))",
                  borderColor: "color-mix(in srgb, var(--primary-color) 36%, var(--ui-border))",
                  boxShadow: "var(--shadow-elevation-1)",
                }}
                transition={{ type: "spring", stiffness: 420, damping: 36, mass: 0.9 }}
              />
            )}
            <span className="panel-tab-label relative z-10">{useCompactLabels ? compactTabLabel(id) : fullLabel}</span>
          </button>
        );
      })}
    </div>
  );
}
