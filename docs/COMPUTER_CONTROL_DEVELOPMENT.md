# Computer Control Development Contract

This document is normative for changes under `src/overlay/computer_control`.

## Core architecture

- The live model owns semantic interpretation of user speech. Production Rust must not infer tool permission, denial, routing, completion, or intent from language-specific words, phrase lists, substring checks, transcript length, or punctuation.
- Every normal user turn receives the full tool catalog. Tool availability must not depend on a conversational, read-only, view-only, locale, or keyword-derived mode.
- Lifecycle state is derived from observable events: user turn arrival, selected tool capability, pending job identity, tool result, model turn boundary, cancellation, reconnect, and verified postcondition.
- A tool call must execute as requested unless a structural invariant blocks it. The harness must not silently replace one tool with another based on transcript wording.

## Allowed deterministic gates

Code-owned gates must be language-neutral and adjacent to the protected effect:

- one in-flight job identity and monotonic cancellation;
- stale UI element and stale frame rejection;
- input-injection success accounting;
- required-field validation from UI structure;
- consequential-action checkpoints derived from element risk and explicit model confirmation;
- postcondition and independent completion verification;
- audio-safe session replacement and bounded reconnects.

Do not add a gate because one transcript, application, website, language, or model run failed. Generalize failures into capability, lifecycle, transport, or evidence invariants.

## Prohibited patterns

- Natural-language allowlists or denylists for mouse, keyboard, browser, shell, submit, send, or integration tools.
- Phrase-based read-only/view-only/conversation modes.
- Transcript-keyword rerouting from the model's requested tool to another tool.
- Product-, site-, game-, person-, language-, or incident-specific prompt rules and regression cases.
- Synthetic continuation turns after the model has completed a turn.
- Treating missing page data as proof that content does not exist.

## Testing and evidence

- Test capability matrices and state transitions, not example utterance dictionaries.
- Include unknown/future tools in structural tests so integrations remain powerful by default.
- Validate hard fixes with a real scripted Live run when possible.
- Inspect structured `events.jsonl`; console text is diagnostic presentation, not authoritative state.
- Before committing, run `cargo test`, `cargo clippy --all-targets -- -D warnings`, and scan touched Computer Control files for leaked incident terms or phrase tables.
