fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

pub fn build_settings_post_message_script(api_key: &str, lang: &str, theme: &str) -> String {
    let api_key = js_string(api_key);
    let lang = js_string(lang);
    let theme = js_string(theme);
    format!(
        r#"
                        if (window.postMessage) {{
                            window.postMessage({{ type: 'pm-dj-set-api-key', apiKey: {api_key}, lang: {lang} }}, '*');
                            window.postMessage({{ type: 'pm-dj-set-theme', theme: {theme} }}, '*');
                        }}
                        "#
    )
}

pub fn build_prompt_dj_init_script(api_key: &str, lang: &str, theme: &str) -> String {
    let api_key = js_string(api_key);
    let lang = js_string(lang);
    let theme = js_string(theme);
    format!(
        r#"
        // --- High-Priority Audio Hook ---
        (function() {{
            window._currentVolume = 1.0;
            window._activeMasterGains = [];

            const OriginalAC = window.AudioContext || window.webkitAudioContext;
            if (OriginalAC) {{
                const proto = OriginalAC.prototype;
                const desc = Object.getOwnPropertyDescriptor(proto, 'destination');
                if (desc && desc.get) {{
                    Object.defineProperty(proto, 'destination', {{
                        configurable: true,
                        enumerable: true,
                        get: function() {{
                            if (!this._masterGain) {{
                                const realDest = desc.get.call(this);
                                this._masterGain = this.createGain();
                                this._masterGain.gain.value = window._currentVolume;
                                this._masterGain.connect(realDest);
                                window._activeMasterGains.push(this._masterGain);
                            }}
                            return this._masterGain;
                        }}
                    }});
                }}
            }}
        }})();

        window.addEventListener('DOMContentLoaded', () => {{
            const style = document.createElement('style');
            style.innerHTML = `
                body {{
                    margin: 0;
                    padding: 0;
                    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif !important;
                    background-color: transparent !important;
                    overflow: hidden;
                }}
                #dj-drag-header {{
                    position: fixed;
                    top: 0;
                    left: 0;
                    width: 100%;
                    height: 32px;
                    background: transparent;
                    z-index: 2147483647;
                    -webkit-app-region: drag;
                    cursor: grab;
                    pointer-events: auto;
                }}
                #dj-drag-header:active {{
                    cursor: grabbing;
                }}
                #dj-close-btn {{
                    position: absolute;
                    top: 0;
                    right: 0;
                    width: 40px;
                    height: 32px;
                    background: transparent;
                    color: rgba(255,255,255,0.5);
                    border: none;
                    font-family: 'Google Sans Flex', 'Segoe UI', system-ui;
                    font-size: 16px;
                    cursor: pointer;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    transition: background 0.2s, color 0.2s;
                    -webkit-app-region: no-drag;
                }}
                #dj-close-btn:hover {{
                    background: rgba(255,0,0,0.5);
                    color: white;
                }}
                #dj-min-btn {{
                    position: absolute;
                    top: 0;
                    right: 40px;
                    width: 40px;
                    height: 32px;
                    background: transparent;
                    color: rgba(255,255,255,0.5);
                    border: none;
                    font-family: 'Google Sans Flex', 'Segoe UI', system-ui;
                    font-size: 16px;
                    cursor: pointer;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    transition: background 0.2s, color 0.2s;
                    -webkit-app-region: no-drag;
                }}
                #dj-min-btn:hover {{
                    background: rgba(255,255,255,0.1);
                    color: white;
                }}
                /* Light theme: keep white text with dark shadow for visibility */
                [data-theme='light'] #dj-close-btn,
                [data-theme='light'] #dj-min-btn {{
                    color: rgba(255,255,255,0.9);
                    text-shadow: 0 1px 3px rgba(0,0,0,0.5), 0 0 6px rgba(0,0,0,0.3);
                }}
            `;
            document.head.appendChild(style);

            const header = document.createElement('div');
            header.id = 'dj-drag-header';

            const minBtn = document.createElement('button');
            minBtn.id = 'dj-min-btn';
            minBtn.innerHTML = '—';
            minBtn.onclick = (e) => {{
                e.stopPropagation();
                if (window.ipc) window.ipc.postMessage('minimize_window');
            }};
            header.appendChild(minBtn);

            const closeBtn = document.createElement('button');
            closeBtn.id = 'dj-close-btn';
            closeBtn.innerHTML = '✕';
            closeBtn.onclick = (e) => {{
                e.stopPropagation();
                window.postMessage({{ type: 'pm-dj-stop-audio' }}, '*');
                if (window.ipc) window.ipc.postMessage('close_window');
            }};
            header.appendChild(closeBtn);

            // --- Volume Slider Removed (moved to PromptDjMidi.ts) ---

            const updateTheme = (theme) => {{
                if (theme === 'light') {{
                    document.documentElement.setAttribute('data-theme', 'light');
                }} else {{
                    document.documentElement.setAttribute('data-theme', 'dark');
                }}
            }};

            window.addEventListener('message', (e) => {{
                if (e.data && e.data.type === 'pm-dj-set-theme') {{
                    updateTheme(e.data.theme);
                }}
            }});

            // Hover Logic (Removed Vol Container part)

            document.body.appendChild(header);

            setTimeout(() => {{
                window.postMessage({{ type: 'pm-dj-set-api-key', apiKey: {api_key}, lang: {lang} }}, '*');
                window.postMessage({{ type: 'pm-dj-set-theme', theme: {theme} }}, '*');
                window.postMessage({{ type: 'pm-dj-set-font', font: 'google-sans-flex' }}, '*');
            }}, 250);
        }});

        "#
    )
}
