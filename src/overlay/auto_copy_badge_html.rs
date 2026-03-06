pub(super) fn get_badge_html() -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
    {font_css}
    :root {{
        --this-bg: #1A3D2A;
        --this-border: #4ADE80;
        --this-text-prio: #ffffff;
        --this-text-sec: rgba(255, 255, 255, 0.9);
        --this-accent: #4ADE80;
        --this-bloom: rgba(74, 222, 128, 0.6);
        --this-shadow: rgba(0, 0, 0, 0.5);
    }}

    * {{ margin: 0; padding: 0; box-sizing: border-box; }}

    body {{
        overflow: hidden;
        background: transparent;
        font-family: 'Google Sans Flex', 'Segoe UI', sans-serif;
        display: flex;
        flex-direction: column;
        justify-content: flex-end;
        align-items: center;
        height: 100vh;
        user-select: none;
        cursor: default;
        padding-bottom: 20px;
    }}

    #notifications {{
        display: flex;
        flex-direction: column;
        width: 100%;
        align-items: center;
        gap: 10px;
    }}

    .badge {{
        min-width: 180px;
        max-width: 90%;
        width: auto;
        background: var(--this-bg);
        border: 2.5px solid var(--this-border);
        border-radius: 12px;
        box-shadow: 0 0 12px var(--this-bloom),
                    0 4px 15px var(--this-shadow);
        backdrop-filter: blur(12px);
        -webkit-backdrop-filter: blur(12px);
        display: flex;
        flex-direction: column;
        justify-content: center;
        align-items: center;
        padding: 4px 18px;
        position: relative;
        opacity: 0;
        transform: translateY(20px) scale(0.92);
        transition: all 0.4s cubic-bezier(0.2, 0.8, 0.2, 1);
    }}

    .badge.visible {{
        opacity: 1;
        transform: translateY(0) scale(1);
    }}

    .row {{
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        line-height: normal;
        position: relative;
    }}

    .title-row {{ margin-bottom: 0px; }}

    .title {{
        font-size: 15px;
        font-weight: 700;
        color: var(--this-text-prio);
        display: flex;
        align-items: center;
        gap: 8px;
        letter-spacing: 1.2px;
        text-transform: uppercase;
        font-variation-settings: 'wght' 700, 'wdth' 115, 'ROND' 100;
    }}

    .check {{
        color: var(--this-accent);
        font-weight: 800;
        font-size: 18px;
        display: flex;
        align-items: center;
        justify-content: center;
        animation: pop 0.4s cubic-bezier(0.175, 0.885, 0.32, 1.275) forwards;
        animation-delay: 0.1s;
        opacity: 0;
        transform: scale(0);
        filter: drop-shadow(0 0 5px var(--this-accent));
    }}

    @keyframes pop {{
        from {{ opacity: 0; transform: scale(0); }}
        to {{ opacity: 1; transform: scale(1); }}
    }}

    .snippet {{
        font-size: 13px;
        font-weight: 500;
        color: var(--this-text-sec);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 100%;
        text-align: center;
        padding-top: 1px;
        font-family: 'Google Sans Flex', 'Segoe UI', sans-serif;
        font-variation-settings: 'wght' 500, 'wdth' 85, 'ROND' 50;
        letter-spacing: -0.3px;
    }}

    .snippet-container {{
        width: 100%;
        display: flex;
        justify-content: center;
        overflow: hidden;
    }}

    .progress-badge {{
        min-width: 360px;
        max-width: 70%;
        padding: 10px 18px 12px;
        gap: 6px;
        align-items: stretch;
    }}

    .progress-title-row,
    .progress-snippet-row {{
        justify-content: flex-start;
    }}

    .progress-title {{
        width: 100%;
        justify-content: space-between;
        gap: 14px;
    }}

    .progress-value {{
        font-size: 13px;
        font-weight: 700;
        color: var(--this-accent);
        font-variation-settings: 'wght' 700, 'wdth' 100, 'ROND' 80;
        white-space: nowrap;
    }}

    .progress-track {{
        width: 100%;
        height: 8px;
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.12);
        overflow: hidden;
        box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.07);
    }}

    .progress-fill {{
        width: 0%;
        height: 100%;
        border-radius: inherit;
        background: linear-gradient(90deg, var(--this-border), var(--this-accent));
        box-shadow: 0 0 10px var(--this-bloom);
        transition: width 0.15s linear;
    }}
