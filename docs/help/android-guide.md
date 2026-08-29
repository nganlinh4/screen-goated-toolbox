# Android guide

## App surfaces and permissions

The Android companion uses native screens and optional overlays. Overlay-based results require the system permission to display over other apps. When that permission is missing, the app keeps the pending Help Assistant question, opens the Android permission page, and retries after permission is granted.

Microphone, notifications, media projection, accessibility, and nearby-device permissions are requested only by features that need them. Denying a permission keeps the affected capability unavailable and does not grant a substitute authority.

## Live workflows

Android supports live capture, transcription, translation, and text-to-speech. A foreground service or overlay may remain active while the main activity is not visible. Use the feature's Stop action to finish the active job.

## Phone Control

Phone Control uses the same stable product contract in Full and Play distributions. Available actions depend on granted Android capabilities. Accessibility provides ordinary UI interaction. Additional explicitly configured authority may provide bounded device operations that Accessibility cannot perform. Missing authority is reported instead of pretending the action succeeded.

## Creation projects

Image to 3D, Image to SVG, and Image Creator share the project-style Creation flow. Jobs may continue in the background, and completed artifacts remain in project history until the user chooses what to download. Downloads are published through Android's user-visible storage rather than exposed from the app's private files directory.

## Full and Play distributions

Full and Play provide the same user-facing capability where a feature is released. They may obtain large native components differently: Full can download verified runtime archives, while Play can use on-demand delivery. Packaging differences do not create a separate workflow.

## Downloaded Tools and cleanup

Downloaded Tools reports the live status of managed runtimes and models. Removal cancels or stops owning jobs before deleting verified component files. User-created results and saved projects remain separate from removable component storage.

## Troubleshooting

If an overlay does not appear, confirm the display-over-other-apps permission. If a capture session cannot start, confirm microphone or media-projection permission as appropriate. If a managed runtime is unavailable, open Downloaded Tools and retry or repair it while online.
