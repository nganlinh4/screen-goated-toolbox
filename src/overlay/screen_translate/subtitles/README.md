# Continuous subtitles

The first subtitle hotkey opens the shared draw-box selector. Releasing the drawn
region starts the session; Escape cancels without starting OCR. Later hotkey presses
toggle the compact editing controls. Only Quit
ends the session. The adjustable region stays on its selected monitor. This mode
does not use the Screen Translate glow setting and never prepares OCR at normal
application startup.

The editor is a native per-pixel layered window, independent of the settings
renderer. Its input region contains only the outline, eight resize handles and
control strip; the center remains transparent and click-through. Resizing uses
the same physical-coordinate geometry as the saved-region editor. Hiding the
editor hides its native window, without ending capture. Translated subtitle cells
are passive and click-through; the editor alone owns adjustment and Quit input.
Quit destroys it and
cancels only the session's owned results.

Capture excludes the editor and result compositor during the session. Region changes invalidate pending work;
only the current confirmed text revision may update its result. OCR uncertainty
does not count as an empty subtitle. Growing text has a bounded confirmation
wait. Translation uses the configured Screen Translate model and shared text
priority chain. Ordinary Screen Translate OCR/layout events feed its existing
unit planner, inference, cell fitting and result compositor.
There is no alternate caption renderer, translation prompt or worker package.
No downloaded video or replay output belongs in the repository.

Subtitle requests include read-only recent dialogue through the shared inference
path, without extra provider calls. History contains only completed, still-current
translations, bounded to four turns, 45 seconds and 4096 serialized bytes. Prefix
revisions replace their earlier turn and never use themselves as history. Region,
language, prompt or model changes clear history; quitting discards the session.
Current source always takes precedence over potentially incomplete earlier dialogue.

Capture, OCR and translation run independently. OCR has one replaceable pending
frame, not a backlog. Persistent text groups own separate revisions and result
lifetimes; ready groups share a translation request. One disappearing group cannot
cancel a still-visible sibling, and a late response cannot restore an expired
revision. Confirmed unchanged text retains its translation across background
motion and follows small source movements without another provider request.
An exact, session-local cache includes translation settings and dialogue context
and is bounded by age, count and bytes.

Changed edge signatures within OCR boxes trigger recognition, including new
strokes in formerly blank pixels. Other image motion is rechecked within 600 ms;
unchanged pixels need no repeated OCR. Strong visual clearing can dismiss while
OCR is busy. Ambiguous backgrounds and recognition failures receive a bounded
hold, not an immediate empty-text verdict. Edge comparison is a scheduling and
visibility hint, not semantic text recognition.

The ignored native test in `tests.rs` exercises ROI capture. Lifecycle tests cover
Quit, region invalidation and hidden editing controls. Result pixels are omitted
from OS screen capture while this mode is running; ordinary capture is restored
on Quit.
