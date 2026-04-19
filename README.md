# Screen Goated Toolbox (SGT)

**The Ultimate AI Productivity Automation Tool for Windows & Android.**

Screen Goated Toolbox (SGT) is a native Windows utility that bridges your screen, system audio, and microphone with the world's most powerful AI models. It allows you to create custom AI workflows using a visual node graph to automate tasks like OCR, translation, meeting transcription, generative audio, and text analysis. SGT also includes a full-featured **Screen Recorder** with GPU-accelerated export and a companion **Android app** for on-the-go live translation.

## Key Features

### Multi-Modal AI Support

* **Cloud Providers:** Native integration with **Groq** (Llama 4 Scout, Whisper), **Google Gemini** (Flash 2.5/3/3.1), **OpenRouter** (Nemotron, DeepSeek Chimera), and **Cerebras** (fast inference).
* **Local AI:** Full support for **Ollama** to run private, local vision and text models without internet.

### Node Graph Workflow

Create complex presets using a visual editor. Connect blocks to define logic:

* **Input:** Screen Region (Snipping), Microphone, System Audio Loopback, or Text Selection.
* **Process:** Chain multiple models (e.g., *Speech to Text* -> *Translate* -> *Summarize*).
* **Output:** Streaming Overlay, Markdown View, Text-to-Speech, or Clipboard.

### Audio Intelligence

* **Real-time "Cabin" Mode:** Live, low-latency transcription and translation overlay. Works with **System Audio** (Zoom/Youtube/Games) or **Microphone**.
* **Per-App Capture:** Target audio from specific running applications.
* **PromptDJ:** A dedicated MIDI-controlled interface for generative music and audio control.

### Screen Recorder

* **GPU-Accelerated Recording:** Multi-monitor window capture with DirectX support.
* **Timeline Editor:** 10 editable tracks — zoom, speed, trim, device audio, mic audio, keystroke display, text overlay, pointer visibility, webcam, and debug.
* **Cinematic Zoom:** Keyframe-based camera motion with smooth Catmull-Rom interpolation and influence points.
* **Cursor Packs:** 12+ built-in cursor style collections with spring physics, wiggle, and tilt effects.
* **Backgrounds:** Built-in gradient presets plus custom image backgrounds with blur, vignette, and color overlay.
* **Export:** MP4 (H.264 via Media Foundation) and GIF output through a zero-copy 3-thread GPU pipeline (wgpu + D3D11 + Media Foundation).

### Productivity Tools

* **Smart Overlays:**
  * **Result Overlay:** Interactive window with streaming text, markdown rendering, and "Refine" chat.
  * **Preset Wheel:** A circular menu (assign a hotkey to any MASTER preset) to quickly select tools.
  * **Favorite Bubble:** A floating dock for instant access to common presets.
* **Text-to-Speech:** High-quality reading using Edge TTS, Gemini Live, or Google Translate.
* **History Gallery:** Auto-saves captures, transcriptions, and generated audio in a searchable database.

### Android Companion App

* **Live Translation:** Real-time transcription and translation via floating overlay or in-app display.
* **On-Device ASR:** Android uses Moonshine Voice (English streaming) + sherpa-onnx Zipformer (7 languages + 8-lang multilingual). Windows uses local Qwen3-ASR and Parakeet paths.
* **Preset Engine:** Same node-graph presets as Windows, with multi-provider AI support (Gemini, Groq, OpenRouter, Cerebras, Ollama).
* **Floating Bubble:** Quick-access overlay bubble for triggering presets from any app.
* **TTS Playback:** Edge TTS and Gemini TTS with speed control.

## Installation

### Option 1: Download Release

