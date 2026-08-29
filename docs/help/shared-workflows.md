# Shared workflows and results

Screen Goated Toolbox combines reusable AI presets with focused mini apps. Windows and Android share behavior where the same feature is available, although packaging and system permissions differ.

## Presets and API keys

Presets describe an input, model, instructions, and output behavior. A preset may use text, an image or screen capture, microphone audio, or device audio. Add the API key required by the selected model in Settings. A missing or rejected key is reported as an error; the app does not silently switch to an unrelated provider.

## Live transcription and translation

Live workflows capture microphone or device audio and show partial and final text in an overlay. Pause stops sending new audio without discarding the current session. Stop finalizes the current result. Availability of device or per-app audio depends on platform support and permission state.

## Text to speech

Text-to-speech reads entered or generated text with the selected voice. Cloud voices need network access and the matching API configuration. Local voices may require a one-time verified tool or model download.

## Creation projects

Image to 3D, Image to SVG, and Image Creator use project-style histories. A project keeps its source, previews, job state, and generated artifacts so the user can return to it. Generation can continue after its mini-app window closes. The app keeps results in the project until the user explicitly downloads the artifact they want.

Image to 3D accepts a reference image and produces a model preview. Available refinement actions depend on the completed artifact and current account capability. Image to SVG converts a reference image into scalable vector output with configurable detail and segmentation. Image Creator produces new images from instructions and optional references.

Downloaded artifacts are user-owned files. When a user chooses Download, the app publishes the selected result to the system Downloads collection. Internal project files are not presented as the user's final export.

## Downloaded Tools

Large runtimes, models, and workers install on demand. Opening a feature automatically prepares its required verified components. Downloaded Tools shows their live state and supports repair or removal. Removing a tool first stops or cancels its owning work, waits for active use to end, and preserves user-created results, settings, and history.

## Help Assistant

Help Assistant answers using a small verified product-help dataset selected for the current platform. The dataset downloads once and is cached. If an update cannot be downloaded, the last verified copy remains usable offline. AI-generated answers still require the configured model and network access.
