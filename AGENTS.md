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
- Use `rg`. Use `apply_patch`. Keep git operations non-destructive.
- Never run `cargo build --release`; owner runs release builds.

## Verify

- Rust: `cargo test`; `cargo clippy --all-targets -- -D warnings`.
- Format: `cargo fmt`; inspect `git diff --check`.
- Windows targets: `scripts/validate-windows-targets.ps1` when target-sensitive.
- Frontend: run package typecheck/tests named in subsystem README.
- Docs: verify every path/command against current tree. Do not edit vendored upstream docs unless SGT owns overlay note.

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
- Android workflow and machine paths: `mobile/README.md`.

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

## Docs Map

- Product/build: `README.md`, `docs/DEVELOPMENT.md`.
- Release: `docs/RELEASING.md`.
- Windows ARM64: `docs/WINDOWS_ARM64_SUPPORT.md`.
- Subsystem details: nearest `README.md`; keep one owner per fact.
