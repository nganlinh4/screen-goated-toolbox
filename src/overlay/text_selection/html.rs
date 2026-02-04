// --- TEXT SELECTION HTML ---
// HTML content and localization helpers for the badge WebView.

/// Get localized badge text based on language and continuous mode status
pub fn get_localized_badge_text(lang: &str, is_continuous: bool) -> String {
    if is_continuous {
        match lang {
            "vi" => "Bôi đen văn bản (Liên tục)",
            "ko" => "텍스트 선택 (연속)",
            _ => "Select text (Continuous)",
        }
    } else {
        match lang {
            "vi" => "Bôi đen văn bản...",
            "ko" => "텍스트 선택...",
            _ => "Select text...",
        }
    }
    .to_string()
}

/// Get localized image badge text (always continuous mode since image badge only shows in continuous mode)
pub fn get_localized_image_badge_text(lang: &str) -> String {
    match lang {
        "vi" => "Chọn vùng MH (Liên tục)",
        "ko" => "화면 선택 (연속)",
        _ => "Select area (Continuous)",
    }
    .to_string()
}

/// Generate the HTML content for the badge WebView
pub fn get_html(initial_text: &str) -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        {font_css}
        :root {{
            --bg-color: rgba(255, 255, 255, 0.95);
            --text-color: #202124;
            /* Aurora Gradient - Idle (Blue-Violet-Cyan) */
            --g1: #0033cc;
            --g2: #00ddff;
            --g3: #8844ff;
            /* Aurora Gradient - Active (Red-Gold-Purple DRAMATIC) */
            --a1: #ff0055;
            --a2: #ffdd00;
            --a3: #aa00ff;
            --wave-color: #1a73e8;
        }}
        [data-theme="dark"] {{
            --bg-color: rgba(26, 26, 26, 0.95);
            --text-color: #ffffff;
            /* Aurora Gradient - Idle (Neon Synthwave) */
            --g1: #2bd9fe;
            --g2: #aa22ff;
            --g3: #00fe9b;
            /* Aurora Gradient - Active (Hyper Energy) */
            --a1: #ff00cc;
            --a2: #ccff00;
            --a3: #ff2200;
            --wave-color: #8ab4f8;
        }}

        * {{
            margin: 0;
            padding: 0;
            user-select: none;
            cursor: default;
        }}

        body {{
            background: transparent;
            overflow: hidden;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 100vh;
            width: 100vw;
            font-family: 'Google Sans Flex Rounded', 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
            font-weight: 500;
        }}

        .badges-wrapper {{
            display: flex;
            flex-direction: column;
            align-items: center;
            gap: 6px;
            /* Smooth height transitions when badges show/hide */
            transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
        }}

        /* Sliding animations for badge stack */
        .badges-wrapper.only-image {{
            /* When only image badge is visible, center it */
        }}

        .badges-wrapper.only-text {{
            /* When only text badge is visible, center it */
        }}

        .badges-wrapper.both-visible {{
            /* When both badges are visible, stack them */
        }}

        /* Clip the glow to the container shape to prevent "inside out" giant square */
        .badge-container {{
            position: relative;
            padding: 2px; /* Border thickness */
            border-radius: 999px; /* Pill shape */
            background: var(--bg-color); /* Opaque track */
            overflow: hidden; /* CRITICAL FIX: Clips the spinning gradient */
            opacity: 0; /* Default invisible */
            transform: translateY(10px);
            /* Remove default animation, handled by classes */
            box-shadow: 0 4px 12px rgba(0,0,0,0.25);
            transition: box-shadow 0.2s, transform 0.2s, margin 0.25s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.2s;
        }}

        .badge-container.entering {{
            animation: slideIn 0.25s cubic-bezier(0.4, 0, 0.2, 1) forwards;
        }}

        .badge-container.exiting {{
            animation: slideOut 0.2s cubic-bezier(0.4, 0, 0.2, 1) forwards;
        }}

        .badge-glow {{
            position: absolute;
            top: -50%;
            left: -50%;
            width: 200%;
            height: 200%;
            background: conic-gradient(
                from 0deg,
                var(--c1),
                var(--c2),
                var(--c3),
                var(--c2),
                var(--c1)
            );
            animation: spin 4s linear infinite; /* Slower, smoother flow */
            opacity: 1;
            z-index: 0;
            filter: blur(2px); /* Soften the gradient blends */
        }}

        .badge-inner {{
            position: relative;
            background: var(--bg-color); /* Covers the center */
            color: var(--text-color);
            padding: 3px 10px;
            border-radius: 999px; /* Match parent */
            font-size: 12px;
            white-space: nowrap;
            z-index: 1; /* Sit above glow */
            display: flex;
            align-items: center;
            gap: 8px;
            font-stretch: condensed;
            letter-spacing: -0.2px;
            box-shadow: 0 0 4px 1px var(--bg-color); /* Soft edge blending */
        }}

        /* Slide In animation - enters from below with bounce */
        @keyframes slideIn {{
            from {{
                opacity: 0;
                transform: translateY(15px) scale(0.95);
            }}
            to {{
                opacity: 1;
                transform: translateY(0) scale(1);
            }}
        }}

        /* Slide Out animation - exits upward smoothly */
        @keyframes slideOut {{
            from {{
                opacity: 1;
                transform: translateY(0) scale(1);
            }}
            to {{
                opacity: 0;
                transform: translateY(-8px) scale(0.95);
            }}
        }}

        /* Legacy fadeIn for compatibility */
        @keyframes fadeIn {{
            to {{ opacity: 1; transform: translateY(0); }}
        }}

        @keyframes spin {{
            from {{ transform: rotate(0deg); }}
            to {{ transform: rotate(360deg); }}
        }}

        @keyframes waveColor {{
            0% {{
                color: var(--a1);
                font-variation-settings: 'GRAD' 0, 'wght' 500, 'ROND' 100;
                transform: translateY(0px) scale(1);
            }}
            33% {{
                color: var(--a2);
                font-variation-settings: 'GRAD' 200, 'wght' 900, 'ROND' 100;
                transform: translateY(-2px) scale(1.1);
            }}
            66% {{
                color: var(--a3);
                font-variation-settings: 'GRAD' 200, 'wght' 900, 'ROND' 100;
                transform: translateY(-1px) scale(1.1);
            }}
            100% {{
                color: var(--a1);
                font-variation-settings: 'GRAD' 0, 'wght' 500, 'ROND' 100;
                transform: translateY(0px) scale(1);
            }}
        }}

        @keyframes idleWave {{
            0% {{
                color: var(--g1);
                font-variation-settings: 'GRAD' 0, 'wght' 400, 'ROND' 100;
            }}
            50% {{
                color: var(--g2);
                font-variation-settings: 'GRAD' 50, 'wght' 600, 'ROND' 100;
            }}
            100% {{
                color: var(--g1);
                font-variation-settings: 'GRAD' 0, 'wght' 400, 'ROND' 100;
            }}
        }}

        @keyframes fadeOut {{
            from {{ opacity: 1; transform: translateY(0); }}
            to {{ opacity: 0; transform: translateY(-10px); }}
        }}

        /* State: Selecting (Active) */
        body.selecting .badge-glow {{
            --c1: var(--a1);
            --c2: var(--a2);
            --c3: var(--a3);
            animation: spin 0.8s linear infinite; /* Faster spin for urgency */
        }}

        body.selecting .badge-container {{
            transform: scale(1.05);
            /* Soft orange outer glow */
            box-shadow: 0 0 15px rgba(255, 94, 0, 0.4), 0 4px 12px rgba(0,0,0,0.3);
        }}

        /* State: Idle */
        body:not(.selecting) .badge-glow {{
            --c1: var(--g1);
            --c2: var(--g2);
            --c3: var(--g3);
        }}

        /* Image badge specific styles */
        #image-badge {{
            display: none; /* Hidden by default */
        }}

        #image-badge.visible {{
            display: block;
        }}

        #image-badge .badge-glow {{
            /* Cyan-Green gradient for image mode */
            --c1: #00cc88;
            --c2: #00bbff;
            --c3: #8800ff;
        }}

        #image-badge .badge-inner {{
            /* Slightly different color accent */
            background: var(--bg-color);
        }}

    </style>
