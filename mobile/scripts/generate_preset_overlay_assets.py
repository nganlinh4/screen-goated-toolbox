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


def extract_match_arm_asset(source_file: Path, arm_name: str) -> str:
    """Read an icon arm by snake_case name: `"close" =>` or enum `Self::Close =>`."""
    source = read(source_file)
    variant = "".join(part.capitalize() for part in arm_name.split("_"))
    arm = rf'(?:"{re.escape(arm_name)}"|Self::{re.escape(variant)})\s*=>\s*'
    match = re.search(arm + r'\{\s*r(#+)"(.*?)"\1\s*\}', source, re.DOTALL)
    if match is not None:
        return match.group(2)
    include = re.search(arm + r'include_str!\("([^"]+)"\)', source)
    if include is not None:
        return read(source_file.parent / include.group(1))
    raise ValueError(f"Missing string asset match arm: {arm_name}")


def generate_text_input_assets(styles: Path, icons: Path, output: Path) -> None:
    from preset_overlay_text_input import generate as generate_text_input

    generate_text_input(styles, output, extract_raw_string,
                        lambda name: extract_match_arm_asset(icons, name), write)


def generate(args: argparse.Namespace) -> None:
    output = args.output / "preset_overlay"
    output.mkdir(parents=True, exist_ok=True)
    if getattr(args, "text_input_source", None):
        generate_text_input_assets(args.text_input_source, args.icons_source, output)
    selection_source = getattr(args, "selection_source", None)
    if selection_source:
        selection = extract_raw_string(read(selection_source), "pub fn get_html(")
        selection = selection.replace("{{", "{").replace("}}", "}")
        selection = selection.replace("{font_css}", "{{FONT_CSS}}").replace("{text}", "{{TEXT}}")
        write(output / "windows_selection_badge.html", selection)

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
    lib_js = grid_source.split("pub fn get_lib_js()", 1)[1].split("pub fn get_lib_css()", 1)[0]
    js_includes = re.findall(r'include_str!\("([^"]+)"\)', lib_js)
    css_includes = re.findall(
        r'include_str!\("([^"]+)"\)', grid_source.split("pub fn get_lib_css()", 1)[1]
    )
    if len(js_includes) != 3 or len(css_includes) != 1:
        raise ValueError("Unexpected canonical Grid.js library definition")
    licenses = "\n".join(read(args.grid_source.parent / path) for path in js_includes[:2])
    js = read(args.grid_source.parent / js_includes[2])
    css = read(args.grid_source.parent / css_includes[0])
    write(output / "windows_gridjs_lib.js", f"/*\n{licenses}\n*/\n{js}")
    write(output / "windows_gridjs_lib.css", css)
    write(
        output / "windows_result_dom_patch.js",
        read(args.grid_source.parent / "../result/scene_compositor/dom_patch_runtime.js"),
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
    recording_template = recording_template.replace(
        "{{ICON_PAUSE}}", extract_match_arm_asset(args.icons_source, "pause")
    )
    recording_template = recording_template.replace(
        "{{ICON_PLAY}}", extract_match_arm_asset(args.icons_source, "play_arrow")
    )
    recording_template = recording_template.replace(
        "{{ICON_CLOSE}}", extract_match_arm_asset(args.icons_source, "close")
    )
    write(output / "windows_recording_template.html", recording_template)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--selection-source", type=Path)
    parser.add_argument("--text-input-source", type=Path)
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
