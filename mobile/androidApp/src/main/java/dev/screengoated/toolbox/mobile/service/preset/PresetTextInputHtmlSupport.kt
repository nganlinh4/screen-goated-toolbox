package dev.screengoated.toolbox.mobile.service.preset

internal fun presetTextInputBaseHtmlTemplate(): String {
    return """
        <!DOCTYPE html>
        <html {{THEME_ATTR}}>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>{{FONT_CSS}}</style>
            <style id="theme-style">{{EDITOR_CSS}}</style>
        </head>
        <body>
            <div class="editor-container">
                <div class="header" id="headerRegion">
                    <span class="header-title" id="headerTitle">{{TITLE_TEXT}}</span>
                    <div class="close-btn" id="closeBtn" title="Close">
                        {{CLOSE_SVG}}
                    </div>
                </div>

                <textarea id="editor" placeholder="{{PLACEHOLDER_TEXT}}" autofocus></textarea>

                <div class="btn-container">
                    <button class="mic-btn" id="micBtn" title="Speech to text">
                        {{MIC_SVG}}
                    </button>
                    <button class="send-btn" id="sendBtn" title="Send">
                        {{SEND_SVG}}
                    </button>
                </div>
            </div>
            <script>
                window.ipc = {
                    postMessage(message) {
                        if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                            window.sgtAndroid.postMessage(String(message));
                        }
                    }
                };
                {{EDITOR_JS}}
            </script>
        </body>
        </html>
    """.trimIndent()
}

