# Image to 3D Parity

## Canonical Source

- Windows launcher and host:
  [three_d_generator](../../src/overlay/three_d_generator)
- Windows web surface: [3d-generator-ui](../../3d-generator-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-3d/state-contract.json)
- Android native creation shell:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- The app is named Image to 3D / 이미지 3D 변환 / Ảnh sang 3D.
- Mode appears before topology:

  - Fast / 빠름 / Nhanh supports 100–15,000 polygons, returns a segmented
    result, and hides automatic separation.
  - Quality / 품질 / Tốt supports 500–20,000 polygons and offers optional
    automatic separation. Quality is the default.

- The selected mode, topology, separation choice, prompt, source, and output
  destination are frozen before a request enters the queue.
- Every job has exactly one source image. Picking or dropping several images
  creates independent one-image sessions; references are never combined into a
  single 3D job.
- The primary action submits only the selected session. Other sessions from the
  same import remain drafts; parallel work requires an explicit submission for
  each session.
- At most two jobs run simultaneously. Four isolated execution slots are
  maintained without consuming SVG or image-editing job slots.
- Progress preserves `preparing`, `generating`, `segmenting`, `finalizing`,
  `done`, `failed`, and `cancelled`.
- Preview setup is visually silent, does not block generation, and cannot turn
  a creation job into a failure.
- Queue thumbnails and the selected source use bounded preview derivatives.
  Original image bytes are never retained by the WebView, session creation is
  immediate, generation remains available while previews load, and preview
  decoding never blocks the WebView message thread. Only the selected or
  near-visible queue entries hydrate; off-screen history waits until it
  approaches the viewport, and hydration yields to model interaction.
- A successful result is a validated GLB with site-neutral naming. Geometry
  metadata is reported when known.
- Fast results and quality results completed with automatic separation are
  already segmented. An eligible unsegmented quality result may expose a
  separate continuation for 24 hours.
- Segmented output preserves existing meaningful part nodes. A single-mesh
  result may be expanded by disconnected components when that produces useful
  parts.
- Durable history stores the frozen product settings and result metadata.
  Missing outputs are hidden; rename and delete operate on the real file.
- A terminal item may be reconfigured and submitted as a new job without
  mutating its previous result.

## Shared Visual System

- Windows and Android expose only Fast and Quality. No implementation branding,
  selection, progress text, or error text appears.
- Android uses the shared native Material 3 Expressive creation shell and the
  same settings order, limits, stage mapping, preview math, and result metadata
  as Windows.
- Viewer controls cover orbit, zoom, pan, grid, wireframe, auto-rotate, toon,
  and outline. Wireframe and outline remain independent.
- Filled Material Symbols Rounded come from the shared SGT icon catalog; the
  mini app does not carry a separate hand-authored icon set.
- An unchanged poll or thumbnail completion preserves existing queue DOM.
  Pointer hover, focus, and the first selection click cannot be invalidated by
  background reconciliation. Interactive orbit renders at full input cadence
  even when the history contains many items.

## Public Boundary

- Public source, fixtures, tests, diagnostics, history, and UI describe only
  product settings, jobs, stages, artifacts, and stable IPC state.
- Implementation-specific mechanics and raw errors remain outside this
  repository's public contracts and presentation.
- Every inbound event is normalized before storage, logging, or display.

## Failure And Recovery

- Submission and recovery are bounded and fail closed when acceptance cannot be
  proven.
- An accepted job is never submitted twice during recovery.
- Cancellation wins over late success and a stale callback cannot affect a
  newer worker assignment.
- A success event is emitted only after the output has been validated and
  committed.
- Closing the mini app cancels all of its queued and running jobs, terminates
  their tracked process trees, destroys the WebView, and prevents a late
  completion from publishing. Shared proactive preparation remains app-owned
  and is not mistaken for a mini-app job.

## Verification

- Shared fixture: `parity-fixtures/image-to-3d/state-contract.json`
- Windows routing tests verify mode limits, topology clamping, separation
  visibility, queue state, cancellation, result validation, and history.
- Android Full and Play tests read the same product fixture and verify the
  native shell, worker isolation, preview contract, and result behavior.

## Platform Deviations

- Windows writes to a selected filesystem folder. Android publishes through
  MediaStore or a persisted Storage Access Framework directory.
- Android uses SceneView/Filament for the adaptive result surface; Windows uses
  its desktop viewer.
- Platform delivery differs, but product settings, progress, cancellation,
  artifact, continuation, and history behavior remains identical.
