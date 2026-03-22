package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import java.util.Base64

internal fun inputAdapterOverlayContent(
    input: PresetInput,
    uiLanguage: String,
): String? {
    return when (input) {
        is PresetInput.Text -> input.text
        is PresetInput.Image -> generateImageDisplayHtml(input.pngBytes)
        is PresetInput.Audio -> generateAudioPlayerHtml(input.wavBytes, uiLanguage)
    }
}

internal fun isInputAdapterMediaHtml(content: String): Boolean {
    return content.contains("data-sgt-input-adapter-media=\"image\"") ||
        content.contains("data-sgt-input-adapter-media=\"audio\"") ||
        content.contains("class=\"audio-player\"")
}

private fun generateImageDisplayHtml(imageBytes: ByteArray): String {
    val base64Image = Base64.getEncoder().encodeToString(imageBytes)
    val mimeType = sniffImageMimeType(imageBytes)
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
            ${overlayFontCss()}
            * { margin: 0; padding: 0; box-sizing: border-box; }
            html, body {
                width: 100%;
                height: 100%;
                overflow: hidden;
                background: transparent;
            }
            body {
                display: flex;
                justify-content: center;
                align-items: center;
                font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
            }
            ::-webkit-scrollbar { display: none; }
            .container {
                position: relative;
                width: 100%;
                height: 100%;
                display: flex;
                justify-content: center;
                align-items: center;
                background: rgba(20, 20, 25, 0.98);
                border-radius: 8px;
            }
            .image {
                width: 100%;
                height: auto;
                object-fit: contain;
                border-radius: 8px;
                transition: opacity 0.15s ease;
            }
            </style>
        </head>
        <body>
            <div class="container" data-sgt-input-adapter-media="image">
                <img class="image" alt="" src="data:$mimeType;base64,$base64Image" />
            </div>
        </body>
        </html>
    """.trimIndent()
}

private fun generateAudioPlayerHtml(audioBytes: ByteArray, uiLanguage: String): String {
    val base64Audio = Base64.getEncoder().encodeToString(audioBytes)
    val mimeType = sniffAudioMimeType(audioBytes)
    val downloadedToast = when (uiLanguage) {
        "vi" -> "Đã tải xuống"
        "ko" -> "다운로드됨"
        else -> "Downloaded"
    }
    val downloadTooltip = when (uiLanguage) {
        "vi" -> "Tải xuống"
        "ko" -> "다운로드"
        else -> "Download"
    }
    val downloadFailedToast = when (uiLanguage) {
        "vi" -> "Không thể tải xuống"
        "ko" -> "다운로드할 수 없음"
        else -> "Could not download"
    }
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
            ${overlayFontCss()}
            * { margin: 0; padding: 0; box-sizing: border-box; }
            html, body {
                width: 100%;
                height: 100%;
                background: transparent;
                overflow: hidden;
            }
            body {
                display: flex;
                justify-content: center;
                align-items: center;
                font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
            }
            ::-webkit-scrollbar { display: none; }
            .audio-player {
                background: #1e1e1e;
                border-radius: 12px;
                padding: 20px 24px;
                width: 100%;
                max-width: 400px;
                box-shadow: 0 4px 24px rgba(0, 0, 0, 0.3);
                border: 1px solid rgba(255, 255, 255, 0.08);
                position: relative;
            }
            .waveform {
                display: flex;
                align-items: center;
                justify-content: center;
                height: 48px;
                margin-bottom: 16px;
                gap: 2px;
            }
            .wave-bar {
                width: 3px;
                min-height: 4px;
                background: #8ab4f8;
                border-radius: 2px;
                transition: height 0.05s ease-out;
            }
            .controls {
                display: flex;
                align-items: center;
                gap: 14px;
            }
            .play-btn {
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
            }
            .play-btn svg {
                fill: #1e1e1e;
                width: 18px;
                height: 18px;
                margin-left: 2px;
            }
            .play-btn.playing svg {
                margin-left: 0;
            }
            .download-btn {
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
            }
            .download-btn svg {
                fill: #9aa0a6;
                width: 20px;
                height: 20px;
                transition: fill 0.2s;
            }
            .download-btn.success svg {
                fill: #4CAF50;
            }
            .progress-container {
                flex: 1;
                display: flex;
                flex-direction: column;
                gap: 6px;
            }
            .progress-bar {
                height: 4px;
                background: rgba(255,255,255,0.1);
                border-radius: 2px;
                overflow: hidden;
                cursor: pointer;
            }
            .progress-fill {
                height: 100%;
                background: #8ab4f8;
                border-radius: 2px;
                width: 0%;
                transition: width 0.1s;
            }
            .time-display {
                display: flex;
                justify-content: space-between;
                font-size: 11px;
                color: #9aa0a6;
            }
            .toast {
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
            }
            .toast.show {
                opacity: 1;
            }
            audio {
                display: none;
            }
            </style>
        </head>
        <body>
            <div class="audio-player" data-sgt-input-adapter-media="audio">
                <div class="toast" id="toast">$downloadedToast</div>
                <div class="waveform" id="waveform"></div>
                <div class="controls">
                    <button class="play-btn" id="playBtn">
                        <svg id="playIcon" viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>
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
                    <button class="download-btn" id="downloadBtn" title="$downloadTooltip">
                        <svg viewBox="0 0 24 24"><path d="M5 20h14v-2H5v2zM19 9h-4V3H9v6H5l7 7 7-7z"/></svg>
                    </button>
                </div>
            </div>
            <audio id="audio">
                <source src="data:$mimeType;base64,$base64Audio" type="$mimeType">
            </audio>
            <script>
            const audio = document.getElementById('audio');
            const progress = document.getElementById('progress');
            const playIcon = document.getElementById('playIcon');
            const playBtn = document.getElementById('playBtn');
            const downloadBtn = document.getElementById('downloadBtn');
            const toast = document.getElementById('toast');
            const currentTimeEl = document.getElementById('current');
            const durationEl = document.getElementById('duration');
            const waveformEl = document.getElementById('waveform');
            const progressBar = document.getElementById('progressBar');

            const BAR_COUNT = 32;
            for (let i = 0; i < BAR_COUNT; i += 1) {
                const bar = document.createElement('div');
                bar.className = 'wave-bar';
                bar.style.height = '4px';
                waveformEl.appendChild(bar);
            }
            const bars = waveformEl.querySelectorAll('.wave-bar');

            let audioContext, analyser, source, dataArray;
            let isSetup = false;

            function setupAudio() {
                if (isSetup) return;
                audioContext = new (window.AudioContext || window.webkitAudioContext)();
                analyser = audioContext.createAnalyser();
                analyser.fftSize = 64;
                source = audioContext.createMediaElementSource(audio);
                source.connect(analyser);
                analyser.connect(audioContext.destination);
                dataArray = new Uint8Array(analyser.frequencyBinCount);
                isSetup = true;
            }

            function formatTime(seconds) {
                if (isNaN(seconds)) return '0:00';
                const minutes = Math.floor(seconds / 60);
                const secs = Math.floor(seconds % 60);
                return minutes + ':' + (secs < 10 ? '0' : '') + secs;
            }

            function visualize() {
                if (!analyser || audio.paused) return;
                analyser.getByteFrequencyData(dataArray);
                for (let i = 0; i < BAR_COUNT; i += 1) {
                    const idx = Math.floor(i * dataArray.length / BAR_COUNT);
                    const value = dataArray[idx];
                    const height = Math.max(4, (value / 255) * 44);
                    bars[i].style.height = height + 'px';
                }
                requestAnimationFrame(visualize);
            }

            audio.onloadedmetadata = function() {
                durationEl.textContent = formatTime(audio.duration);
            };

            audio.ontimeupdate = function() {
                const pct = (audio.currentTime / audio.duration) * 100;
                progress.style.width = pct + '%';
                currentTimeEl.textContent = formatTime(audio.currentTime);
            };

            audio.onended = function() {
                playIcon.innerHTML = '<path d="M8 5v14l11-7z"/>';
                playBtn.classList.remove('playing');
                bars.forEach(function(bar) { bar.style.height = '4px'; });
            };

            playBtn.onclick = function() {
                setupAudio();
                if (audio.paused) {
                    audio.play();
                    playIcon.innerHTML = '<path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/>';
                    playBtn.classList.add('playing');
                    visualize();
                } else {
                    audio.pause();
                    playIcon.innerHTML = '<path d="M8 5v14l11-7z"/>';
                    playBtn.classList.remove('playing');
                }
            };

            downloadBtn.onclick = function() {
                if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                    window.sgtAndroid.postMessage(JSON.stringify({
                        type: 'saveMediaToDownloads',
                        filename: 'recording.wav',
                        mimeType: '$mimeType',
                        base64: '$base64Audio',
                        successMessage: '$downloadedToast',
                        failureMessage: '$downloadFailedToast'
                    }));
                }

                const originalIcon = downloadBtn.innerHTML;
                downloadBtn.innerHTML = '<svg viewBox="0 0 24 24"><path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/></svg>';
                downloadBtn.classList.add('success');
                toast.classList.add('show');

                setTimeout(function() {
                    downloadBtn.innerHTML = originalIcon;
                    downloadBtn.classList.remove('success');
                    toast.classList.remove('show');
                }, 2500);
            };

            progressBar.onclick = function(event) {
                const rect = progressBar.getBoundingClientRect();
                const pct = (event.clientX - rect.left) / rect.width;
                audio.currentTime = pct * audio.duration;
            };
            </script>
        </body>
        </html>
    """.trimIndent()
}