internal fun presetTextInputCss(isDark: Boolean): String {
    val vars = if (isDark) {
        """
        :root {
            --bg-color: rgba(32, 33, 36, 0.8);
            --text-color: #e8eaed;
            --header-text: #9aa0a6;
            --placeholder-color: #9aa0a6;
            --scrollbar-thumb: #5f6368;
            --scrollbar-thumb-hover: #80868b;
            --btn-bg: #3c4043;
            --btn-border: rgba(255, 255, 255, 0.1);
            --mic-fill: #8ab4f8;
            --mic-border: transparent;
            --mic-hover-bg: rgba(138, 180, 248, 0.12);
            --send-fill: #8ab4f8;
            --send-border: transparent;
            --send-hover-bg: rgba(138, 180, 248, 0.12);
            --close-hover-bg: rgba(232, 234, 237, 0.08);
            --container-border: 1px solid #3c4043;
            --container-shadow: 0 0px 16px rgba(0,0,0,0.25);
            --input-bg: #303134;
            --input-border: 1px solid transparent;
            --wave-color: #8ab4f8;
        }
        """.trimIndent()
    } else {
        """
        :root {
            --bg-color: rgba(255, 255, 255, 0.75);
            --text-color: #202124;
            --header-text: #5f6368;
            --wave-color: #1a73e8;
            --placeholder-color: #5f6368;
            --scrollbar-thumb: #dadce0;
            --scrollbar-thumb-hover: #bdc1c6;
            --btn-bg: #ffffff;
            --btn-border: #dadce0;
            --mic-fill: #1a73e8;
            --mic-border: transparent;
            --mic-hover-bg: rgba(26, 115, 232, 0.06);
            --send-fill: #1a73e8;
            --send-border: transparent;
            --send-hover-bg: rgba(26, 115, 232, 0.06);
            --close-hover-bg: rgba(32, 33, 36, 0.04);
            --container-border: 1px solid #dadce0;
            --container-shadow: 0 0px 16px rgba(0,0,0,0.25);
            --input-bg: #f1f3f4;
            --input-border: 1px solid transparent;
        }
        """.trimIndent()
    }

    return """
        $vars

        html, body {
            width: 100%;
            height: 100%;
            overflow: hidden;
            background: transparent;
            padding: 10px;
            font-family: 'Google Sans Flex', sans-serif;
            font-variation-settings: 'ROND' 100;
        }

        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
            user-select: none;
            font-variation-settings: 'ROND' 100;
        }

        *::-webkit-scrollbar {
            width: 10px;
            height: 10px;
            background: transparent;
        }
        *::-webkit-scrollbar-thumb {
            background: var(--scrollbar-thumb);
            border-radius: 5px;
            border: 2px solid transparent;
            background-clip: content-box;
        }
        *::-webkit-scrollbar-thumb:hover {
            background: var(--scrollbar-thumb-hover);
            border: 2px solid transparent;
            background-clip: content-box;
        }

        .editor-container {
            width: calc(100% - 20px);
            height: calc(100% - 20px);
            margin: 10px;
            display: flex;
            flex-direction: column;
            overflow: hidden;
            background: var(--bg-color);
            position: relative;
            border-radius: 20px;
            border: var(--container-border);
            box-shadow: var(--container-shadow);
            opacity: 0;
            transform: scale(0.95);
            transition: background 0.2s, border-color 0.2s;
        }

        .editor-container.entering {
            animation: inputFadeIn 0.2s cubic-bezier(0.2, 0, 0, 1) forwards;
        }

        .editor-container.exiting {
            animation: inputFadeOut 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
        }

        @keyframes inputFadeIn {
            to { opacity: 1; transform: scale(1); }
        }

        @keyframes inputFadeOut {
            from { opacity: 1; transform: scale(1); }
            to { opacity: 0; transform: scale(0.95); }
        }

        .header {
            height: 32px;
            background: transparent;
            display: flex;
            align-items: center;
            padding: 0 10px;
            cursor: default;
            touch-action: none;
        }

        .header-title {
            flex: 1;
            font-size: 14px;
            font-weight: 600;
            text-transform: uppercase;
            font-stretch: 151%;
            letter-spacing: 0.15em;
            line-height: 24px;
            padding-top: 4px;
            color: var(--header-text);
            padding-left: 14px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            font-family: 'Google Sans Flex', sans-serif;
        }

        .header-title span {
            display: inline-block;
            transition: color 0.2s;
        }

        @keyframes waveColor {
            0%, 100% {
                color: var(--header-text);
                font-variation-settings: 'GRAD' 0, 'wght' 600, 'ROND' 100;
            }
            50% {
                color: var(--wave-color);
                font-variation-settings: 'GRAD' 200, 'wght' 1000, 'ROND' 100;
            }
        }

        .close-btn {
            width: 32px;
            height: 32px;
            display: flex;
            align-items: center;
            justify-content: center;
            border-radius: 50%;
            cursor: pointer;
            color: var(--header-text);
            transition: background 0.1s;
            margin-right: 6px;
        }

        .close-btn svg {
            width: 20px;
            height: 20px;
            fill: currentColor;
        }

        .mic-btn svg, .send-btn svg {
            width: 22px;
            height: 22px;
        }

        .mic-btn svg { fill: var(--mic-fill); }
        .send-btn svg { fill: var(--send-fill); }

        .close-btn:hover {
            background: var(--close-hover-bg);
        }

        #editor {
            flex: 1;
            width: 100%;
            margin: 0px 8px 8px 8px;
            background: var(--input-bg);
            border-radius: 22px;
            padding: 12px 14px;
            padding-right: 68px;
            border: var(--input-border);
            outline: none;
            resize: none;
            font-family: 'Google Sans Flex', sans-serif;
            font-size: 15px;
            line-height: 1.55;
            color: var(--text-color);
            overflow-y: auto;
            user-select: text;
            width: calc(100% - 16px);
        }

        #editor::placeholder {
            color: var(--placeholder-color);
            opacity: 1;
        }

        .btn-container {
            position: absolute;
            bottom: 13px;
            right: 13px;
            display: flex;
            flex-direction: column;
            gap: 10px;
            z-index: 100;
        }

        .mic-btn, .send-btn {
            width: 50px;
            height: 50px;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            cursor: pointer;
            background: var(--btn-bg);
            border: 1px solid var(--btn-border);
            box-shadow: 0 2px 8px rgba(0,0,0,0.1);
            transition: all 0.2s cubic-bezier(0.2, 0.0, 0.2, 1);
            backdrop-filter: blur(8px);
            -webkit-backdrop-filter: blur(8px);
        }

        .mic-btn svg, .send-btn svg {
            width: 27px;
            height: 27px;
            transition: transform 0.2s, fill 0.2s;
        }

        .mic-btn:active, .send-btn:active {
            transform: scale(0.95);
        }

        .mic-btn:hover {
            background: var(--mic-hover-bg);
            border-color: var(--mic-fill);
        }

        .send-btn:hover {
            background: var(--send-hover-bg);
            border-color: var(--send-fill);
        }
    """.trimIndent()
}

