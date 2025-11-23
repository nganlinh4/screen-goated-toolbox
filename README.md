# Screen Grounded Translator (SGT)

A powerful Windows utility that captures any region of your screen and processes it using advanced AI Vision models. Whether you need to translate text, extract code (OCR), summarize content, or get image descriptions, SGT handles it with customizable presets and global hotkeys.

**"Grounded"** = Anchored on screen regions — results appear in an overlay exactly where you selected them.

## Key Features

- **Multi-Provider Support:** Utilize **Groq** (Llama 4 Vision) or **Google Gemini** (Flash Lite) for fast and accurate processing.
- **Preset System:** Create unlimited custom profiles (e.g., "Translate to Vietnamese", "OCR Code", "Summarize Image").
- **Advanced Hotkeys:** Assign custom key combinations (e.g., `Ctrl+Alt+T`, `Win+Shift+S`) to specific presets.
- **Retranslation Pipeline:** Chain a Vision model (to extract text) with a Text model (to translate/refine) for higher accuracy.
- **Dynamic Prompts:** Customize prompts with language tags (e.g., `{language1}`) to switch targets easily.
- **Smart Overlay:**
  - Streaming text support (Typewriter effect).
  - Auto-copy to clipboard.
  - "Broom" cursor for precise selection.
  - Linked windows for dual-view (original text + translation).
- **Localization:** UI available in English, Vietnamese, and Korean.

## Screenshot

![Screenshot](docs\images\screenshot.png)
![Demo Video](docs\images\demo-video.gif)

## Prerequisites

- **OS:** Windows 10 or Windows 11.
- **API Keys:**
  - **Groq:** [Get a free key here](https://console.groq.com/keys) (Recommended for Llama models).
  - **Google Gemini:** [Get a free key here](https://aistudio.google.com/app/apikey) (Optional, for Gemini models).

## Installation

### Option 1: Download Release
Download the latest `.exe` from the [Releases](https://github.com/nganlinh4/screen-grounded-translator/releases) page.

### Option 2: Build from Source
Ensure [Rust](https://www.rust-lang.org/tools/install) is installed.

```bash
git clone https://github.com/nganlinh4/screen-grounded-translator
cd screen-grounded-translator
# The build script will handle icon resource embedding automatically
cargo build --release
```

Run the executable found in `target/release/`.

## Getting Started

1. **Launch the App:** Open `screen-grounded-translator.exe`.
2. **Global Settings:**
   - Paste your **Groq API Key** and/or **Gemini API Key**.
   - Toggle **Run at Windows Startup** if desired.
3. **Configure a Preset:**
   - Select a preset on the left (e.g., "Translate").
   - **Prompt:** Define what the AI should do (e.g., "Extract text and translate to {language1}").
   - **Model:** Choose between `Scout` (Fast), `Maverick` (Accurate), or `Gemini`.
   - **Hotkeys:** Click "Add Key" and press your desired combination (e.g., `Alt+Q`).
4. **Capture:**
   - Press your hotkey. The screen will dim.
   - Drag to select the area you want to process.
   - The result will appear in an overlay window.

## Configuration Guide

### The Preset System
Unlike the old version, SGT now uses **Presets**. You can define specific behaviors for different hotkeys:

* **Translation:** Vision Model reads image -> Text Model translates it.
* **OCR (Text Extraction):** Vision Model extracts text -> Auto-copies to clipboard -> Hides overlay.
* **Summarization:** Vision Model analyzes visual content -> Returns a summary.

### Retranslation (Pipeline)
For complex translations, enabling **Retranslation** is recommended:
1. **Vision Model:** Reads the raw text from the image.
2. **Retranslate Model:** (Usually a fast text model like `gpt-oss-20b`) takes that raw text and translates it to your target language.
   *This often results in higher quality translations than Vision models alone.*

### Available Models
* **Vision Models (Image Understanding):**
  * `Scout` (Llama 4 Scout 17B 16E) - Extremely fast, good for general text.
  * `Maverick` (Llama 4 Maverick 17B 128E) - Highly accurate instruction following.
  * `Gemini Flash Lite` (Google) - Balanced performance.
* **Text Models (Retranslation):**
  * `Fast Text` (OpenAI/GPT-OSS via Groq).

## Troubleshooting

**Hotkey conflict / Not working:**
* If using the app in games or elevated applications (Task Manager, Registry Editor), you must **run SGT as Administrator**.
* Ensure no other background app is using the same key combination.

**"NO_API_KEY" Error:**
* Ensure you have entered keys in the "Global Settings" (Gear icon) tab.
* Ensure the specific preset is using a model that corresponds to the key you provided (e.g., don't select Gemini model if you only provided a Groq key).

**Window behaves strangely:**
* Double-click the system tray icon or use the tray menu "Settings" to force restore the UI.

## License

MIT — See [LICENSE](LICENSE) file.

## Credits

Developed by **nganlinh4**.
* Powered by [Groq](https://groq.com) and [Google DeepMind](https://deepmind.google/technologies/gemini/).
* Built with [Rust](https://www.rust-lang.org/) and [egui](https://github.com/emilk/egui).