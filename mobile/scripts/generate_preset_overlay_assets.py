#!/usr/bin/env python3
"""Generate Android preset overlay assets from the canonical Windows sources."""

from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content.encode("utf-8"))


def extract_raw_string(source: str, marker: str) -> str:
    marker_index = source.find(marker)
    if marker_index < 0:
        raise ValueError(f"Missing marker: {marker}")
    start = source.find('r#"', marker_index)
    if start < 0:
        raise ValueError(f"Missing raw string start for: {marker}")
    content_start = start + 3
    end = source.find('"#', content_start)
    if end < 0:
        raise ValueError(f"Missing raw string end for: {marker}")
    return source[content_start:end]


def extract_concat_includes(source_file: Path, marker: str) -> str:
    source = read(source_file)
    marker_index = source.find(marker)
    if marker_index < 0:
        raise ValueError(f"Missing marker: {marker}")
    start = source.find("concat!(", marker_index)
    end = source.find(");", start)
    if start < 0 or end < 0:
        raise ValueError(f"Missing concat block for: {marker}")
    paths = re.findall(r'include_str!\("([^"]+)"\)', source[start:end])
    if not paths:
        raise ValueError(f"Missing include_str entries for: {marker}")
    return "".join(read(source_file.parent / path) for path in paths)


def extract_quoted_strings(source: str, marker: str, count: int) -> list[str]:
    marker_index = source.find(marker)
    if marker_index < 0:
        raise ValueError(f"Missing marker: {marker}")
    values = re.findall(r'"([^"]*)"', source[marker_index:])[:count]
    if len(values) != count:
        raise ValueError(f"Missing quoted strings for: {marker}")
    return values


def extract_match_arm_raw_string(source: str, arm_name: str) -> str:
    pattern = re.compile(
        rf'"{re.escape(arm_name)}"\s*=>\s*\{{\s*r(#+)"(.*?)"\1\s*\}}',
        re.DOTALL,
    )
    match = pattern.search(source)
    if match is None:
        raise ValueError(f"Missing raw string match arm: {arm_name}")
    return match.group(2)


