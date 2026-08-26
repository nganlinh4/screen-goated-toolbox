# Optional Component Delivery

SGT ships a thin signed host. Optional interfaces, native runtimes, workers,
models, and external tools are installed only when a user opens or selects the
capability that needs them. Downloaded Tools is the product surface for repair,
status, and removal.

## Delivery source

Use an immutable upstream release or content-addressed model revision when one
exists. If an artifact has no suitable durable host, publish it as a uniquely
versioned asset on the
[`sgt-runtime-bundles`](https://github.com/nganlinh4/screen-goated-toolbox/releases/tag/sgt-runtime-bundles)
release. That release is append-only: never replace or delete an asset that a
released host can reference.

Mutable branches, nightly aliases, floating model revisions, and guessed URLs
are not executable delivery contracts. HTTPS provides transport; the signed
host's exact size and SHA-256 record provides artifact identity.

## Development, staging, and production channels

Component work has three storage layers with different trust and retention
rules:

1. The managed local cache under `%LOCALAPPDATA%/SGT-Development/cache` holds
   two Cargo lanes, package jobs, staging contracts, and test evidence. It is
   bounded and disposable; it is never an application delivery source.
2. The mutable `sgt-runtime-staging` prerelease holds only current development
   candidates. A candidate keeps a content-addressed filename, is read back and
   hashed after upload, and may replace the previous candidate for the same
   contract. Only an explicitly opted-in debug build may use this tag.
3. The immutable `sgt-runtime-bundles` release holds promoted production bytes.
   Promotion copies the exact verified staging bytes, reads them back, writes a
   production URL into the reviewed contract, and never overwrites or deletes a
   published asset.

Do not create a release asset for every edit. Rebuild locally as often as
needed, replace the staging candidate when device or first-use testing needs a
remote object, and promote once when the component is ready for an app release.
A published host references production only, so users on an existing app
version do not redownload while development candidates change.

The staging index and contracts contain no local paths. A debug staging build
uses an isolated component registry and disables production catalog discovery;
unchanged contracts fall back to their tracked production records. A release
build fails immediately if staging selection is present.

## Source-change rule

Any code or frontend change that changes an optional component's packaged bytes
invalidates that component's current delivery checkpoint. Rebuild only the
affected deterministic package. If its digest changed, upload a new asset whose
name contains the component version and digest prefix, read the published bytes
back, and update the tracked delivery contract before releasing the host. If the
digest did not change, reuse the existing asset and do not upload a duplicate.

The release tag is an artifact store, not a mutable update channel. Never replace
an existing asset under the same name and never delete an asset while any signed
host may reference it. Development may prepare packages locally, but debug and
release application code must exercise the same external verified contract; it
must not discover or fall back to a developer build directory.

Use `scripts/build-component-candidate.ps1` for standard Windows components and
`scripts/component_release.py` for staging, verification, and promotion. Local
package output belongs in the managed cache, not `local-runtime-bundles/` or a
new repository target directory. Package scripts accept an explicit output
directory; worker builds share the package Cargo lane.

## Package boundaries

- Keep every user-visible capability independently installable and removable.
- Present one user-visible product as one Downloaded Tools lifecycle even when
  its verified delivery currently uses multiple internal artifacts. First use
  may prepare those artifacts concurrently, but status, repair, and removal
  belong to the product and Clean All counts it once.
- Share a dependency through an explicit reference, not by copying it into each
  feature package.
- Do not combine optional capabilities into a single convenience bundle.
- Keep user-created recordings, exports, history, settings, and source files
  outside component-owned directories.
- A host built without verified delivery metadata fails closed at first use and
  keeps core startup operational.

## Installation contract

Each delivery record declares a component identifier, version, architecture,
host compatibility, dependency identifiers, exact archive size and SHA-256,
and an exact inventory of installed files.

The installer must:

1. deduplicate concurrent requests for the same component;
2. download to a bounded staging location;
3. verify archive size, digest, format, entry count, paths, and expanded size;
4. reject traversal, absolute paths, reparse points, links, and undeclared files;
5. write an ownership receipt for the verified inventory;
6. atomically publish an absent version directory; and
7. retain the previous usable version until the new version is verified.

Executable workers additionally require architecture and protocol compatibility
checks before launch. A signed catalog or host update may select a newer version;
downloaded code never selects its own update source.

## Update and launch policy

The signed component catalog is bounded by minimum and maximum host versions.
It may select only immutable contracts with the same content-addressed URL,
archive digest, expanded inventory, and protocol checks required at build time.
Catalog discovery never turns HTTPS, a release filename, or a self-reported
version into trust.

- Mini-app UI packs check the app-selected compatible contract before open. A
  missing or newer pack downloads in the background, verifies, installs, and
  then opens without requiring a second click.
- Recorder and other workers update at their next open/session boundary, retain
  their component leases and file handles for the full process lifetime, and
  keep user-created output outside the component root.
- Creation is not a delivery exception. Its Windows executable and Android
  runtime artifacts are content-addressed, fully hashed, and explicitly tied to
  the main app version. A job repairs the app-selected version first; before job
  acceptance, a typed start failure may select one newer signed contract that is
  compatible with that same host and retry once.
- yt-dlp and Deno may select a newer signed contract after a typed tool failure
  and retry the operation once. They never invoke an upstream self-updater.
- FFmpeg checks for an app-approved signed update while idle, no more than the
  catalog interval, and keeps the current verified version if no update exists.
- Native dependencies and models advance only through an app-approved catalog
  contract. They do not update themselves merely because a consumer failed.

If update discovery is offline or unavailable, an already selected verified
contract remains authoritative. A newly selected package is never published
into its version directory until its archive and complete installed inventory
pass verification. Old version directories remain available for safe rollback
and are removed only through ordinary managed removal.

External process tools follow the same rule. Windows owns `yt-dlp-x64`,
`ffmpeg-x64`, and `deno-x64` separately, holds every executable and notice file
open with read-only sharing for the complete child-process lifetime, and never
exposes a self-update or “latest” operation. Legacy shared-bin bytes are adopted
only when their complete size and SHA-256 inventory matches the signed host.
Downloaded workers never discover these tools through `PATH`, a system install,
or a developer directory. Before spawning a worker, the signed mother SGT host
resolves every declared tool capability from its component registry, installs
and verifies any missing contract on demand, passes the canonical managed path
explicitly, and retains the component lease and locked inventory for the whole
worker session. A typed missing-capability response may trigger the same
host-owned resolver and one retry; it must never tell the worker to install or
locate the tool itself.
WebView2 is different: SGT verifies and runs an exact Microsoft-signed bootstrapper,
while the installed Evergreen Runtime remains shared, system-managed software.

## Use and removal contract

Every active runtime, worker, model session, and feature install holds a lease.
Removal rejects new leases, cancels or drains installation, and becomes pending
until active leases end. A native library that cannot unload remains pending
until process restart.

Removal first stops or cancels the owning feature and waits for its processes
and leases to finish. It then deletes regular, non-reparse files listed in a
valid ownership receipt, even if a recorded file changed after installation.
Missing files are harmless; unrecorded and unsafe entries are preserved and
reported. Clean All uses the same owner-shutdown and receipt-bounded removal
contract; it must not delete broad application-data roots or rely on a
hard-coded component list.

Repair atomically moves an invalid managed version into a visible recovery
directory and records its exact inventory. Downloaded Tools reports the reason
and recovery path. Routine recovery cleanup removes only record-declared
regular files whose size and digest still match. An explicit, confirmed Clean
All removes changed record-declared regular files too; reparse and unknown
bytes remain visible and preserved.

Shared dependencies are removed only when no installed component or active
lease references them. Interrupted installs and pending removals are reconciled
on startup without triggering a download.

Each registry mutation releases the cross-process mutation guard before
resuming unrelated pending removals. Waiting for one component's active lease
must not monopolize the registry or make another component report that the
registry is busy.

Downloaded Tools status is live state, not dialog-open state. Completion of a
background install, repair, or removal must invalidate the affected presence
and size caches and wake the UI. Acceptance must observe the correct status and
installed size without closing or reopening the dialog.

## Release gate

Before building a release host:

1. produce deterministic component archives and delivery manifests;
2. confirm every asset name is new on its selected release;
3. upload without overwrite flags;
4. download the published asset again and verify size and SHA-256;
5. build every platform/flavor from the same reviewed delivery catalog; and
6. test first use, offline failure, cancellation, repair, concurrent use,
   active-use removal, restart completion, and Clean All preservation.

Publishing component artifacts and publishing an application release are
separate checkpoints. Preparing a host change does not authorize either one.