internal fun presetTextInputJavascript(): String {
    return """
        const container = document.querySelector('.editor-container');
        const headerRegion = document.getElementById('headerRegion');
        const editor = document.getElementById('editor');
        const closeBtn = document.getElementById('closeBtn');
        const micBtn = document.getElementById('micBtn');
        const sendBtn = document.getElementById('sendBtn');
        let dragTouch = null;
        let dragWasActive = false;
        const TOUCH_DRAG_GAIN = Math.max(window.devicePixelRatio || 1, 1.85);

        container.addEventListener('mousedown', (e) => {
            const isInteractive = e.target.closest('#editor') ||
                                e.target.closest('.close-btn') ||
                                e.target.closest('.mic-btn') ||
                                e.target.closest('.send-btn');
            if (isInteractive) return;
            if (e.button === 0) {
                window.ipc.postMessage('drag_window');
            }
        });

        headerRegion.addEventListener('touchstart', (e) => {
            if (e.touches.length !== 1) return;
            dragTouch = {
                x: e.touches[0].screenX,
                y: e.touches[0].screenY
            };
        }, { passive: true });

        headerRegion.addEventListener('touchmove', (e) => {
            if (!dragTouch || e.touches.length !== 1) return;
            const touch = e.touches[0];
            const dx = (touch.screenX - dragTouch.x) * TOUCH_DRAG_GAIN;
            const dy = (touch.screenY - dragTouch.y) * TOUCH_DRAG_GAIN;
            if (Math.abs(dx) >= 1 || Math.abs(dy) >= 1) {
                dragWasActive = true;
                window.ipc.postMessage(JSON.stringify({
                    type: 'dragInputWindow',
                    dx,
                    dy
                }));
                window.ipc.postMessage('dragAt:' + Math.round(touch.screenX) + ',' + Math.round(touch.screenY));
            }
            dragTouch = {
                x: touch.screenX,
                y: touch.screenY
            };
            e.preventDefault();
        }, { passive: false });

        const finishDragTouch = (event) => {
            if (dragWasActive) {
                const point = (event && event.changedTouches && event.changedTouches[0]) || null;
                if (point) {
                    window.ipc.postMessage('dragEnd:' + Math.round(point.screenX) + ',' + Math.round(point.screenY));
                } else if (dragTouch) {
                    window.ipc.postMessage('dragEnd:' + Math.round(dragTouch.x) + ',' + Math.round(dragTouch.y));
                }
            }
            dragWasActive = false;
            dragTouch = null;
        };
        headerRegion.addEventListener('touchend', finishDragTouch);
        headerRegion.addEventListener('touchcancel', finishDragTouch);

        closeBtn.addEventListener('click', () => {
            window.ipc.postMessage('close_window');
        });

        window.onload = () => {
            setTimeout(() => editor.focus(), 50);
        };

        editor.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                const text = editor.value.trim();
                if (text) {
                    window.ipc.postMessage('submit:' + text);
                }
            }

            if (e.key === 'Escape') {
                e.preventDefault();
                window.ipc.postMessage('cancel');
            }

            if (e.key === 'ArrowUp') {
                const isSingleLine = !editor.value.includes('\n');
                if ((isSingleLine || editor.selectionStart === 0) && !e.shiftKey) {
                    e.preventDefault();
                    window.ipc.postMessage('history_up:' + editor.value);
                }
            }

            if (e.key === 'ArrowDown') {
                const isSingleLine = !editor.value.includes('\n');
                if ((isSingleLine || editor.selectionStart === editor.value.length) && !e.shiftKey) {
                    e.preventDefault();
                    window.ipc.postMessage('history_down:' + editor.value);
                }
            }
        });

        micBtn.addEventListener('click', (e) => {
            e.preventDefault();
            window.ipc.postMessage('mic');
        });

        sendBtn.addEventListener('click', (e) => {
            e.preventDefault();
            const text = editor.value.trim();
            if (text) {
                window.ipc.postMessage('submit:' + text);
            }
        });

        document.addEventListener('contextmenu', e => e.preventDefault());

        window.setEditorText = (text) => {
            editor.value = text;
            editor.selectionStart = editor.selectionEnd = text.length;
            editor.focus();
        };

        window.insertTextAtCursor = (text) => {
            const start = editor.selectionStart;
            const end = editor.selectionEnd;
            const before = editor.value.substring(0, start);
            const after = editor.value.substring(end);
            editor.value = before + text + after;
            const newPos = start + text.length;
            editor.selectionStart = editor.selectionEnd = newPos;
            editor.focus();
        };

        window.clearInput = () => {
            editor.value = '';
            editor.focus();
        };

        window.exportDraftState = () => {
            return JSON.stringify({
                text: editor.value || ''
            });
        };

        window.restoreDraftState = (raw) => {
            const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
            editor.value = data && data.text ? data.text : '';
            editor.focus();
            editor.selectionStart = editor.selectionEnd = editor.value.length;
        };

        window.focusEditor = () => {
            editor.focus();
            editor.selectionStart = editor.selectionEnd = editor.value.length;
        };

        window.updateTheme = (isDark) => {
            document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light');
        };

        window.updateInputChrome = (titleText, footerText, placeholderText) => {
            const title = document.getElementById('headerTitle');
            if (title) {
                title.removeAttribute('data-wrapped');
                title.textContent = titleText;
            }
            editor.placeholder = placeholderText;
        };

        window.playEntry = () => {
            const el = document.querySelector('.editor-container');
            if (el) {
                el.classList.remove('exiting');
                el.classList.add('entering');

                const title = document.getElementById('headerTitle');
                if (title) {
                    const text = title.textContent || '';
                    title.innerHTML = text.split('').map((char, i) =>
                        '<span style="animation: waveColor 0.6s ease forwards ' + (0.2 + i * 0.05) + 's">' +
                        (char === ' ' ? '&nbsp;' : char) +
                        '</span>'
                    ).join('');
                    title.setAttribute('data-wrapped', 'true');
                }
            }
        };

        window.playExit = () => {
            const el = document.querySelector('.editor-container');
            if (el) {
                el.classList.remove('entering');
                el.classList.add('exiting');
            }
        };

        window.closeWithAnimation = () => {
            window.playExit();
            window.setTimeout(() => window.ipc.postMessage('input_exit_done'), 170);
        };
    """.trimIndent()
}
