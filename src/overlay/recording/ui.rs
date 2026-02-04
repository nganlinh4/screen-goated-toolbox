// --- RECORDING UI ---
// HTML generation for the recording overlay WebView.

use super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;

pub fn generate_html() -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let icon_pause = crate::overlay::html_components::icons::get_icon_svg("pause");
    let icon_play = crate::overlay::html_components::icons::get_icon_svg("play_arrow");
    let icon_close = crate::overlay::html_components::icons::get_icon_svg("close");
    let (text_rec, text_proc, text_wait, text_init, subtext, text_paused, is_dark) = {
        let app = APP.lock().unwrap();
        let lang = app.config.ui_language.as_str();
        let locale = crate::gui::locale::LocaleText::get(lang);
        let is_dark = match app.config.theme_mode {
            crate::config::ThemeMode::Dark => true,
            crate::config::ThemeMode::Light => false,
            crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };
        // Store initial theme state
        LAST_THEME_IS_DARK.store(is_dark, Ordering::SeqCst);
        (
            match lang {
                "vi" => "Đang ghi âm...",
                "ko" => "녹음 중...",
                _ => "Recording...",
            },
            match lang {
                "vi" => "Đang xử lý...",
                "ko" => "처리 중...",
                _ => "Processing...",
            },
            match lang {
                "vi" => "Chuẩn bị...",
                "ko" => "준비 중...",
                _ => "Starting...",
            },
            match lang {
                "vi" => "Đang kết nối...",
                "ko" => "연결 중...",
                _ => "Connecting...",
            },
            locale.recording_subtext,
            locale.recording_paused,
            is_dark,
        )
    };

    // Theme-specific colors
    let (
        container_bg,
        container_border,
        text_color,
        subtext_color,
        btn_bg,
        btn_hover_bg,
        btn_color,
        text_shadow,
    ) = if is_dark {
        (
            "rgba(18, 18, 18, 0.85)",
            "rgba(255, 255, 255, 0.1)",
            "white",
            "rgba(255, 255, 255, 0.7)",
            "rgba(255, 255, 255, 0.05)",
            "rgba(255, 255, 255, 0.15)",
            "rgba(255, 255, 255, 0.8)",
            "0 1px 2px rgba(0, 0, 0, 0.3)",
        )
    } else {
        (
            "rgba(255, 255, 255, 0.92)",
            "rgba(0, 0, 0, 0.1)",
            "#222222",
            "rgba(0, 0, 0, 0.6)",
            "rgba(0, 0, 0, 0.05)",
            "rgba(0, 0, 0, 0.1)",
            "rgba(0, 0, 0, 0.7)",
            "0 1px 2px rgba(255, 255, 255, 0.3)",
        )
    };

    format!(
        r#"
<!DOCTYPE html>
<html>
<head>
<style>
    {font_css}

    * {{ box-sizing: border-box; user-select: none; }}

    body {{
        margin: 0;
        padding: 0;
        width: 100vw;
        height: 100vh;
        overflow: hidden;
        background: transparent;
        display: flex;
        justify-content: center;
        align-items: center;
        opacity: 0;
        transition: opacity 0.15s ease-out;
    }}

    body.visible {{
        opacity: 1;
    }}

    .container {{
        width: {width}px;
        height: {height}px;
        background: {container_bg};
        backdrop-filter: blur(20px);
        -webkit-backdrop-filter: blur(20px);
        border: 1px solid {container_border};
        border-radius: 50px;
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        padding: 0 3px;
        gap: 6px;
        position: relative;
        color: {text_color};
        font-family: 'Google Sans Flex', sans-serif;
    }}

    .text-group {{
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        justify-content: center;
        flex-grow: 1;
        min-width: 0;
        margin-left: 5px;
    }}

    .status-text {{
        font-size: 15px;
        font-weight: 700;
        margin-bottom: 2px;
        text-shadow: {text_shadow};
        font-stretch: expanded;
        white-space: nowrap;
    }}

    .sub-text {{
        font-size: 10px;
        color: {subtext_color};
        margin-bottom: 0;
        white-space: nowrap;
        font-family: 'Google Sans Flex', sans-serif;
        font-variation-settings: 'opsz' 14;
    }}

    #volume-canvas {{
        height: 30px;
        width: 100px;
        margin-right: 5px;
    }}

    .btn {{
        position: relative;
        width: 34px;
        height: 34px;
        border-radius: 50%;
        background: {btn_bg};
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        pointer-events: auto;
        transition: background 0.2s, transform 0.1s;
        color: {btn_color};
        flex-shrink: 0;
        margin: 0 2px;
    }}

    .btn:hover {{
        background: {btn_hover_bg};
    }}
    .btn:active {{
        transform: scale(0.95);
    }}

    .btn svg {{
        width: 24px;
        height: 24px;
        fill: currentColor;
        display: block;
    }}

    .btn-close svg {{
        width: 36px;
        height: 36px;
    }}

    #icon-pause, #icon-play {{
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        height: 100%;
    }}

    .hidden {{ display: none !important; }}

