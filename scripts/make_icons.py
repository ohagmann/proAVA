#!/usr/bin/env python3
"""Erzeugt aus der Geometrie von assets/icons/proava.svg die PNGs (16–512) und
proava.ico. Nur Pillow nötig (kein SVG-Renderer): Die Formen sind hier
nachgezeichnet – bei Änderungen am SVG beide gleich halten.

    python3 scripts/make_icons.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icons"
BLUE = (0x25, 0x63, 0xC9, 255)
SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
SUPER = 8  # Kantenglättung: groß zeichnen, dann verkleinern


def draw(size: int) -> Image.Image:
    big = size * SUPER
    k = big / 256
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = lambda v: v * k  # noqa: E731
    d.rounded_rectangle([s(8), s(8), s(248), s(248)], radius=s(52), fill=BLUE)
    outer = [(112, 40), (144, 40), (196, 168), (164, 168), (154, 142), (102, 142), (92, 168), (60, 168)]
    d.polygon([(s(x), s(y)) for x, y in outer], fill=(255, 255, 255, 255))
    counter = [(128, 74), (112, 116), (144, 116)]
    d.polygon([(s(x), s(y)) for x, y in counter], fill=BLUE)
    if size >= 32:  # LV-Zeilen nur, wo sie noch erkennbar sind
        d.rounded_rectangle([s(60), s(186), s(196), s(198)], radius=s(6), fill=(255, 255, 255, 217))
        d.rounded_rectangle([s(60), s(208), s(156), s(220)], radius=s(6), fill=(255, 255, 255, 153))
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    images = {}
    for size in SIZES:
        images[size] = draw(size)
        images[size].save(OUT / f"proava-{size}.png", optimize=True)
    images[256].save(
        OUT / "proava.ico",
        sizes=[(n, n) for n in [16, 24, 32, 48, 64, 128, 256]],
        append_images=[images[n] for n in [16, 24, 32, 48, 64, 128]],
    )
    print("geschrieben:", ", ".join(f"proava-{n}.png" for n in SIZES), "proava.ico")


if __name__ == "__main__":
    main()
