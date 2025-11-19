# Screen Grounded Translator (SGT)

A Windows utility that translates text from any region of your screen using Groq's Vision API. Capture a selected area, perform OCR and translation using Llama-based vision models, and view the result in an overlay.

**"Grounded"** = anchored/positioned on screen regions — translation appears exactly where you selected.

## Demo

<div align="center">
  <img src="demo-video.gif" width="100%" alt="Screen Grounded Translator Demo" />
</div>

## Screenshots

<div align="center">
  <img src="settings-UI-light.png" height="500" alt="Settings Light Mode" />
  <img src="settings-UI-dark.png" height="500" alt="Settings Dark Mode" />
</div>

<div align="center">
  <p><em>System Tray Integration</em></p>
  <img src="appearance-in-system-tray.png" alt="System Tray" />
</div>

## Features

- **Region Selection:** Drag-and-select any part of the screen to translate
- **Vision-based Translation:** Uses Groq (Llama 3.2 Vision) for simultaneous text extraction and translation
- **Overlay Results:** Displays translation in a floating window
- **System Tray:** Minimizes to tray for background operation
- **Multi-language Support:** Translate to Vietnamese, Korean, English, and 100+ other languages
- **Customizable Settings:**
  - Change target language
  - Remap hotkey (F2-F10, or Tilde)
  - Dark/Light mode
  - Run at Windows startup
- **No Extra Bloat:** Lightweight, single-window design with tray integration

## Prerequisites

- **OS:** Windows 10/11
- **Groq API Key:** Get one free at [console.groq.com/keys](https://console.groq.com/keys)

## Installation

### Option 1: Download Release
Download the latest `.exe` from the [Releases](https://github.com/nganlinh4/screen-grounded-translator/releases) page.

### Option 2: Build from Source
Ensure Rust and Cargo are installed.

```bash
git clone https://github.com/nganlinh4/screen-grounded-translator
cd screen-grounded-translator
cargo build --release
```

Run the executable from `target/release/screen-grounded-translator.exe`.

## First Time Setup

1. Launch `screen-grounded-translator.exe`
2. Enter your Groq API key (from [console.groq.com/keys](https://console.groq.com/keys))
3. Select target language
4. (Optional) Change hotkey, enable startup, theme
5. Close window — app stays in tray

## Usage

1. Press the hotkey (default: `~`)
2. The screen will dim; click and drag to select the text region
3. Release to capture and translate
4. View the translation in the overlay that appears
5. Click the overlay to dismiss it

## Hotkeys

| Key | Action |
|-----|--------|
| `~` (Tilde), F2, F4, F6-F10 | Trigger screen capture |
| Click overlay | Dismiss translation |
| ESC | Cancel region selection |

## Configuration

All settings auto-save:

- **API Key:** Required (get free at [Groq Console](https://console.groq.com/keys))
- **Target Language:** Language to translate into
- **Hotkey:** Activation key (see Hotkeys table above)
- **Dark Mode:** UI theme preference
- **Run at Startup:** Auto-launch with Windows
- **Rate Limits:** Check [Groq rate limits](https://console.groq.com/docs/rate-limits) for your usage

## Important Notes

### Fullscreen Apps & Games

If you need to use the hotkey in fullscreen apps or games, **run this application as Administrator**. Windows blocks hotkeys in exclusive fullscreen mode unless the app has elevated privileges.

To run as admin:
1. Right-click the `.exe`
2. Select "Run as administrator"
3. Restart the app

### Tray Icon

- **Left-click tray icon:** Show/hide settings window
- **Right-click tray icon:** Quit the application
- Closing the settings window minimizes to tray (doesn't quit)

## Troubleshooting

**Hotkey doesn't work:**
- Restart app after changing hotkey
- Check another app isn't using it
- In fullscreen? Run as admin (see above)

**Translation slow/fails:**
- Verify API key is valid
- Check internet & Groq API status
- Review [rate limits](https://console.groq.com/docs/rate-limits)

**OCR not recognizing text:**
- Ensure text region is clear and readable
- Try larger selection for small text
- Llama Vision supports text in most languages

**Windows SmartScreen warning:**
- False positive on first run — click "Run anyway"
- App is safe and open-source

## FAQ

**Q: Does this send my screenshots to Groq?**
A: Yes, screenshots are sent to Groq API for translation. Read their [privacy policy](https://groq.com/privacy).

**Q: Can I use this offline?**
A: No, requires internet connection and valid Groq API key.

**Q: What languages can be translated?**
A: Any language readable by Llama Vision (most major languages supported).

## License

MIT — See [LICENSE](LICENSE) file

## Credits

Made by [nganlinh4](https://github.com/nganlinh4)

Uses [Groq API](https://groq.com) and [Llama Vision](https://www.llama.com)