</head>
<body>
    <div class="badges-wrapper">
        <!-- Image Continuous Badge (above text badge) -->
        <div id="image-badge" class="badge-container">
            <div class="badge-glow"></div>
            <div class="badge-inner">
                <span id="image-text">Chọn vùng MH</span>
            </div>
        </div>

        <!-- Text Selection Badge -->
        <div id="text-badge" class="badge-container">
            <div class="badge-glow"></div>
            <div class="badge-inner">
                <span id="text">{text}</span>
            </div>
        </div>
    </div>

    <script>
        // Text badge entry/exit
        function playEntry() {{
            const el = document.getElementById('text-badge');
            if(el) {{
                // Cancel any pending hide timer
                if (window.textBadgeTimer) {{
                    clearTimeout(window.textBadgeTimer);
                    window.textBadgeTimer = null;
                }}
                // Cancel any pending exit animation timer
                if (window.textBadgeExitTimer) {{
                    clearTimeout(window.textBadgeExitTimer);
                    window.textBadgeExitTimer = null;
                }}
                el.style.display = 'block';
                el.classList.remove('exiting');
                // Small delay to allow display:block to take effect before animation
                requestAnimationFrame(() => {{
                    el.classList.add('entering');
                    updateWrapperState();
                }});
            }}
        }}

        // Helper to update badge wrapper state classes
        function updateWrapperState() {{
            const wrapper = document.querySelector('.badges-wrapper');
            const imageBadge = document.getElementById('image-badge');
            const textBadge = document.getElementById('text-badge');

            const imageVisible = imageBadge && imageBadge.style.display !== 'none';
            const textVisible = textBadge && textBadge.style.display !== 'none';

            wrapper.classList.remove('only-image', 'only-text', 'both-visible');

            if (imageVisible && textVisible) {{
                wrapper.classList.add('both-visible');
            }} else if (imageVisible) {{
                wrapper.classList.add('only-image');
            }} else if (textVisible) {{
                wrapper.classList.add('only-text');
            }}
        }}

        function playExit() {{
            const el = document.getElementById('text-badge');
            console.log('[JS] playExit called, el=', el);
            if(el) {{
                el.classList.remove('entering');
                el.classList.add('exiting');
                // Cancel any pending timer first
                if (window.textBadgeExitTimer) {{
                    clearTimeout(window.textBadgeExitTimer);
                }}
                // Use a short delay to allow exit animation to play
                window.textBadgeExitTimer = setTimeout(() => {{
                    el.style.display = 'none';
                    el.classList.remove('exiting');
                    window.textBadgeExitTimer = null;
                    updateWrapperState();
                }}, 200);
            }}
        }}

        // Image badge show/hide
        function showImageBadge() {{
            const el = document.getElementById('image-badge');
            if(el) {{
                if (window.imageBadgeTimer) {{
                    clearTimeout(window.imageBadgeTimer);
                    window.imageBadgeTimer = null;
                }}
                el.style.display = 'block';
                el.classList.add('visible');
                el.classList.remove('exiting');
                // Small delay to allow display:block to take effect before animation
                requestAnimationFrame(() => {{
                    el.classList.add('entering');
                    updateWrapperState();
                }});
            }}
        }}

        function hideImageBadge() {{
            const el = document.getElementById('image-badge');
            if(el) {{
                el.classList.remove('entering');
                el.classList.add('exiting');
                // Remove visible class after animation
                if (window.imageBadgeTimer) clearTimeout(window.imageBadgeTimer);
                window.imageBadgeTimer = setTimeout(() => {{
                    el.classList.remove('visible', 'exiting');
                    el.style.display = 'none';
                    window.imageBadgeTimer = null;
                    updateWrapperState();
                }}, 200);
            }}
        }}

        // Update image badge text with wave animation (same as text badge)
        function updateImageText(newText) {{
            const title = document.getElementById('image-text');
            if (title) {{
                // Apply gentle idle wave animation to each character
                const chars = newText.split('');
                title.innerHTML = chars.map((char, i) =>
                    `<span style="
                        display: inline-block;
                        animation: idleWave 3s ease-in-out infinite;
                        animation-delay: ${{i * 0.1}}s;
                    ">${{char === ' ' ? '&nbsp;' : char}}</span>`
                ).join('');
            }}
        }}

        function updateState(isSelecting, newText) {{
            if (isSelecting) {{
                document.body.classList.add('selecting');
            }} else {{
                document.body.classList.remove('selecting');
            }}

            const title = document.getElementById('text');
            if (isSelecting) {{
                // Apply DRAMATIC, SPEEDY, LOOPING Wave Animation
                const chars = newText.split('');
                title.innerHTML = chars.map((char, i) =>
                    `<span style="
                        display: inline-block;
                        animation: waveColor 0.6s linear infinite;
                        animation-delay: ${{i * 0.05}}s;
                    ">${{char === ' ' ? '&nbsp;' : char}}</span>`
                ).join('');
            }} else {{
                // Idle State: Gentle Blue Wave
                const chars = newText.split('');
                title.innerHTML = chars.map((char, i) =>
                    `<span style="
                        display: inline-block;
                        animation: idleWave 3s ease-in-out infinite;
                        animation-delay: ${{i * 0.1}}s;
                    ">${{char === ' ' ? '&nbsp;' : char}}</span>`
                ).join('');
            }}
        }}

        function updateTheme(isDark) {{
            if (isDark) {{
                document.documentElement.setAttribute('data-theme', 'dark');
            }} else {{
                document.documentElement.removeAttribute('data-theme');
            }}
        }}
    </script>
</body>
</html>"#,
        font_css = font_css,
        text = initial_text
    )
}
