---
name: update-frontend
description: Use when updating or revamping the screen recorder frontend UI. Covers the shared theme layer, key screen-record entry points, dialog/timeline/sidebar hotspots, and the fastest file map for visual changes.
---

# Update Frontend

Use this skill for `screen-record` UI changes, especially redesign, polish, layout changes, and theme cleanup.

## Core rule

- Treat [screen-record/src/App.css](../../screen-record/src/App.css) as the main aesthetic control plane.
- Prefer changing shared `ui-*`, `timeline-*`, and semantic color tokens there before patching one-off component classes.
- Keep preview/editor/export visually aligned. Do not create separate “special styling” paths when one shared component or token can own it.
- Start with the files that own the largest surface area before touching leaf components:
  [screen-record/src/App.css](../../screen-record/src/App.css),
  [screen-record/src/App.tsx](../../screen-record/src/App.tsx),
  [screen-record/src/components/Header.tsx](../../screen-record/src/components/Header.tsx),
  [screen-record/src/components/dialogs/ExportDialog.tsx](../../screen-record/src/components/dialogs/ExportDialog.tsx),
  [screen-record/src/components/sidepanel/SidePanel.tsx](../../screen-record/src/components/sidepanel/SidePanel.tsx).

## File map

Read these first for most UI work:

- [screen-record/src/App.tsx](../../screen-record/src/App.tsx)
  Main recorder composition, preview shell, playback toolbar wiring, dialogs, timeline, side panel.
- [screen-record/src/App.css](../../screen-record/src/App.css)
  Theme tokens, material surfaces, shared buttons, timeline primitives, interaction colors.
- [screen-record/src/components/ui/Dialog.tsx](../../screen-record/src/components/ui/Dialog.tsx)
  All dialog shell placement, rounding, overlay, close button behavior.
- [screen-record/src/components/ui/DropdownMenu.tsx](../../screen-record/src/components/ui/DropdownMenu.tsx)
  Shared dropdown menu visuals and item selection styling.
- [screen-record/src/components/ui/button.tsx](../../screen-record/src/components/ui/button.tsx)
  Base button variants used across the app.

## Main screen areas

- Header:
  [screen-record/src/components/Header.tsx](../../screen-record/src/components/Header.tsx)
- Preview + playback controls:
  [screen-record/src/components/VideoPreview.tsx](../../screen-record/src/components/VideoPreview.tsx)
- Side panel tabs + panel transitions:
  [screen-record/src/components/sidepanel/SidePanel.tsx](../../screen-record/src/components/sidepanel/SidePanel.tsx)
- Timeline shell:
  [screen-record/src/components/timeline/TimelineArea.tsx](../../screen-record/src/components/timeline/TimelineArea.tsx)

## Timeline hotspots

When editing lower-editor UI, check these together:

- [screen-record/src/components/timeline/TrimTrack.tsx](../../screen-record/src/components/timeline/TrimTrack.tsx)
- [screen-record/src/components/timeline/Playhead.tsx](../../screen-record/src/components/timeline/Playhead.tsx)
- [screen-record/src/components/timeline/ZoomTrack.tsx](../../screen-record/src/components/timeline/ZoomTrack.tsx)
- [screen-record/src/components/timeline/SpeedTrack.tsx](../../screen-record/src/components/timeline/SpeedTrack.tsx)
- [screen-record/src/components/timeline/TextTrack.tsx](../../screen-record/src/components/timeline/TextTrack.tsx)
- [screen-record/src/components/timeline/KeystrokeTrack.tsx](../../screen-record/src/components/timeline/KeystrokeTrack.tsx)
- [screen-record/src/components/timeline/PointerTrack.tsx](../../screen-record/src/components/timeline/PointerTrack.tsx)

## Dialog hotspots

- Export dialog:
  [screen-record/src/components/dialogs/ExportDialog.tsx](../../screen-record/src/components/dialogs/ExportDialog.tsx)
- Result dialog:
  [screen-record/src/components/dialogs/MediaResultDialog.tsx](../../screen-record/src/components/dialogs/MediaResultDialog.tsx)
- Shared processing / confirm dialogs:
  [screen-record/src/components/dialogs/index.tsx](../../screen-record/src/components/dialogs/index.tsx)

## Styling workflow

1. Decide whether the change is global or local.
2. If global, patch [screen-record/src/App.css](../../screen-record/src/App.css) first.
3. Reuse shared classes:
   `ui-surface`, `ui-chip-button`, `ui-toolbar-button`, `ui-action-button`, `ui-segmented`, `timeline-lane`, `timeline-block`, `timeline-add-button`, `timeline-handle-pill`.
4. Only add new shared primitives if at least two components benefit.
5. Keep hover/active/focus colors semantically consistent:
   blue = general action / zoom
   yellow = pointer-related
   teal = applied/success utility states
   red = destructive / recording / playhead

## Interaction rules

- Pointer-driven timelines should use pointer events for controls inside them.
- If visual content must clip to a rounded track, clip an inner wrapper, not the whole lane, so handles can stay outside.
- If a transition affects layout or state handoff, prefer one shared owner component rather than duplicated local animation logic.

## Verification

- Run:
  `cd screen-record && npx tsc --noEmit`
- For visual sweeps after larger UI changes, manually check:
  header dropdowns
  export dialog
  preview playback controls
  side panel tab transitions
  timeline tracks in light and dark mode
  button hover tones in light and dark mode
