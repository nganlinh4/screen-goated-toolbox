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

## Package boundaries

- Keep every user-visible capability independently installable and removable.
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

External process tools follow the same rule. Windows owns `yt-dlp-x64`,
`ffmpeg-x64`, and `deno-x64` separately, holds every executable and notice file
open with read-only sharing for the complete child-process lifetime, and never
exposes a self-update or “latest” operation. Legacy shared-bin bytes are adopted
only when their complete size and SHA-256 inventory matches the signed host.
WebView2 is different: SGT verifies and runs an exact Microsoft-signed bootstrapper,
while the installed Evergreen Runtime remains shared, system-managed software.

## Use and removal contract

Every active runtime, worker, model session, and feature install holds a lease.
Removal rejects new leases, cancels or drains installation, and becomes pending
until active leases end. A native library that cannot unload remains pending
until process restart.

Removal deletes only files listed in a valid ownership receipt whose current
size and digest still match. Missing files are harmless. Modified or unexpected
files are preserved and reported. Clean All enumerates the same registry; it
must not delete broad application-data roots or rely on a hard-coded component
list.

Repair atomically moves an invalid managed version into a visible recovery
directory and records its exact inventory. Downloaded Tools reports the reason
and recovery path. Recovery cleanup removes only record-declared regular files
whose size and digest still match; modified, reparse, and unknown bytes remain
visible and preserved.

Shared dependencies are removed only when no installed component or active
lease references them. Interrupted installs and pending removals are reconciled
on startup without triggering a download.

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
