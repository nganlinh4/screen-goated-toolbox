# Preset System Parity

## Canonical Source

- Windows editor/catalog: [preset settings](../../src/gui/settings_ui/preset.rs), [preset model](../../src/config/preset/preset.rs)
- Windows execution: [chain](../../src/overlay/process/chain), [text input](../../src/overlay/text_input), [result](../../src/overlay/result), [favorite bubble](../../src/overlay/favorite_bubble)
- Shared model catalog: [catalog/model_catalog.json](../../catalog/model_catalog.json)
- Android preset model/runtime: [shared preset](../../mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/preset), [Android preset](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset), [overlay host](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset)

## Data and Editor Contract

- Windows built-ins are canonical seed data. Android persists user overrides by preset ID; restore removes the override.
- Favorite state, ordering, edits, and custom presets are repository-backed, never Compose-only state.
- Android supports preset creation, duplication, deletion, and the current node-graph editor actions. Capability UI must still reflect real runtime support for every block/provider.
- Unknown/corrupt override fields fall back safely to canonical built-ins.
- Hotkeys and controller/master invocation remain Windows-only until Android has a real equivalent.

## Launch and Overlay Contract

- Presets execute from the floating bubble service. Zero favorites shows a localized empty state.
- The panel preserves Windows keep-open, size, multi-column, overlap, animation, drag/reposition, and refresh semantics through a thin Android bridge.
- Input uses the Windows text-input DOM/CSS/message contract, including submit, cancel, history, close, and working microphone input.
- Permission-gated image/audio paths fail before capture, explain the required Android permission, and preserve retry state.
- Image presets support continuous relaunch. Non-image continuous mode remains a documented gap.
- Result windows are session-owned, precreated in loading state, multi-window, and support markdown streaming or raw HTML according to block render mode.
- Reuse Windows markdown fitting/theme/font/table and button-canvas contracts. Preserve text selection, one-finger window drag, two-finger bidirectional content scroll, navigation recovery, and result geometry ownership.
- Edit/refine, undo/redo, share/download, and speaker actions are real Android actions. Do not list implemented actions as placeholders.
- Android still omits the desktop markdown/plain toggle and broom mouse-button variants.

## Provider Contract

- Resolve every internal model ID through generated data from `catalog/model_catalog.json`; call the resolved provider and `full_name`.
- Preserve Windows render-mode, streaming, thinking/search gating, provider-availability, retry, and fallback semantics.
- Gemini Live setup uses the catalog-owned output ceiling for each endpoint: 8,192 for Live 2.5 and 65,536 for Live 3.1, on both Windows and Android.
- Every native Live setup envelope is built through the platform's typed setup builder; endpoint policy is applied by construction and feature adapters supply only capability deltas.
- Live server events are decoded structurally. Setup completion must be a top-level field, all audio parts in a frame are retained, and finite responses complete on either `turnComplete` or `generationComplete`.
- Blank, legacy, or unknown Gemini TTS model values normalize to the catalog-owned TTS default on both platforms; listed models remain unchanged.
- Provider/auth failures and retryable model failures remain distinct. Retrying an open result updates its loading status.
- Cerebras vision is base64 PNG/JPEG through `gemma-4-31b` only. It uses the Cerebras key and endpoint on both platforms; it must never fall through to Groq.
- Vision payloads preserve their real MIME type. Groq images use a prompt-aware encoded-byte budget below the provider request ceiling: keep PNG when it fits, otherwise use adaptive JPEG compression and resizing before sending. Qwen vision uses 2,048 maximum completion tokens and a conservative local preflight for the portable 8,000-TPM tier; prompts that cannot leave the fixture-owned image/envelope reserve fail before image encoding or network I/O. Other Groq vision models leave the ceiling unset. Qwen reasoning stays at provider default with `reasoning_format: hidden`; only when a completed response contains no final content does the client retry once with reasoning disabled. A token-rate 429 may retry once when Groq's structural `retry-after` is at most 30 seconds; otherwise preserve the provider error and continue the normal fallback chain. Windows and Android use the same contract.
- The canonical general image retry order is Cerebras Gemma 4 31B, Groq Scout, Groq Qwen 3.6 27B, Gemini 3.1 Flash Lite, Gemini Live 3.1, then Gemini 2.5 Flash. Gemini 2.5 Flash Lite remains selectable but is not in the automatic chain.
- Computer-control pixel grounding has a separate fail-closed chain: Gemini 3.1 Flash Lite only by default (`CC_VISION_MODEL` may explicitly override it). General OCR/description fallbacks never inherit authority to click. Coordinate clicks require a fresh marked-crop verification at 70% confidence; `CC_VERIFY_LOCATE=0` is a diagnostic escape hatch, not a preset default.
- A vision model that returns a rate-limit/quota error enters a five-minute in-process cooldown, preventing later chain steps from repeatedly paying for the same known failure. Small provider `retry-after` recovery remains bounded inside the request.
- Deprecated Cerebras Llama 3.1 8B and Qwen 3 235B selections normalize to GPT-OSS 120B before execution.
- Cerebras requests set a bounded `max_completion_tokens`, keep provider reasoning defaults, gzip JSON bodies at 12 KiB, and expose daily-request plus token-minute rate data.
- Refine operations may attach Cerebras predicted output only for GPT-OSS 120B and GLM 4.7. Ordinary generation and Gemma never receive prediction fields.
- Cerebras structured-output requests use strict schemas only on models that document constrained decoding. Gemma 4 vision omits `response_format` because that model rejects it. Tool requests are a separate contract, permit parallel calls, and must stop after eight rounds; tools, prediction, and `response_format` are never combined illegally.
- Hidden blocks execute without windows; each visible result block owns its own result window.
- Unsupported graph/provider paths return an explicit reason. Never guess from ID prefixes.

## Fixtures

- [audio-runtime.json](../../parity-fixtures/preset-system/audio-runtime.json)
- [catalog-overrides.json](../../parity-fixtures/preset-system/catalog-overrides.json)
- [custom-models-dialog.json](../../parity-fixtures/preset-system/custom-models-dialog.json)
- [gemini-live-socket-protocol.json](../../parity-fixtures/preset-system/gemini-live-socket-protocol.json)
- [node-graph-editor.json](../../parity-fixtures/preset-system/node-graph-editor.json)
- [result-overlay.json](../../parity-fixtures/preset-system/result-overlay.json)
- [retry-runtime.json](../../parity-fixtures/preset-system/retry-runtime.json)
- [text-input-overlay.json](../../parity-fixtures/preset-system/text-input-overlay.json)
- [text-provider-routing.json](../../parity-fixtures/preset-system/text-provider-routing.json)
- [vision-payload.json](../../parity-fixtures/preset-system/vision-payload.json)

## Known Contract Debt

- `result-overlay.json` still marks implemented result actions unsupported.
- `text-input-overlay.json` still labels microphone input deferred.
- One `catalog-overrides.json` case name says HTML is a placeholder although its expected result is supported.

Treat these as fixture/source synchronization work, not as permission to restore old behavior.
