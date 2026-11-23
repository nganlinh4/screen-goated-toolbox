# History UI Parity

## Canonical Source
- Windows entrypoints:
  - `src/gui/app/rendering/title_bar.rs`
  - `src/gui/app/rendering/mod.rs`
  - `src/gui/settings_ui/mod.rs`
- Supporting state/logic:
  - `src/history.rs`
  - `src/history/worker.rs`
  - `src/gui/app/types.rs`
- UI/output owners:
  - `src/gui/settings_ui/history.rs`
- If Windows uses HTML/CSS/JS/WebView, name that web surface explicitly as canonical:
  - Not applicable. Windows history is a native `egui` surface.

## Behavior Contract
- User-visible flow:
  - History is the 4th main-shell tab.
  - Header contains title, max-items control, search, open-folder, and clear-all actions.
  - Results are newest-first and searchable by result text or timestamp only.
  - Each card shows icon, timestamp, result text, and explicit inline actions.
- State model:
  - `HistoryType = Image | Audio | Text`
  - `HistoryItem = { id, timestamp, itemType, text, mediaPath, optional mediaProof }`
  - Text items display result text while their backing `.txt` file stores the original input/source text.
  - Max history items default to Windows `50` and clamp to `10..200`.
- Transition rules:
  - Visible results represent committed history. Save,delete,clear and retention
    changes publish only after the database replacement succeeds. Failed writes
    preserve the previous visible items and backing files,report an error,and
    retire any uncommitted media created by the failed save.
  - One persistence worker orders changes. Clear retires older queued or
    media-prepared saves. An operation already admitted to database persistence
    completes before later queued operations;slow writes do not hold the UI cache
    lock or block clear/cancellation admission.
  - Startup retention uses the same commit boundary. Failure keeps the loaded
    snapshot and its media;success publishes the pruned snapshot before cleanup.
  - A database read or parse failure never authorizes overwriting it with an
    empty history. The section reports unavailable storage and mutations remain
    disabled until the repository is reopened after storage repair.
  - Save inserts at the front.
  - Delete removes the committed card, then applies the same ownership gates to its backing file.
  - An open action must resolve an item against the current committed snapshot;
    stale items, ambiguous IDs and unavailable storage cannot authorize an open.
    Media names must be a single nonblank leaf without separators, colon or
    control characters before either platform constructs a media path or grant.
    An item's backing filename must have the extension produced for its type:
    PNG for images, WAV for audio, and TXT for text, ignoring case.
    Descriptor handoff is admitted against that snapshot atomically with state
    publication, after file verification and without holding the state lock while
    reading file contents.
  - Clear-all removes committed cards. Cleanup removes only unreferenced backing
    files whose stored identity, length and digest still match. Legacy files
    without ownership proof, changed files and unreferenced unknown files remain.
    A replacement after an identity observation must never be deleted, including
    when it contains identical bytes. A media open must bind the object that was
    checked; a path returned after the check does not establish that binding.
    Android moves the entry using retained directory descriptors into a unique
    retirement slot and audits the moved object before deleting. A mismatch is
    restored only if the original name is vacant. A slot that cannot safely be
    restored remains preserved.
  - Prune removes oldest committed items until the limit is satisfied, then
    applies the same ownership gates to their backing files.
  - Rapid saves must create unique backing filenames so one card can never overwrite another card's media/source file.
  - A new media reference carries the identity obtained from its retained
    creation handle. Sync and a bounded content fingerprint must complete on
    that same object before publication. A replacement name never acquires
    ownership proof merely by having the same bytes.
  - The creation lease remains open through database outcome handling. Revalidate
    it before persistence and after a successful write before visible publication.
    A post-write media/route mismatch is uncertain: retain files, keep the prior
    cache and disable mutation until storage is reopened and repaired.
  - Retaining a directory location does not require listing its entries.
    Subsequent file reads and mutations still require their own search, read
    and write permissions; a retained directory never grants those permissions.
  - Database and settings publication uses descriptor-relative no-replace for
    an absent target, or exchange for an existing target. Audit the installed
    creation identity and bytes and the actual displaced version before reporting
    success. A competing replacement must survive an uncertain exchange.
    Unsupported atomic operations have no path-based replacement fallback.
  - File-content sync does not make a new filename or rename durable. Sync the
    containing directory after creating media/staging names and after database
    publication before reporting success. Newly created directory entries require
    sync of their parents, from the deepest changed parent outward.
    Directory sync has its own permissions/capability requirements; retaining an
    O_PATH location does not grant them. Failure after publication is uncertain.
  - Database/settings writes keep one bounded ownership receipt and reusable
    staging slot per target. An exclusive descriptor lock coordinates owners.
    Persist the intent before creating or rewriting a slot, and its complete
    identity/fingerprint before publication. Reopen accepts only an exact known
    target/slot pair; partial writes, torn receipts and foreign entries remain
    preserved with mutation disabled. Recovery of an applied but uncertain rename
    syncs and revalidates its directory before exposing that snapshot.
  - A writer retains its loaded target descriptor until a verified successor is
    pinned or the writer is closed, so its live-process identity cannot be
    recycled while it authorizes a write. Read-only snapshots do not retain it.
  - Search filters only on `item.text` and `timestamp`.
  - Mobile settings reset restores the History max-items setting to the Windows default `50`.
  - Legacy Android settings snapshots that still carry the old implicit `200` max-items default must normalize back to the Windows default `50` unless the user explicitly changed the limit.
