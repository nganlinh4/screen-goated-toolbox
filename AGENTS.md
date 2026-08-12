# SGT Agent Rules

## Prime

- Work from repo root. Read nearest subsystem doc before edits.
- User scope wins. Preserve dirty/unrelated work. Never reset or overwrite it.
- Nontrivial task: state exact check first. Test competing causes. Verify with code, logs, tests, or renders.
- Hard result with `reasoning_output_tokens` exactly `516`, `1034`, or `1552`: distrust; rerun/second-pass.
- Hard Codex work: `model_reasoning_effort=xhigh` unless user asks speed.
- No filler. Short, concrete updates. Outcome first.

## Code

- Rust edition 2024. Windows app. `eframe` + `wgpu`; WebView2 mini apps; Android companion.
- Max 600 lines per source file. Split by responsibility before limit.
- Use `anyhow::Result`. Remove dead code. Never add `#[allow(dead_code)]`.
- No warnings. No incident-, app-, site-, person-, game-, model-run-, or language-specific hacks in reusable code, prompts, comments, or tests.
- Prefer general capability/state invariants. Unknown future integrations must keep working.
- Public creation code, docs, fixtures, and tests contain only product behavior,
  stable public contracts, and delivery invariants. Do not document non-public
  implementation details, compatibility evidence, or real-run artifacts.
- Use `rg`. Use `apply_patch`. Keep git operations non-destructive.

## Verify

- Rust: `cargo test`; `cargo clippy --all-targets -- -D warnings`.
- Format: `cargo fmt`; inspect `git diff --check`.
- Windows targets: `scripts/validate-windows-targets.ps1` when target-sensitive.
- Frontend: run package typecheck/tests named in subsystem README.
- Docs: verify every path/command against current tree. Do not edit vendored upstream docs unless SGT owns overlay note.
- Real UI acceptance is complete only after the repaired build succeeds
  end-to-end and its committed artifact is validated. After each failure,
  inspect fresh evidence, diagnose, repair, run focused checks, and rerun with a
  new evidence directory. Never abandon the rerun because an earlier repair
  also failed. Stop only for a safety boundary or a genuinely external blocker;
  report that state as blocked, never passed or complete.

## Computer Control

- Must follow `docs/COMPUTER_CONTROL_DEVELOPMENT.md`.
- Model owns language meaning. Full tool catalog every normal turn. No phrase/keyword permission gates or reroutes.
- Code gates only structural effects: job identity, cancellation, stale targets, required fields, consequential checkpoints, postconditions, reconnect/audio safety.

## Windows ↔ Android

- Windows behavior canonical for parity features.
- Before parity change: update `.claude/parity/<feature>.md` and shared fixture under `parity-fixtures/`.
- Use `.claude/skills/enforce-mobile-parity/SKILL.md`.
- Port Windows state/HTML contract; thin platform shim only. No guessed mobile redesign or duplicated core logic.
- Divergent glue, repeated fixes, or parity monolith: rewrite from canonical Windows architecture.
- Android rules: `mobile/AGENTS.md`. Workflow and machine paths: `mobile/README.md`.

## Screen Recorder

- Semantic kebab-case class on JSX elements.
- Preview = export. One parameter/math source; no separate look tuning.
- Background catalog: `screen-record/src/config/shared-background-presets.json`.
- UI icons: `node screen-record/scripts/add_material_icon.mjs <material-symbol-name>`.
- Cursor packs: single clipped `44x43` SVG per cursor; mirror dev + packaged assets; verify preview/export parity.
- UI work: `.claude/skills/update-frontend/SKILL.md`.

## Catalog Work

- Models: `.claude/commands/manage-model-catalog.md`.
- Recorder backgrounds: `.claude/commands/manage-background-presets.md`.

## Optional Component Delivery

- Before changing a downloadable mini-app, worker, runtime, model package, or
  external-tool contract, read `docs/COMPONENT_DELIVERY.md`.
- A source change that changes packaged bytes requires rebuilding only the
  affected deterministic package. Upload the newly content-addressed asset to
  the append-only `sgt-runtime-bundles` release, read it back, update the exact
  size/SHA-256 delivery contract, and only then build the host release.
- Never overwrite or delete a published asset while a released host can
  reference it. If rebuilt bytes have the same hash, do not upload a duplicate.
- Debug and release hosts use app-selected external delivery contracts. Do not
  add local package fallbacks, floating URLs, self-updaters, or bundled payloads.
- First use/update must verify, install, and then open automatically. Removal
  must stop the owning UI/process, wait for leases, and delete only receipt-owned
  unchanged files.

## Docs Map

- Product/build: `README.md`, `docs/DEVELOPMENT.md`.
- Release: `docs/RELEASING.md`.
- Subsystem details: nearest `README.md`; keep one owner per fact.
