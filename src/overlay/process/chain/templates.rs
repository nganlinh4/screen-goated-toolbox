// --- CHAIN HTML TEMPLATES ---
// HTML generation for image and audio display in chain processing.

use crate::gui::locale::LocaleText;
use crate::overlay::html_components::icons::OverlayIcon;
use crate::overlay::utils::js_string_literal;

/// Generate HTML for displaying an image in the input adapter.
pub fn generate_image_display_html(img_data: &[u8]) -> String {
    use base64::Engine;
    let base64_img = base64::engine::general_purpose::STANDARD.encode(img_data);

    // Simple magic byte detection for MIME type
    let mime_type = if img_data.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else {
        "image/png" // Fallback
    };

    // Get locally cached font CSS for proper Unicode support
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<style>
{}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 100vh;
    background: transparent;
    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
}}
::-webkit-scrollbar {{ display: none; }}
.container {{
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    justify-content: center;
    align-items: center;
    background: rgba(20, 20, 25, 0.98);
    border-radius: 8px;
}}
.image {{
    width: 100%;
    height: auto;
    object-fit: contain;
    border-radius: 8px;
    transition: opacity 0.15s ease;
}}
</style>
</head>
<body>
<div class="container">
    <img class="image" id="img" src="data:{};base64,{}" />
</div>
</body>
</html>"#,
        font_css, mime_type, base64_img
    )
}

/// Generate HTML for the audio player in the input adapter.
pub fn generate_audio_player_html(wav_data: &[u8], locale: &LocaleText) -> String {
    const MAX_INLINE_AUDIO_BYTES: usize = 8 * 1024 * 1024;
    if wav_data.len() > MAX_INLINE_AUDIO_BYTES {
        return format!(
            "<div style=\"padding:20px;color:white\">{}</div>",
            crate::overlay::utils::escape_html(locale.overlay.audio_preview_unavailable)
        );
    }
    use base64::Engine;
    let base64_audio = base64::engine::general_purpose::STANDARD.encode(wav_data);
    // Get locally cached font CSS for proper Unicode support
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let play_icon = OverlayIcon::PlayArrow.svg();
    let download_icon = OverlayIcon::Download.svg();
    let play_icon_js = js_string_literal(&play_icon);
    let pause_icon_js = js_string_literal(&OverlayIcon::Pause.svg());
    let done_icon_js = js_string_literal(&OverlayIcon::Check.svg());
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<style>
{}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 100vh;
    background: transparent;
    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
}}
::-webkit-scrollbar {{ display: none; }}
.audio-player {{
    background: #1e1e1e;
    border-radius: 12px;
    padding: 20px clamp(8px, 6vw, 24px);
    width: 100%;
    max-width: 400px;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.08);
    position: relative;
}}
.waveform {{
    display: flex;
    align-items: center;
    gap: 2px;
    height: 60px;
    margin-bottom: 16px;
    justify-content: center;
}}
.wave-bar {{
    width: 3px;
    min-height: 4px;
    background: #8ab4f8;
    border-radius: 2px;
    transition: height 0.05s ease-out;
}}
.controls {{
    display: flex;
    align-items: center;
    gap: clamp(6px, 3.5vw, 14px);
}}
.play-btn {{
    width: 44px;
    height: 44px;
    background: #8ab4f8;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform 0.2s, background-color 0.2s;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
    flex-shrink: 0;
}}
.play-btn:hover {{
    transform: scale(1.05);
    background: #aecbfa;
}}
.play-btn svg {{
    color: #1e1e1e;
    width: 18px;
    height: 18px;
    margin-left: 2px;
}}
.play-btn.playing svg {{
    margin-left: 0;
}}
.download-btn {{
    position: relative;
    width: 36px;
    height: 36px;
    background: transparent;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
    flex-shrink: 0;
    margin-left: 4px;
    font-family: inherit;
}}
.download-btn:hover {{
    background: rgba(255, 255, 255, 0.1);
}}
.download-btn::after {{
    content: attr(aria-label);
    position: absolute;
    right: 0;
    bottom: calc(100% + 5px);
    z-index: 1;
    box-sizing: border-box;
    width: max-content;
    max-width: min(16rem, calc(100vw - 36px));
    overflow-wrap: anywhere;
    padding: 5px 8px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: #29292e;
    color: #fff;
    font-size: 12px;
    line-height: 1.3;
    pointer-events: none;
    visibility: hidden;
}}
.download-btn:not(.success):is(:hover, :focus-visible)::after {{
    visibility: visible;
    transition: visibility 0s 0.4s;
}}
.download-btn svg {{
    color: #9aa0a6;
    width: 20px;
    height: 20px;
    transition: color 0.2s;
}}
.download-btn:hover svg {{
    color: #fff;
}}
.download-btn.success svg {{
    color: #4CAF50;
}}
.download-btn.success:hover {{
    background: rgba(76, 175, 80, 0.15);
}}
.progress-container {{
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
}}
.progress-bar {{
    height: 4px;
    background: rgba(255,255,255,0.1);
    border-radius: 2px;
    overflow: hidden;
    cursor: pointer;
}}
.progress-fill {{
    height: 100%;
    background: #8ab4f8;
    border-radius: 2px;
    width: 0%;
    transition: width 0.1s;
}}
.time-display {{
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: #9aa0a6;
}}
.toast {{
    position: absolute;
    bottom: 74px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(30, 30, 35, 0.95);
    color: #fff;
    padding: 8px 16px;
    border-radius: 20px;
    font-size: 13px;
    font-weight: 500;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.3s ease;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    border: 1px solid rgba(255,255,255,0.1);
    white-space: nowrap;
    z-index: 100;
    backdrop-filter: blur(4px);
}}
.toast.show {{
    opacity: 1;
}}
audio {{ display: none; }}
</style>
</head>
<body>
<div class="audio-player">
    <div class="toast" id="toast">{}</div>
    <div class="waveform" id="waveform"></div>
    <div class="controls">
        <button class="play-btn" id="playBtn">
            {play_icon}
        </button>
        <div class="progress-container">
            <div class="progress-bar" id="progressBar">
                <div class="progress-fill" id="progress"></div>
            </div>
            <div class="time-display">
                <span id="current">0:00</span>
                <span id="duration">0:00</span>
            </div>
        </div>
        <button class="download-btn" id="downloadBtn" aria-label="{}">
            {download_icon}
        </button>
    </div>
