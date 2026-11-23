# Creation Storage And Destination Availability

## Canonical Source

- Windows delivery: [creation_delivery](../../src/overlay/creation_delivery.rs)
- Windows admission: [admission](../../src/overlay/generation_history/admission.rs)
- Windows ownership protection: [protected](../../src/overlay/generation_history/protected.rs)
- Android delivery: [CreationDeliveryTransaction.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/CreationDeliveryTransaction.kt)
- Android admission: [CreationStorageAdmission.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation/CreationStorageAdmission.kt)
- UI/output owners remain the Image to 3D and Image to SVG surfaces; this
  contract changes no layout or platform bridge.

## Behavior Contract

- An accepted dispatch retains its frozen request, artifact and ownership proof
  until its handoff finishes or its cancellation cleanup is proved complete.
- A temporarily unavailable destination suspends that handoff without invalidating
  unrelated requests, history or destinations. Every other handoff is considered
  independently, subject to its own ownership and capacity checks.
- Reading saved state validates its structure without requiring every output
  destination to be online. Invalid state remains an error and is preserved.
- Pending output paths and captured object identities continue to protect their
  artifacts from cleanup. A saved protection record never grants a mutation.
- Storage accounting retains all pending allocations. Unknown materialization
  counts as still required; a durably prepared publication is not reserved twice.
- A retained delivery keeps its output allocation even after its accepted-request
  row is retired. Matching request and delivery rows reserve each allocation once.
- Recovery also retains accepted requests that have no delivery record yet. A
  temporarily unavailable destination defers only that request; another request
  can recover independently. Retrying preserves its dispatch and frozen bytes.
- Recovery retries unavailable storage with bounded backoff until it can continue,
  the original deadline expires, or the owner closes. Deadline handling must not
  require the external destination to become available first.
- Retained accepted requests count toward their tool's queue capacity even before
  they can be registered in memory. A request present in both places counts once.
- Pending recovery remains active work for status refreshes; an empty visible job
  list cannot stop updates before the retained requests are resolved.
- The selected destination must be available and ownership must be verified
  immediately before publication or cleanup. An unavailable parent is not proof
  that its files have been removed.

## Failure And Recovery

- Unavailable locations retain their dispatch, artifact and cancellation state.
  They do not cause regeneration, publication to another location or silent loss
  of reservations. Retrying resumes only that saved handoff.
- Cancellation is durable for its verified dispatch even when output cleanup
  must wait. Retained cleanup cannot stop another independent handoff.
- Cleanup removes only unchanged owned bytes. Changed or unobservable files are
  preserved; incomplete cleanup cannot be reported as complete.

## Fixtures

- Shared cases: [destination-availability.json](../../parity-fixtures/creation/destination-availability.json)
- Accepted-request recovery: [queued-recovery.json](../../parity-fixtures/creation/queued-recovery.json)
- Windows tests use owned temporary folders, native file identities and directory
  disappearance. Android tests exercise the same accounting and independent
  reconciliation through typed delivery records and the production policy.

## Deviations

- Windows uses filesystem destinations; Android also supports user-granted
  storage providers. Availability and ownership probes are platform shims;
  retained state, accounting and independent recovery follow this contract.
- Android restores managed staging work without querying its SAF output provider.
  Provider availability is resolved by publication. Windows native-directory
  validation can defer replay before dispatch. Both preserve the same accepted
  request and dispatch, and a newly expired request remains visible as failed.
