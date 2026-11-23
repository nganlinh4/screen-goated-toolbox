"""Port the canonical Windows text-input document and edit operations."""

import json
import re
from pathlib import Path


def generate(source: Path, output: Path, raw_string, icon_asset, write) -> None:
    body = source.read_text(encoding="utf-8")
    css = body.split("pub fn get_editor_css", 1)[1].split("pub fn get_editor_html", 1)[0]
    dark = raw_string(css, "if is_dark {")
    light = raw_string(css, "} else {")
    base = raw_string(css, "format!(").replace("{{", "{").replace("}}", "}")
    write(output / "windows_text_input_dark.css", base.replace("{vars}", dark))
    write(output / "windows_text_input_light.css", base.replace("{vars}", light))
    template = raw_string(body, "pub fn get_editor_html")
    template = template.replace("{{", "{").replace("}}", "}")
    tokens = ["theme_attr", "font_css", "css", "title_text", "placeholder", "submit_txt",
              "newline_txt", "cancel_hint", "cancel_txt", "close_title", "mic_title", "send_title"]
    for token in tokens:
        template = template.replace("{" + token + "}", "{{" + token.upper() + "}}")
    for token, name in [("close_svg", "close"), ("mic_svg", "mic"), ("send_svg", "send")]:
        template = template.replace("{" + token + "}", icon_asset(name))
    template = template.replace("<script>", "<script>\n{{BRIDGE_PRELUDE}}\n", 1)
    template = template.replace("</script>", "{{EDIT_OPERATIONS}}\n{{MOBILE_SHIM}}\n</script>", 1)
    write(output / "windows_text_input.html", template)
    operations = []
    for filename, marker, name in [("activation.rs", "fn tick(", "activate"),
                                    ("editor_text.rs", "fn apply_text(", "apply")]:
        text = (source.parent / filename).read_text(encoding="utf-8")
        script = raw_string(text, marker).replace("{payload}", "payload")
        script = script.replace("{{", "{").replace("}}", "}")
        operations.append(f"window.sgtText_{name} = function(payload) {{ return {script}; }};")
    write(output / "windows_text_input_edits.js", "\n".join(operations))
    locale_root = source.parents[2] / "gui/locale"
    fields = {"preset_editor.rs": ["text_input_footer_submit", "text_input_footer_newline", "text_input_footer_cancel"],
              "overlay.rs": ["text_input_close_tooltip", "text_input_speech_to_text_tooltip", "text_input_send_tooltip"]}
    locales = {}
    for directory in sorted(locale_root.iterdir()):
        if not directory.is_dir() or not all((directory / name).is_file() for name in fields):
            continue
        values = {}
        for name, keys in fields.items():
            text = (directory / name).read_text(encoding="utf-8")
            for key in keys:
                matches = re.findall(rf'\b{key}:\s*("(?:[^"\\]|\\.)*")\s*,', text)
                if len(matches) != 1:
                    raise ValueError(f"Expected one canonical locale field {directory.name}/{key}")
                values[key] = json.loads(matches[0])
        locales[directory.name] = values
    if "en" not in locales:
        raise ValueError("Missing canonical fallback locale")
    write(output / "windows_text_input_locales.json", json.dumps(locales,ensure_ascii=False,indent=2)+"\n")