</style>
</head>
<body>
    <div class="container">
        <div class="btn btn-pause" onclick="togglePause()" id="btn-pause">
             <div id="icon-pause">{icon_pause}</div>
             <div id="icon-play" class="hidden">{icon_play}</div>
        </div>

        <div class="text-group">
            <div class="status-text" id="status">{tx_rec}</div>
            <div class="sub-text">{tx_sub}</div>
        </div>

        <div style="display: flex; align-items: center;">
            <canvas id="volume-canvas" width="200" height="60" style="width: 100px; height: 30px;"></canvas>
        </div>

        <div class="btn btn-close" onclick="closeApp()">
            {icon_close}
        </div>
    </div>

    <script>
        const TEXT_REC = "{tx_rec}";
        const TEXT_PROC = "{tx_proc}";
        const TEXT_WAIT = "{tx_wait}";
        const TEXT_INIT = "{tx_init}";
        const TEXT_PAUSED = "{tx_paused}";

        const statusEl = document.getElementById('status');
        const pauseBtn = document.getElementById('btn-pause');
        const iconPause = document.getElementById('icon-pause');
        const iconPlay = document.getElementById('icon-play');

        let currentState = "warmup";

        const volumeCanvas = document.getElementById('volume-canvas');
        const volumeCtx = volumeCanvas ? volumeCanvas.getContext('2d') : null;

        const BAR_WIDTH = 8;
        const BAR_GAP = 6;
        const BAR_SPACING = BAR_WIDTH + BAR_GAP;
        const VISIBLE_BARS = 20;

        const barHeights = new Array(VISIBLE_BARS + 2).fill(6);
        let latestRMS = 0;
        let scrollProgress = 0;
        let lastTime = 0;
        let animationFrame = null;

        let isDark = {is_dark};

        const COLORS_DARK = {{
            recording:    ['#00a8e0', '#00c8ff', '#40e0ff'],
            processing:   ['#00FF00', '#32CD32', '#98FB98'],
            warmup:       ['#FFD700', '#FFA500', '#FFDEAD'],
            initializing: ['#9F7AEA', '#805AD5', '#B794F4'],
            paused:       ['#888888', '#AAAAAA', '#CCCCCC']
        }};

        const COLORS_LIGHT = {{
            recording:    ['#0066cc', '#0088dd', '#00aaee'],
            processing:   ['#00AA00', '#008800', '#006600'],
            warmup:       ['#cc6600', '#dd8800', '#ee9900'],
            initializing: ['#6B46C1', '#553C9A', '#805AD5'],
            paused:       ['#666666', '#888888', '#aaaaaa']
        }};

        let COLORS = isDark ? COLORS_DARK : COLORS_LIGHT;
        let currentColors = COLORS.warmup;

        function updateState(state, rms) {{
            currentState = state;
            latestRMS = rms;

            if (state === 'processing') {{
                 statusEl.innerText = TEXT_PROC;
                 currentColors = COLORS.processing;
                 pauseBtn.style.visibility = 'hidden';
                 pauseBtn.style.pointerEvents = 'none';
            }} else if (state === 'paused') {{
                 statusEl.innerText = TEXT_PAUSED;
                 currentColors = COLORS.paused;
                 for (let i = 0; i < barHeights.length; i++) barHeights[i] = 6;
                 pauseBtn.style.visibility = 'visible';
                 pauseBtn.style.pointerEvents = 'auto';
                 iconPause.classList.add('hidden');
                 iconPlay.classList.remove('hidden');
            }} else if (state === 'initializing') {{
                 statusEl.innerText = TEXT_INIT;
                 currentColors = COLORS.initializing;
                 for (let i = 0; i < barHeights.length; i++) barHeights[i] = 6;
                 pauseBtn.style.visibility = 'hidden';
                 pauseBtn.style.pointerEvents = 'none';
            }} else if (state === 'warmup') {{
                 statusEl.innerText = TEXT_WAIT;
                 currentColors = COLORS.warmup;
                 for (let i = 0; i < barHeights.length; i++) barHeights[i] = 6;
                 pauseBtn.style.visibility = 'hidden';
                 pauseBtn.style.pointerEvents = 'none';
            }} else {{
                 statusEl.innerText = TEXT_REC;
                 currentColors = COLORS.recording;
                 pauseBtn.style.visibility = 'visible';
                 pauseBtn.style.pointerEvents = 'auto';
                 iconPause.classList.remove('hidden');
                 iconPlay.classList.add('hidden');
            }}
        }}

        function drawWaveform(timestamp) {{
            if (!volumeCtx) return;

            const dt = lastTime ? (timestamp - lastTime) / 1000 : 0.016;
            lastTime = timestamp;

            const speed = currentState === 'processing' ? 0.06 : 0.15;
            scrollProgress += dt / speed;

            if (currentState === 'processing') {{
                const decayFactor = 0.95;
                const minHeight = 15;
                for (let i = 0; i < barHeights.length; i++) {{
                    if (barHeights[i] > minHeight) {{
                        barHeights[i] = Math.max(minHeight, barHeights[i] * decayFactor);
                    }}
                }}
            }}

            while (scrollProgress >= 1) {{
                scrollProgress -= 1;
                barHeights.shift();

                const h = volumeCanvas.height;
                let displayRMS = latestRMS;
                if (currentState === 'processing') {{
                    displayRMS = 0.12 + 0.2 * Math.abs(Math.sin(timestamp / 120));
                }} else if (currentState === 'initializing') {{
                    displayRMS = 0.08 + 0.12 * Math.abs(Math.sin(timestamp / 300));
                }} else if (currentState === 'paused') {{
                    displayRMS = 0.02;
                }} else if (currentState === 'warmup') {{
                    displayRMS = 0.02;
                }}

                let v = Math.max(6, Math.min(h - 4, displayRMS * 250 + 6));
                barHeights.push(v);
            }}

            const w = volumeCanvas.width;
            const h = volumeCanvas.height;
            volumeCtx.clearRect(0, 0, w, h);

            const pixelOffset = scrollProgress * BAR_SPACING;

            if (currentState === 'processing') {{
                const baseHue = (timestamp / 20) % 360;
                const wavePhase = timestamp / 200;

                for (let i = 0; i < barHeights.length; i++) {{
                    const waveValue = Math.sin((i * 0.4) + wavePhase);
                    const pillHeight = 12 + 35 * Math.abs(waveValue);

                    const x = i * BAR_SPACING - pixelOffset;
                    const y = (h - pillHeight) / 2;

                    if (x > -BAR_WIDTH && x < w) {{
                        const barHue = (baseHue + i * 18) % 360;
                        volumeCtx.fillStyle = `hsl(${{barHue}}, 100%, 55%)`;
                        volumeCtx.beginPath();
                        if (volumeCtx.roundRect) {{
                            volumeCtx.roundRect(x, y, BAR_WIDTH, pillHeight, BAR_WIDTH / 2);
                        }} else {{
                            volumeCtx.rect(x, y, BAR_WIDTH, pillHeight);
                        }}
                        volumeCtx.fill();
                    }}
                }}
            }} else {{
                const grad = volumeCtx.createLinearGradient(0, h, 0, 0);
                grad.addColorStop(0, currentColors[0]);
                grad.addColorStop(0.5, currentColors[1]);
                grad.addColorStop(1, currentColors[2]);
                volumeCtx.fillStyle = grad;

                for (let i = 0; i < barHeights.length; i++) {{
                    const pillHeight = barHeights[i];
                    const x = i * BAR_SPACING - pixelOffset;
                    const y = (h - pillHeight) / 2;

                    if (x > -BAR_WIDTH && x < w) {{
                        volumeCtx.beginPath();
                        if (volumeCtx.roundRect) {{
                            volumeCtx.roundRect(x, y, BAR_WIDTH, pillHeight, BAR_WIDTH / 2);
                        }} else {{
                            volumeCtx.rect(x, y, BAR_WIDTH, pillHeight);
                        }}
                        volumeCtx.fill();
                    }}
                }}
            }}

            const fadeWidth = 30;

            volumeCtx.save();
            volumeCtx.globalCompositeOperation = 'destination-out';

            const leftGrad = volumeCtx.createLinearGradient(0, 0, fadeWidth, 0);
            leftGrad.addColorStop(0, 'rgba(0, 0, 0, 1)');
            leftGrad.addColorStop(1, 'rgba(0, 0, 0, 0)');
            volumeCtx.fillStyle = leftGrad;
            volumeCtx.fillRect(0, 0, fadeWidth, h);

            const rightGrad = volumeCtx.createLinearGradient(w - fadeWidth, 0, w, 0);
            rightGrad.addColorStop(0, 'rgba(0, 0, 0, 0)');
            rightGrad.addColorStop(1, 'rgba(0, 0, 0, 1)');
            volumeCtx.fillStyle = rightGrad;
            volumeCtx.fillRect(w - fadeWidth, 0, fadeWidth, h);

            volumeCtx.restore();

            animationFrame = requestAnimationFrame(drawWaveform);
        }}

        if (!animationFrame) {{
            animationFrame = requestAnimationFrame(drawWaveform);
        }}

        function togglePause() {{
            window.ipc.postMessage('pause_toggle');
        }}

        function closeApp() {{
            window.ipc.postMessage('cancel');
        }}

        function resetState() {{
            hideState();
            setTimeout(() => {{
                 window.ipc.postMessage('ready');
            }}, 10);
        }}

        const container = document.querySelector('.container');
        container.addEventListener('mousedown', (e) => {{
            if (e.target.closest('.btn')) return;
            window.ipc.postMessage('drag_window');
        }});

        function hideState() {{
            document.body.classList.remove('visible');
        }}

        window.updateTheme = function(newIsDark, containerBg, containerBorder, textColor, subtextColor, btnBg, btnHoverBg, btnColor, textShadow) {{
            isDark = newIsDark;
            COLORS = isDark ? COLORS_DARK : COLORS_LIGHT;

            const container = document.querySelector('.container');
            container.style.background = containerBg;
            container.style.borderColor = containerBorder;
            container.style.color = textColor;

            const subtext = document.querySelector('.sub-text');
            if (subtext) subtext.style.color = subtextColor;

            const statusText = document.querySelector('.status-text');
            if (statusText) statusText.style.textShadow = textShadow;

            document.querySelectorAll('.btn').forEach(btn => {{
                btn.style.background = btnBg;
                btn.style.color = btnColor;
            }});

            if (currentState === 'recording') currentColors = COLORS.recording;
            else if (currentState === 'paused') currentColors = COLORS.paused;
            else if (currentState === 'warmup') currentColors = COLORS.warmup;
            else if (currentState === 'initializing') currentColors = COLORS.initializing;
            else if (currentState === 'processing') currentColors = COLORS.processing;
        }};
    </script>
</body>
</html>
    "#,
        width = get_ui_dimensions().0 - 20,
        height = get_ui_dimensions().1 - 20,
        font_css = font_css,
        tx_rec = text_rec,
        tx_proc = text_proc,
        tx_wait = text_wait,
        tx_init = text_init,
        tx_sub = subtext,
        tx_paused = text_paused,
        icon_pause = icon_pause,
        icon_play = icon_play,
        icon_close = icon_close,
        container_bg = container_bg,
        container_border = container_border,
        text_color = text_color,
        subtext_color = subtext_color,
        btn_bg = btn_bg,
        btn_hover_bg = btn_hover_bg,
        btn_color = btn_color,
        text_shadow = text_shadow,
        is_dark = if is_dark { "true" } else { "false" }
    )
}
