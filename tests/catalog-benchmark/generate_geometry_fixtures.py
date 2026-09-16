"""Generate synthetic compact OCR inputs with Pillow's bundled font."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


def main() -> None:
    root = Path(__file__).parent / "images" / "ocr"
    for name, dimensions, size, text in [
        ("compact-label", (192, 32), 14, "Ready to export"),
        ("thin-status", (256, 12), 10, "Queue 07 / Items 24"),
    ]:
        image = Image.new("RGB", dimensions, "white")
        draw = ImageDraw.Draw(image)
        font = ImageFont.load_default(size=size)
        bounds = draw.textbbox((0, 0), text, font=font)
        height = bounds[3] - bounds[1]
        draw.text((4, (dimensions[1] - height) // 2 - bounds[1]), text, fill="black", font=font)
        image.save(root / f"{name}.png")


if __name__ == "__main__":
    main()
