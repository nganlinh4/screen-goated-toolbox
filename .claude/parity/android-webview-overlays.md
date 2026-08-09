# Android WebView Overlay Rendering Parity

## Canonical Sources

- Windows result scene compositor: [src/overlay/result/scene_compositor](../../src/overlay/result/scene_compositor)
- Windows Phone Control orb: [src/overlay/computer_control/orb/orb.html](../../src/overlay/computer_control/orb/orb.html)
- Windows realtime overlay: [src/overlay/realtime_webview](../../src/overlay/realtime_webview)
- Android shared rendering policy: [OverlayWebViewRendering.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/OverlayWebViewRendering.kt)
- Android Phone Control orb host: [PhoneControlOverlayController.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/phonecontrol/overlay/PhoneControlOverlayController.kt)

## Rendering Contract

- Android overlay windows request hardware acceleration before their WebView is
  attached. A WebView does not keep a persistent hardware view layer because
  continuously animated transparent content would invalidate that extra
  offscreen texture every frame.
- Phone Control, Live Translate, and preset overlay WebViews share this window
  rendering policy. Feature-specific window, focus, touch, capture, and
  lifecycle ownership remains unchanged.
- Latest-state visual updates that supersede older updates are conflated to at
  most one dispatch per display frame. Geometry changes are likewise applied at
  most once per display frame while a pointer gesture is active.
- Phone Control keeps the canonical full-display renderer and separate orb touch
  shim. Dragging updates only the touch window and renderer placement; the
  full-display renderer window is resized only for attachment or configuration
  changes. Its durable normalized position is written only when the gesture
  ends outside the dismiss target.
- Performance work must preserve the canonical HTML, transparent pixels,
  backdrop/blur treatment, animations, controls, capture exclusion, and touch
  regions. It cannot replace a WebView effect with a reduced native redesign.
- Related preset result cards may later share one Android renderer only when the
  Windows scene protocol can be ported without changing independent card input,
  focus, navigation, and lifecycle behavior. Renderer consolidation is not a
  substitute for the shared per-window frame contract.

## Fixture

- [rendering-contract.json](../../parity-fixtures/android-webview-overlays/rendering-contract.json)

## Deviations

- Android uses platform overlay windows rather than the Windows child-process
  scene host. The visual and update contracts remain canonical.