</div>
<audio id="audio">
    <source src="data:audio/wav;base64,{}" type="audio/wav">
</audio>
<script>
const audio = document.getElementById('audio');
const progress = document.getElementById('progress');
const PLAY_ICON = {play_icon_js};
const PAUSE_ICON = {pause_icon_js};
const DONE_ICON = {done_icon_js};
const playBtn = document.getElementById('playBtn');
const downloadBtn = document.getElementById('downloadBtn');
const toast = document.getElementById('toast');
const currentTimeEl = document.getElementById('current');
const durationEl = document.getElementById('duration');
const waveformEl = document.getElementById('waveform');
const progressBar = document.getElementById('progressBar');

// Create waveform bars
const BAR_COUNT = 32;
for (let i = 0; i < BAR_COUNT; i++) {{
    const bar = document.createElement('div');
    bar.className = 'wave-bar';
    bar.style.height = '4px';
    waveformEl.appendChild(bar);
}}
const bars = waveformEl.querySelectorAll('.wave-bar');

// Web Audio API setup
let audioContext, analyser, source, dataArray;
let isSetup = false;

function setupAudio() {{
    if (isSetup) return;
    audioContext = new (window.AudioContext || window.webkitAudioContext)();
    analyser = audioContext.createAnalyser();
    analyser.fftSize = 64;
    source = audioContext.createMediaElementSource(audio);
    source.connect(analyser);
    analyser.connect(audioContext.destination);
    dataArray = new Uint8Array(analyser.frequencyBinCount);
    isSetup = true;
}}

function formatTime(s) {{
    if (isNaN(s)) return '0:00';
    const m = Math.floor(s / 60);
    const sec = Math.floor(s % 60);
    return m + ':' + (sec < 10 ? '0' : '') + sec;
}}