</style>
</head>
<body>
    <div id="notifications"></div>
    <script>
        window.onerror = function(msg, source, line, col, error) {{
            window.ipc.postMessage('error: ' + msg + ' @ ' + line);
        }};

        const themes = {{
            success: {{
                dark: {{
                    bg: 'rgba(10, 24, 18, 0.95)',
                    border: '#4ADE80',
                    textPrio: '#ffffff',
                    textSec: 'rgba(255, 255, 255, 0.9)',
                    accent: '#4ADE80',
                    bloom: 'rgba(74, 222, 128, 0.5)',
                    shadow: 'rgba(0, 0, 0, 0.6)'
                }},
                light: {{
                    bg: 'rgba(255, 255, 255, 0.95)',
                    border: '#16a34a',
                    textPrio: '#1a1a1a',
                    textSec: '#333333',
                    accent: '#16a34a',
                    bloom: 'rgba(22, 163, 74, 0.3)',
                    shadow: 'rgba(0, 0, 0, 0.2)'
                }},
                duration: 1000
            }},
            file_copy: {{
                dark: {{
                    bg: 'rgba(8, 22, 28, 0.95)',
                    border: '#22D3EE',
                    textPrio: '#ffffff',
                    textSec: 'rgba(255, 255, 255, 0.92)',
                    accent: '#22D3EE',
                    bloom: 'rgba(34, 211, 238, 0.45)',
                    shadow: 'rgba(0, 0, 0, 0.6)'
                }},
                light: {{
                    bg: 'rgba(236, 254, 255, 0.95)',
                    border: '#0891B2',
                    textPrio: '#1a1a1a',
                    textSec: '#334155',
                    accent: '#0891B2',
                    bloom: 'rgba(8, 145, 178, 0.28)',
                    shadow: 'rgba(0, 0, 0, 0.2)'
                }},
                duration: 2400
            }},
            gif_copy: {{
                dark: {{
                    bg: 'rgba(34, 12, 28, 0.95)',
                    border: '#F472B6',
                    textPrio: '#ffffff',
                    textSec: 'rgba(255, 255, 255, 0.92)',
                    accent: '#F472B6',
                    bloom: 'rgba(244, 114, 182, 0.45)',
                    shadow: 'rgba(0, 0, 0, 0.6)'
                }},
                light: {{
                    bg: 'rgba(253, 242, 248, 0.95)',
                    border: '#DB2777',
                    textPrio: '#1a1a1a',
                    textSec: '#4c1d95',
                    accent: '#DB2777',
                    bloom: 'rgba(219, 39, 119, 0.28)',
                    shadow: 'rgba(0, 0, 0, 0.2)'
                }},
                duration: 2400
            }},
            info: {{
                dark: {{
                    bg: 'rgba(30, 25, 10, 0.95)',
                    border: '#FACC15',
                    textPrio: '#ffffff',
                    textSec: 'rgba(255, 255, 255, 0.9)',
                    accent: '#FACC15',
                    bloom: 'rgba(250, 204, 21, 0.5)',
                    shadow: 'rgba(0, 0, 0, 0.6)'
                }},
                light: {{
                    bg: 'rgba(255, 251, 235, 0.95)',
                    border: '#CA8A04',
                    textPrio: '#1a1a1a',
                    textSec: '#333333',
                    accent: '#CA8A04',
                    bloom: 'rgba(202, 138, 4, 0.3)',
                    shadow: 'rgba(0, 0, 0, 0.2)'
                }},
                duration: 1500
            }},
            update: {{
                dark: {{
                    bg: 'rgba(10, 18, 30, 0.95)',
                    border: '#60A5FA',
                    textPrio: '#ffffff',
                    textSec: 'rgba(255, 255, 255, 0.9)',
                    accent: '#60A5FA',
                    bloom: 'rgba(96, 165, 250, 0.5)',
                    shadow: 'rgba(0, 0, 0, 0.6)'
                }},
                light: {{
                    bg: 'rgba(239, 246, 255, 0.95)',
                    border: '#2563EB',
                    textPrio: '#1a1a1a',
                    textSec: '#333333',
                    accent: '#2563EB',
                    bloom: 'rgba(37, 99, 235, 0.3)',
                    shadow: 'rgba(0, 0, 0, 0.2)'
                }},
                duration: 5000
            }},
            error: {{
                dark: {{
                    bg: 'rgba(30, 10, 10, 0.95)',
                    border: '#F87171',
                    textPrio: '#ffffff',
                    textSec: 'rgba(255, 255, 255, 0.9)',
                    accent: '#F87171',
                    bloom: 'rgba(248, 113, 113, 0.5)',
                    shadow: 'rgba(0, 0, 0, 0.6)'
                }},
                light: {{
                    bg: 'rgba(254, 242, 242, 0.95)',
                    border: '#DC2626',
                    textPrio: '#1a1a1a',
                    textSec: '#333333',
                    accent: '#DC2626',
                    bloom: 'rgba(220, 38, 38, 0.3)',
                    shadow: 'rgba(0, 0, 0, 0.2)'
                }},
                duration: 2500
            }}
        }};

        let isDarkMode = false;

        window.setTheme = (isDark) => {{
            isDarkMode = isDark;
        }};

        function getColors(type, isDark) {{
            const t = themes[type] || themes.success;
            return isDark ? t.dark : t.light;
        }}

        function applyTheme(target, colors) {{
            const s = target.style;
            s.setProperty('--this-bg', colors.bg);
            s.setProperty('--this-border', colors.border);
            s.setProperty('--this-text-prio', colors.textPrio);
            s.setProperty('--this-text-sec', colors.textSec);
            s.setProperty('--this-accent', colors.accent);
            s.setProperty('--this-bloom', colors.bloom);
            s.setProperty('--this-shadow', colors.shadow);
        }}

        function getLeadingIcon(type) {{
            if (type === 'gif_copy') {{
                return `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="3" y="5" width="18" height="14" rx="3"></rect>
                    <text x="12" y="15" text-anchor="middle" font-size="7.5" font-weight="700" fill="currentColor" stroke="none">GIF</text>
                </svg>`;
            }}
            if (type === 'file_copy') {{
                return `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.7" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="3" y="6" width="13" height="12" rx="2"></rect>
                    <polygon points="16 10 21 7.5 21 16.5 16 14"></polygon>
                </svg>`;
            }}
            return `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="20 6 9 17 4 12"></polyline>
            </svg>`;
        }}

        function maybeHideWindow() {{
            const container = document.getElementById('notifications');
            if (container.children.length === 0) {{
                window.ipc.postMessage('finished');
            }}
        }}

        window.addNotification = (title, snippet, type, durationOverride) => {{
            const container = document.getElementById('notifications');
            const colors = getColors(type, isDarkMode);
            const themeDuration = (themes[type] || themes.success).duration;
            const duration = Number.isFinite(durationOverride) && durationOverride > 0
                ? durationOverride
                : themeDuration;

            const badge = document.createElement('div');
            badge.className = 'badge';
            applyTheme(badge, colors);

            const hasSnippet = (snippet && snippet.length > 0);
            const checkDisplay = hasSnippet ? 'flex' : 'none';
            const snippetDisplay = hasSnippet ? 'flex' : 'none';
            const leadingIcon = getLeadingIcon(type);

            badge.innerHTML = `
                <div class="row title-row">
                    <div class="title">
                        <span class="check" style="display: ${{checkDisplay}}">
                            ${{leadingIcon}}
                        </span>
                        <span>${{title}}</span>
                    </div>
                </div>
                <div class="row snippet-container" style="display: ${{snippetDisplay}}">
                    <div class="snippet">${{snippet}}</div>
                </div>
            `;

            container.appendChild(badge);

            requestAnimationFrame(() => {{
                requestAnimationFrame(() => {{
                    badge.classList.add('visible');
                }});
            }});

            setTimeout(() => {{
                badge.classList.remove('visible');
                setTimeout(() => {{
                    if (badge.parentNode) badge.parentNode.removeChild(badge);
                    maybeHideWindow();
                }}, 400);
            }}, duration);
        }};

        window.upsertProgressNotification = (title, snippet, progress) => {{
            const container = document.getElementById('notifications');
            const colors = getColors('info', isDarkMode);
            let badge = document.getElementById('progress-badge');
            if (!badge) {{
                badge = document.createElement('div');
                badge.id = 'progress-badge';
                badge.className = 'badge progress-badge';
                badge.innerHTML = `
                    <div class="row title-row progress-title-row">
                        <div class="title progress-title">
                            <span class="progress-title-text"></span>
                            <span class="progress-value"></span>
                        </div>
                    </div>
                    <div class="row snippet-container progress-snippet-row">
                        <div class="snippet progress-snippet"></div>
                    </div>
                    <div class="progress-track">
                        <div class="progress-fill"></div>
                    </div>
                `;
                container.appendChild(badge);
                requestAnimationFrame(() => {{
                    requestAnimationFrame(() => {{
                        badge.classList.add('visible');
                    }});
                }});
            }}

            applyTheme(badge, colors);
            const clamped = Math.max(0, Math.min(100, Number(progress) || 0));
            badge.querySelector('.progress-title-text').textContent = title;
            badge.querySelector('.progress-value').textContent = `${{Math.round(clamped)}}%`;
            badge.querySelector('.progress-snippet').textContent = snippet || '';
            badge.querySelector('.progress-fill').style.width = `${{clamped}}%`;
        }};

        window.removeProgressNotification = () => {{
            const badge = document.getElementById('progress-badge');
            if (!badge) {{
                maybeHideWindow();
                return;
            }}

            badge.classList.remove('visible');
            setTimeout(() => {{
                if (badge.parentNode) badge.parentNode.removeChild(badge);
                maybeHideWindow();
            }}, 250);
        }};
    </script>
</body>
</html>"#
    )
}
