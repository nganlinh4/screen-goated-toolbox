---
name: update-frontend
description: Update, redesign, or debug the Screen Recorder React UI, including shared styling, preview controls, dialogs, side panels, and timeline interactions. Use for any visual or interaction work under screen-record.
---

# Update Screen Recorder Frontend

## Read First

- [App.tsx](../../../screen-record/src/App.tsx): composition and state ownership
- [App.css](../../../screen-record/src/App.css): theme tokens and shared visual primitives
- [VideoPreview.tsx](../../../screen-record/src/components/VideoPreview.tsx): preview and playback
- [SidePanel.tsx](../../../screen-record/src/components/sidepanel/SidePanel.tsx): editing controls
- [TimelineArea.tsx](../../../screen-record/src/components/timeline/TimelineArea.tsx): timeline shell
- [Dialog.tsx](../../../screen-record/src/components/ui/Dialog.tsx): shared dialogs

Read the nearest component plus its tests before editing. For timeline work, inspect every affected track in `screen-record/src/components/timeline/` together.

## Rules

- Use semantic kebab-case classes on JSX elements.
- Change shared tokens/classes in `App.css` when the rule is global; keep local behavior with its owner component.
- Reuse `ui-*` and `timeline-*` primitives. Add a shared primitive only when multiple components need it.
- Preview and export must share parameters and math. Never tune a separate export look.
- Use pointer events for pointer-driven timeline controls.
- Clip rounded visual content in an inner wrapper so external handles remain visible.
- Keep one owner for state handoff and layout transitions.
- Preserve keyboard focus, touch targets, light/dark themes, and reduced-motion behavior.

## Verify

From `screen-record/` run:

```powershell
npm run build
npm test
```

Run the focused component, Playwright, Wry, or performance suite named in `screen-record/README.md` when the change reaches that boundary. Manually inspect the changed surface in light and dark themes.