- Output contract:
  - Android uses the same model semantics and ordering as Windows.
  - Android file opens use external intents through a History-specific provider
    that validates the current item and returns a verified read-only descriptor.
    The granted URI identifies the complete item record as well as its media proof.
    Open initiation observes identity and contents through one retained
    descriptor; the provider checks the same proof on its own descriptor.
    Media observation runs off the UI thread, with the result delivered only
    while the History surface remains active.
    For legacy items without a stored proof, that initial observation cannot
    establish the original file's provenance.
  - Android folder opens reuse the existing DocumentsContract-style folder launcher when the history root is externally addressable.

## Failure And Recovery
- Permission/runtime failures:
  - If Android cannot expose the history folder externally, keep the button visible and surface a localized “folder unavailable” toast.
  - If a backing file is missing, opening the item fails gracefully with a localized toast.
- Timeout/retry behavior:
  - Not applicable.
- Stop/reset behavior:
  - Live translate flushes any pending committed segment into history before the session is marked stopped.

## Fixtures
- Shared fixtures:
  - `parity-fixtures/history-ui/state-machine.json`
  - `parity-fixtures/history-ui/commit-boundaries.json`
  - `parity-fixtures/history-ui/media-creation.json`
  - `parity-fixtures/history-ui/namespace-access.json`
  - `parity-fixtures/history-ui/media-lease.json`
  - `parity-fixtures/history-ui/publication.json`
  - `parity-fixtures/history-ui/directory-durability.json`
  - `parity-fixtures/history-ui/publication-recovery.json`
  - `parity-fixtures/history-ui/media-namespace-races.json`
  - `parity-fixtures/history-ui/loaded-identity.json`
  - `parity-fixtures/history-ui/media-names.json`
  - `parity-fixtures/history-ui/media-types.json`
  - `parity-fixtures/history-ui/startup-ownership.json`
- Platform-specific tests:
  - Windows `src/history/state_machine_tests.rs` replays the applicable shared
    state-machine cases through the persistence worker, database reload and
    backing-file reads. Android-only legacy settings migration remains Android
    coverage. Search replay calls the same predicate as the native History UI.
  - Android repository/filter tests
  - Android producer-hook tests for preset and live-save paths

## Deviations
- Android records live-translate history as committed text segments because its runtime does not persist reusable WAV blobs like the Windows audio-history path.
- Android settings and database replacements commit separately. A committed
  retention setting may remain selected when pruning the database fails; loaded
  items and media remain until a subsequent database commit succeeds.
- Android defers queued mutations until secure bootstrap completes. Startup
  retention runs first through that same worker; loading never rewrites settings.
- If Android bootstrap fails after History loads, its deferred worker is
  cancelled and its retained storage descriptors are closed before retry.
- Atomic replacement is required, with no copy or non-atomic fallback. A changed
  target or an uncertain replacement disables mutations until storage is reopened.
- Android database/settings use bounded receipt and staging slots. Verified
  displaced objects can be reused through their descriptors; partial or unknown
  objects remain preserved and unavailable. Media ownership intents and
  loaded-file identity across reopen remain
  incomplete. Windows database publication still needs its namespace audit and
  fixture replay.
- Descriptor publication, conditional media removal and FileProvider open race
  acceptance remain platform verification requirements; JVM namespace modeling
  alone does not establish those guarantees or directory power-loss durability.
- Android enforces directory-sync ordering in storage; actual filesystem
  power-loss behavior and the corresponding Windows durability replay remain
  separate acceptance requirements.