function visualize() {{
    if (!analyser || audio.paused) return;
    analyser.getByteFrequencyData(dataArray);
    for (let i = 0; i < BAR_COUNT; i++) {{
        const idx = Math.floor(i * dataArray.length / BAR_COUNT);
        const value = dataArray[idx];
        const height = Math.max(4, (value / 255) * 56);
        bars[i].style.height = height + 'px';
    }}
    requestAnimationFrame(visualize);
}}

audio.onloadedmetadata = () => {{
    durationEl.textContent = formatTime(audio.duration);
}};

audio.ontimeupdate = () => {{
    const pct = (audio.currentTime / audio.duration) * 100;
    progress.style.width = pct + '%';
    currentTimeEl.textContent = formatTime(audio.currentTime);
}};

audio.onended = () => {{
    playBtn.innerHTML = PLAY_ICON;
    playBtn.classList.remove('playing');
    bars.forEach(b => b.style.height = '4px');
}};

playBtn.onclick = () => {{
    setupAudio();
    if (audio.paused) {{
        audio.play();
        playBtn.innerHTML = PAUSE_ICON;
        playBtn.classList.add('playing');
        visualize();
    }} else {{
        audio.pause();
        playBtn.innerHTML = PLAY_ICON;
        playBtn.classList.remove('playing');
    }}
}};

downloadBtn.onclick = () => {{
    const link = document.createElement('a');
    link.href = audio.querySelector('source').src;
    const date = new Date();
    const ts = date.getFullYear() + '-' + (date.getMonth()+1) + '-' + date.getDate() + '_' + date.getHours() + '-' + date.getMinutes() + '-' + date.getSeconds();
    link.download = 'recording_' + ts + '.wav';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);

    // Visual Feedback
    const originalIcon = downloadBtn.innerHTML;
    downloadBtn.innerHTML = DONE_ICON;
    downloadBtn.classList.add('success');
    toast.classList.add('show');

    setTimeout(() => {{
        downloadBtn.innerHTML = originalIcon;
        downloadBtn.classList.remove('success');
        toast.classList.remove('show');
    }}, 2500);
}};

progressBar.onclick = (e) => {{
    const rect = progressBar.getBoundingClientRect();
    const pct = (e.clientX - rect.left) / rect.width;
    audio.currentTime = pct * audio.duration;
}};
</script>
</body>
</html>"#,
        font_css,
        locale.shell.downloaded_successfully,
        crate::overlay::utils::escape_html(locale.shell.download_recording_tooltip),
        base64_audio
    )
}

#[cfg(test)]
mod audio_preview_tests {
    use super::*;

    #[test]
    fn large_audio_is_not_embedded_in_preview() {
        let audio = vec![0; 8 * 1024 * 1024 + 1];
        for language in ["en", "ko", "vi"] {
            let locale = LocaleText::get(language);
            let html = generate_audio_player_html(&audio, &locale);
            assert!(html.contains(locale.overlay.audio_preview_unavailable));
            assert!(!html.contains("data:audio"));
        }
    }

    #[test]
    fn player_icons_are_catalog_symbols_swapped_whole() {
        let html = generate_audio_player_html(&[0; 64], &LocaleText::get("en"));
        crate::overlay::html_components::icons::assert_only_catalog_icons(&html);
        for (constant, icon) in [
            ("PLAY_ICON", OverlayIcon::PlayArrow),
            ("PAUSE_ICON", OverlayIcon::Pause),
            ("DONE_ICON", OverlayIcon::Check),
        ] {
            let declaration = format!("const {constant} = {};", js_string_literal(&icon.svg()));
            assert!(html.contains(&declaration), "{constant}");
        }
    }

    #[test]
    fn download_tooltip_uses_the_localized_button_label_without_a_native_title() {
        for language in ["en", "ko", "vi"] {
            let locale = LocaleText::get(language);
            let html = generate_audio_player_html(&[0; 64], &locale);
            let button = format!(
                r#"<button class="download-btn" id="downloadBtn" aria-label="{}">"#,
                crate::overlay::utils::escape_html(locale.shell.download_recording_tooltip),
            );
            assert!(html.contains(&button), "{language}");
            assert!(html.contains("content: attr(aria-label);"));
        }
    }
}