def generate(args: argparse.Namespace) -> None:
    output = args.output / "preset_overlay"
    output.mkdir(parents=True, exist_ok=True)

    fit_script = extract_concat_includes(
        args.fit_source,
        "const FIT_FONT_SCRIPT: &str = concat!(",
    )
    write(
        output / "windows_markdown_fit.js",
        "\n".join(
            [
                "window.runWindowsMarkdownFit = function(streamingMode, phase) {",
                f"    const source = {json.dumps(fit_script)};",
                "    const resolved = source",
                '        .replace(/__FIT_PHASE__/g, phase || "mobile_markdown_fit")',
                '        .replace(/__STREAMING_MODE__/g, streamingMode ? "true" : "false");',
                "    return window.eval(resolved);",
                "};",
            ]
        ),
    )

    css_source = read(args.css_source)
    write(
        output / "windows_markdown.css",
        extract_raw_string(css_source, 'pub const MARKDOWN_CSS: &str = r#"'),
    )
    write(
        output / "windows_markdown_theme_dark.css",
        extract_raw_string(css_source, "if is_dark {"),
    )
    write(
        output / "windows_markdown_theme_light.css",
        extract_raw_string(css_source, "} else {"),
    )

    grid_source = read(args.grid_source)
    grid_css_url, grid_js_url = extract_quoted_strings(
        grid_source,
        "pub fn get_lib_urls() -> (&'static str, &'static str) {",
        2,
    )
    write(
        output / "windows_gridjs_urls.json",
        "{\n"
        f'  "cssUrl": {json.dumps(grid_css_url)},\n'
        f'  "jsUrl": {json.dumps(grid_js_url)}\n'
        "}",
    )
    write(
        output / "windows_gridjs.css",
        extract_raw_string(grid_source, "pub fn get_css() -> &'static str {"),
    )
    write(
        output / "windows_gridjs_init.js",
        extract_raw_string(grid_source, "pub fn get_init_script() -> &'static str {"),
    )

    if args.static_assets.is_dir():
        for source in args.static_assets.iterdir():
            if source.is_file():
                shutil.copyfile(source, output / source.name)

    write(
        output / "windows_button_canvas.css",
        extract_raw_string(read(args.button_css_source), "pub fn get_base_css() -> &'static str {"),
    )
    write(
        output / "windows_button_canvas.js",
        extract_raw_string(read(args.button_js_source), "pub fn get_javascript() -> &'static str {"),
    )
    button_theme_source = read(args.button_theme_source)
    write(
        output / "windows_button_canvas_theme_dark.css",
        extract_raw_string(button_theme_source, "if is_dark {"),
    )
    write(
        output / "windows_button_canvas_theme_light.css",
        extract_raw_string(button_theme_source, "} else {"),
    )

    recording_template = extract_raw_string(read(args.recording_source), "format!(")
    recording_template = recording_template.replace("{{", "{").replace("}}", "}")
    replacements = {
        "{font_css}": "{{FONT_CSS}}",
        "{width}": "{{WINDOW_WIDTH}}",
        "{height}": "{{WINDOW_HEIGHT}}",
        "{tx_rec}": "{{TEXT_RECORDING}}",
        "{tx_proc}": "{{TEXT_PROCESSING}}",
        "{tx_wait}": "{{TEXT_WARMUP}}",
        "{tx_init}": "{{TEXT_INITIALIZING}}",
        "{tx_sub}": "{{TEXT_SUBTEXT}}",
        "{tx_paused}": "{{TEXT_PAUSED}}",
        "{icon_pause}": "{{ICON_PAUSE}}",
        "{icon_play}": "{{ICON_PLAY}}",
        "{icon_close}": "{{ICON_CLOSE}}",
        "{container_bg}": "{{COLOR_CONTAINER_BG}}",
        "{container_border}": "{{COLOR_CONTAINER_BORDER}}",
        "{text_color}": "{{COLOR_TEXT}}",
        "{subtext_color}": "{{COLOR_SUBTEXT}}",
        "{btn_bg}": "{{COLOR_BUTTON_BG}}",
        "{btn_hover_bg}": "{{COLOR_BUTTON_HOVER_BG}}",
        "{btn_color}": "{{COLOR_BUTTON}}",
        "{text_shadow}": "{{COLOR_TEXT_SHADOW}}",
        "{is_dark}": "{{IS_DARK}}",
    }
    for old, new in replacements.items():
        recording_template = recording_template.replace(old, new)
    recording_template = recording_template.replace(
        '<div class="container">',
        '<div class="container" id="container">',
    )
    recording_template = recording_template.replace(
        "<script>",
        "<script>\n        {{BRIDGE_PRELUDE}}\n",
        1,
    )
    recording_template = recording_template.replace(
        "\n    </script>\n</body>",
        "\n        {{MOBILE_SHIM}}\n    </script>\n</body>",
    )
    icon_source = read(args.icons_source)
    recording_template = recording_template.replace(
        "{{ICON_PAUSE}}", extract_match_arm_raw_string(icon_source, "pause")
    )
    recording_template = recording_template.replace(
        "{{ICON_PLAY}}", extract_match_arm_raw_string(icon_source, "play_arrow")
    )
    recording_template = recording_template.replace(
        "{{ICON_CLOSE}}", extract_match_arm_raw_string(icon_source, "close")
    )
    write(output / "windows_recording_template.html", recording_template)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    for name in (
        "fit_source",
        "css_source",
        "button_css_source",
        "button_js_source",
        "button_theme_source",
        "grid_source",
        "recording_source",
        "icons_source",
        "static_assets",
        "output",
    ):
        parser.add_argument(f"--{name.replace('_', '-')}", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    generate(parse_args())