Download the latest `.exe` and `.apk` from the [Releases](https://github.com/nganlinh4/screen-goated-toolbox/releases) page.

* **Windows x64:** `ScreenGoatedToolbox_v<version>.exe`
* **Windows arm64:** `ScreenGoatedToolbox_v<version>-arm64.exe`
* **Android:** `ScreenGoatedToolbox_v<version>.apk`

### Option 2: Build from Source (Windows)

**Prerequisites:**

* [Rust](https://www.rust-lang.org/) (Nightly toolchain required).
* [Node.js](https://nodejs.org/) (Required for PromptDJ and Screen Record frontends).
* **Visual Studio Build Tools 2022** with "Desktop development with C++" workload.
* For Windows `arm64` builds: Visual Studio ARM64 MSVC tools and LLVM `clang`.

```bash
git clone https://github.com/nganlinh4/screen-goated-toolbox
cd screen-goated-toolbox

# 1. Setup dependencies and patch libraries
powershell -ExecutionPolicy Bypass -File scripts/setup-egui-snarl.ps1

# 2. Build the application (Script handles Frontend build + Rust build + UPX)
powershell -ExecutionPolicy Bypass -File build.ps1
```

The default build is `x64`. To build specific Windows architectures:

```powershell
powershell -ExecutionPolicy Bypass -File build.ps1 -Arch x64
powershell -ExecutionPolicy Bypass -File build.ps1 -Arch arm64
powershell -ExecutionPolicy Bypass -File build.ps1 -Arch all
```

Artifacts are written under target-specific release folders and copied into `target\release\`.

### Validate Windows x64 + arm64 compilation

On Windows, use:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\validate-windows-targets.ps1 -Arch all
```

This validates:

* `x86_64-pc-windows-msvc`
* `aarch64-pc-windows-msvc`

The script auto-detects Visual Studio, uses `VsDevCmd.bat`, and writes logs into `target\validation-*.log`.

### Windows ARM64 notes

Windows ARM64 now compiles successfully, but runtime support still depends on the feature:

* **Supported direction:** general app launch, updater/installer selection, architecture-aware runtime downloads.
* **Requires runtime prerequisite:** WebView2-based overlays and tools.
* **Still unsupported:** local Qwen3 CUDA runtime on Windows ARM64 / Apple-silicon Windows VMs.
* **Still environment-dependent:** DirectML-heavy and recorder GPU/VM paths.

See [docs/WINDOWS_ARM64_SUPPORT.md](docs/WINDOWS_ARM64_SUPPORT.md) for the runtime support boundary.

### Option 3: Build from Source (Android)

**Prerequisites:**

* JDK 17 (e.g., Eclipse Temurin).
* Android SDK with build-tools and platform 36.

```bash
# Build signed release APK (extracts version from Cargo.toml)
powershell -ExecutionPolicy Bypass -File mobile\build-release.ps1
```

The APK will be located in `target/release/`. See `mobile/README.md` for detailed development workflow.

### Quick Development Build

To rebuild and run during development (builds PromptDJ and Translation Gummy frontends, then runs the app):

```powershell
cd promptdj-midi; npm install; npm run build; cd ..; New-Item -ItemType Directory -Path src\overlay\prompt_dj\dist -Force | Out-Null; Copy-Item promptdj-midi\dist\* -Destination src\overlay\prompt_dj\dist -Recurse -Force; cd translation-gummy-ui; npm install; npm run build; cd ..; New-Item -ItemType Directory -Path src\overlay\translation_gummy\dist -Force | Out-Null; Copy-Item translation-gummy-ui\dist\* -Destination src\overlay\translation_gummy\dist -Recurse -Force; cargo run
```

### Quick Screen Record Build

To rebuild and run during development (builds Screen Record frontend, then runs the app):

```powershell
cd screen-record; npm install; npm run build; cd ..; New-Item -ItemType Directory -Path src\overlay\screen_record\dist -Force | Out-Null; Copy-Item screen-record\dist\* -Destination src\overlay\screen_record\dist -Recurse -Force; cargo run
```

### On-Device Speech Recognition

SGT supports multiple on-device ASR engines. Models are downloaded automatically on first use.

#### Windows — Qwen3-ASR (NVIDIA GPU)

The Qwen3 ASR engine runs as a native CUDA DLL. Requires an **NVIDIA GPU**.

```powershell
# Build the native runtime DLL + bundle all libtorch dependencies
powershell -ExecutionPolicy Bypass -File scripts/build_qwen3_runtime.ps1

# Or build and copy directly to the app's private bin dir:
powershell -ExecutionPolicy Bypass -File scripts/build_qwen3_runtime.ps1 -CopyToPrivateBin
```

Available models (selected from overlay dropdown):
- **Qwen3-ASR 0.6B** — 52 languages, ~1.8 GB weights
- **Qwen3-ASR 1.7B** — 52 languages, ~3.5 GB weights, higher accuracy

#### Android — Moonshine Voice + Zipformer

The Android app uses two streaming ASR engines:

- **Moonshine Voice** (English) — Tiny/Small/Medium streaming variants via [Moonshine AI](https://github.com/moonshine-ai/moonshine) SDK. ~40-300 MB per model.
- **Zipformer** (multilingual) — Streaming transducer models via [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx). 8 language options:

| Language | Model | Source |
|----------|-------|--------|
| English | Kroko Zipformer | HuggingFace |
| Korean | Zipformer v1 | ModelScope |
| Chinese | Zipformer v1 | HuggingFace |
| French | Kroko Zipformer | HuggingFace |
| German | Kroko Zipformer | HuggingFace |
| Spanish | Kroko Zipformer | HuggingFace |
| Russian | Vosk Zipformer | HuggingFace |
| AR/EN/ID/JA/RU/TH/VI/ZH | 8-lang multilingual | HuggingFace |

Select models and languages from the transcription window header dropdown.

### Help Index (for "Ask anything about SGT")

The help assistant uses `help-index.json`, a pre-chunked index of the full codebase (Rust + TypeScript + Kotlin). At query time, keyword search retrieves the most relevant code chunks and sends them to Gemini.

To regenerate the index, run your own KaLM-compatible embedding server and point the build/query scripts at it.

#### 1. Start a KaLM embedding server

Set up a local or remote Python environment with your embedding server implementation, then start an HTTP endpoint that accepts:

- `POST /api/embed`
- JSON body: `{"input": "..."}`
- JSON response: `{"embeddings": [[...]]}`

Example:

```bash
python serve.py --host 0.0.0.0 --port 8400
```

#### 2. Point SGT at the server

Set `KALM_EMBED_SERVER_URL` before building or querying the index:

```bash
export KALM_EMBED_SERVER_URL=http://127.0.0.1:8400/api/embed
```

#### 3. Build the index

```bash
python scripts/help_index_build.py
```

#### 4. Query the index locally

```bash
python scripts/help_index_query.py "how does TTS work?"
```

## Getting Started

1. **Launch SGT:** Run the executable.
2. **Global Settings:**
    * Click the **Settings** icon in the sidebar.
    * Enter API Keys for the providers you wish to use (Gemini, Groq, OpenRouter, Cerebras).
    * *(Optional)* Enable **Ollama** if you have it installed locally.
3. **Select a Preset:**
    * Use the sidebar to choose a built-in preset (e.g., "Translate Region", "Transcribe Speech").
    * Assign a **Global Hotkey** (e.g., `Alt+Q`) to the preset.
4. **Usage:**
    * Press your hotkey.
    * **For Image Presets:** Drag to select a screen area.
    * **For Audio Presets:** Recording starts automatically (or opens the Realtime overlay).

## Advanced Configuration

### The Node Graph

SGT uses a node-based system for Presets.

1. **Create Preset:** Click `+` in the sidebar.
2. **Input Node:** Choose "Image", "Audio", or "Text".
3. **Process Node:** Select your AI Model and enter a System Prompt (e.g., "Translate this to Vietnamese").
4. **Connect:** Drag wires between nodes to define the data flow.
5. **Variables:** Use `{language1}` in your prompt to allow dynamic language selection via the UI.

### Real-time Translation (Cabin Mode)

1. Select/Create an **Audio** preset.
2. Set **Processing Mode** to **Realtime (Live)**.
3. Set **Source** to **Device** (System Audio) or **Mic**.
4. Launch the preset. A minimalist overlay will appear showing live subtitles.
5. *Tip:* You can toggle Transcription, Translation, and TTS directly from the overlay.

### Using Local AI (Ollama)

1. Install [Ollama](https://ollama.com/).
2. Pull models: `ollama pull llama3` (text) or `ollama pull moondream` (vision).
3. In SGT **Global Settings**, enable Ollama and set the URL (default: `http://localhost:11434`).
4. In your Preset's **Process Node**, select the model from the "Local" section.

## Troubleshooting

**"NO_API_KEY" Error**

* Go to Global Settings and ensure you have pasted a valid key for the model provider selected in your preset (Groq vs Google vs OpenRouter vs Cerebras).

**WebView2 / Blank UI**

* SGT uses Microsoft Edge WebView2 for complex rendering (Markdown, Charts, PromptDJ). Ensure the [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) is installed on your Windows machine.

**Audio Recording is Silent**

* **Device Audio:** Ensure audio is actually playing through your default output device.
* **Permissions:** Check Windows Privacy settings to ensure the app has access to the Microphone.

**PromptDJ / MIDI Not Working**

* Ensure your MIDI controller is connected *before* launching SGT.
* Click "Refresh Devices" inside the PromptDJ interface.

## License

MIT License

## Credits

Developed by **nganlinh4**.

* **Desktop UI:** [egui](https://github.com/emilk/egui) & [wry](https://github.com/tauri-apps/wry).
* **Audio:** [cpal](https://github.com/RustAudio/cpal) & [symphonia](https://github.com/pdeljanov/Symphonia).
* **GPU Rendering:** [wgpu](https://github.com/gfx-rs/wgpu) & Media Foundation.
* **Mobile:** Jetpack Compose & Kotlin Multiplatform.
* **On-Device ASR:** Android uses [Moonshine Voice](https://github.com/moonshine-ai/moonshine) and [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx); Windows uses [Qwen3-ASR](https://github.com/QwenLM/Qwen3-ASR) and Parakeet.
* **AI Providers:** Groq, Google DeepMind, OpenRouter, Cerebras.