private fun sniffImageMimeType(imageBytes: ByteArray): String {
    return when {
        imageBytes.size >= 3 &&
            imageBytes[0] == 0xFF.toByte() &&
            imageBytes[1] == 0xD8.toByte() &&
            imageBytes[2] == 0xFF.toByte() -> "image/jpeg"
        imageBytes.size >= 12 &&
            imageBytes.copyOfRange(0, 4).contentEquals(byteArrayOf('R'.code.toByte(), 'I'.code.toByte(), 'F'.code.toByte(), 'F'.code.toByte())) &&
            imageBytes.copyOfRange(8, 12).contentEquals(byteArrayOf('W'.code.toByte(), 'E'.code.toByte(), 'B'.code.toByte(), 'P'.code.toByte())) -> "image/webp"
        else -> "image/png"
    }
}

private fun sniffAudioMimeType(audioBytes: ByteArray): String {
    return when {
        audioBytes.size >= 12 &&
            audioBytes.copyOfRange(0, 4).contentEquals(byteArrayOf('R'.code.toByte(), 'I'.code.toByte(), 'F'.code.toByte(), 'F'.code.toByte())) &&
            audioBytes.copyOfRange(8, 12).contentEquals(byteArrayOf('W'.code.toByte(), 'A'.code.toByte(), 'V'.code.toByte(), 'E'.code.toByte())) -> "audio/wav"
        audioBytes.size >= 4 &&
            audioBytes.copyOfRange(0, 4).contentEquals(byteArrayOf('O'.code.toByte(), 'g'.code.toByte(), 'g'.code.toByte(), 'S'.code.toByte())) -> "audio/ogg"
        else -> "audio/wav"
    }
}
