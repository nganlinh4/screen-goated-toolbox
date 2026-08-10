"""Build deterministic Google Sans Flex assets for every SGT UI surface."""

from pathlib import Path

from fontTools import __version__ as fonttools_version
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "assets" / "GoogleSansFlex-VariableFont_GRAD,ROND,opsz,slnt,wdth,wght.ttf"
VARIABLE_OUTPUT = ROOT / "assets" / "GoogleSansFlex-VariableFont.woff"
REGULAR_OUTPUT = ROOT / "assets" / "GoogleSansFlex-Regular.woff"
BOLD_OUTPUT = ROOT / "assets" / "GoogleSansFlex-Bold.woff"
UI_VARIABLE_OUTPUT = ROOT / "assets" / "GoogleSansFlex-UI.woff"
SHELL_OUTPUT = ROOT / "assets" / "GoogleSansFlex-Shell.ttf"
ANDROID_COMPOSE_OUTPUT = (
    ROOT / "mobile" / "androidApp" / "src" / "main" / "res" / "font" / "google_sans_flex.ttf"
)
ANDROID_WEB_OUTPUT = (
    ROOT / "mobile" / "androidApp" / "src" / "main" / "assets" / "GoogleSansFlex.woff"
)
PINNED_FONTTOOLS = "4.61.1"
# Result fitting animates only width and weight. Pin the remaining axes to the
# product's stable delivery values before WOFF compression instead of shipping
# variation data that no rendered card needs to select.
PRESERVED_AXES = ("wdth", "wght")
UI_PRESERVED_AXES = ("wdth", "wght", "GRAD", "ROND")


def source_font() -> TTFont:
    return TTFont(SOURCE, recalcTimestamp=False)


def save_woff(font: TTFont, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    font.flavor = "woff"
    font.save(output, reorderTables=True)
    print(f"Wrote {output.relative_to(ROOT)} ({output.stat().st_size} bytes)")


def save_ttf(font: TTFont, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    font.flavor = None
    font.save(output, reorderTables=True)
    print(f"Wrote {output.relative_to(ROOT)} ({output.stat().st_size} bytes)")


def build_variable() -> None:
    font = source_font()
    instantiateVariableFont(
        font,
        {"opsz": 18, "ROND": 100, "slnt": 0, "GRAD": 0},
        inplace=True,
        optimize=True,
    )
    actual_axes = tuple(axis.axisTag for axis in font["fvar"].axes)
    if actual_axes != PRESERVED_AXES:
        raise SystemExit(f"unexpected output axes: {actual_axes}")
    save_woff(font, VARIABLE_OUTPUT)


def build_static(weight: int, output: Path) -> None:
    font = source_font()
    instantiateVariableFont(
        font,
        {"opsz": 18, "wdth": 90, "wght": weight, "GRAD": 0, "ROND": 100, "slnt": 0},
        inplace=True,
        optimize=True,
    )
    if "fvar" in font:
        raise SystemExit(f"static {weight} font retained variation axes")
    save_woff(font, output)


def build_product_ui() -> None:
    font = source_font()
    instantiateVariableFont(
        font,
        {"opsz": 18, "slnt": 0},
        inplace=True,
        optimize=True,
    )
    actual_axes = tuple(axis.axisTag for axis in font["fvar"].axes)
    if actual_axes != UI_PRESERVED_AXES:
        raise SystemExit(f"unexpected product UI axes: {actual_axes}")
    save_ttf(font, ANDROID_COMPOSE_OUTPUT)
    save_woff(font, UI_VARIABLE_OUTPUT)
    ANDROID_WEB_OUTPUT.write_bytes(UI_VARIABLE_OUTPUT.read_bytes())
    print(
        f"Wrote {ANDROID_WEB_OUTPUT.relative_to(ROOT)} "
        f"({ANDROID_WEB_OUTPUT.stat().st_size} bytes)"
    )


def build_shell() -> None:
    font = source_font()
    instantiateVariableFont(
        font,
        {"opsz": 18, "wdth": 100, "wght": 400, "GRAD": 0, "ROND": 0, "slnt": 0},
        inplace=True,
        optimize=True,
    )
    if "fvar" in font:
        raise SystemExit("shell font retained variation axes")
    save_ttf(font, SHELL_OUTPUT)


def main() -> None:
    if fonttools_version != PINNED_FONTTOOLS:
        raise SystemExit(
            f"fonttools {PINNED_FONTTOOLS} is required; found {fonttools_version}"
        )
    build_variable()
    build_static(400, REGULAR_OUTPUT)
    build_static(700, BOLD_OUTPUT)
    build_product_ui()
    build_shell()


if __name__ == "__main__":
    main()
