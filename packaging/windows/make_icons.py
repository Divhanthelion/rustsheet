"""Render the RustSheet icon and every size the exe, window and MSIX need.

Usage: python packaging/windows/make_icons.py   (needs Pillow)

Writes:
  assets/icon-256.png         window icon (embedded by src/gui/app.rs)
  assets/icon-1024.png        master, for the store listing and README
  assets/rustsheet.ico        exe icon (embedded by build.rs)
  packaging/windows/Assets/   MSIX logos (referenced by AppxManifest.xml)
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
MSIX_ASSETS = ROOT / "packaging" / "windows" / "Assets"

GREEN_TOP = (46, 125, 70)
GREEN_BOTTOM = (24, 78, 44)
SHEET = (255, 255, 255)
HEADER = (214, 236, 218)
LINE = (190, 214, 196)
RUST = (217, 83, 43)

SUPERSAMPLE = 4


def draw_icon(size: int) -> Image.Image:
    """Draw the icon at `size` px. Small sizes use a coarser grid so it stays legible."""
    s = size * SUPERSAMPLE
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    # Background: rounded square with a vertical green gradient.
    gradient = Image.new("RGBA", (s, s))
    gd = ImageDraw.Draw(gradient)
    for y in range(s):
        t = y / (s - 1)
        gd.line(
            [(0, y), (s, y)],
            fill=tuple(round(a + (b - a) * t) for a, b in zip(GREEN_TOP, GREEN_BOTTOM)),
        )
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, s - 1, s - 1], radius=s * 0.22, fill=255)
    img.paste(gradient, (0, 0), mask)

    d = ImageDraw.Draw(img)
    small = size <= 32
    cols, rows = (2, 3) if small else (3, 4)

    # The sheet: a white card inset from the edge.
    inset = s * (0.17 if small else 0.18)
    x0, y0, x1, y1 = inset, inset, s - inset, s - inset
    d.rounded_rectangle([x0, y0, x1, y1], radius=s * 0.06, fill=SHEET)

    cell_w = (x1 - x0) / cols
    cell_h = (y1 - y0) / rows
    line_w = max(SUPERSAMPLE, round(s * (0.035 if small else 0.018)))

    # Header row, clipped to the card's rounded top corners.
    header = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(header).rectangle([x0, y0, x1, y0 + cell_h], fill=HEADER)
    card_mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(card_mask).rounded_rectangle([x0, y0, x1, y1], radius=s * 0.06, fill=255)
    img.paste(header, (0, 0), Image.composite(header, Image.new("RGBA", (s, s)), card_mask))

    # The selected cell, in rust.
    sel_col, sel_row = (1, 1) if small else (1, 2)
    d.rectangle(
        [
            x0 + cell_w * sel_col,
            y0 + cell_h * sel_row,
            x0 + cell_w * (sel_col + 1),
            y0 + cell_h * (sel_row + 1),
        ],
        fill=RUST,
    )

    for c in range(1, cols):
        x = x0 + cell_w * c
        d.line([(x, y0), (x, y1)], fill=LINE, width=line_w)
    for r in range(1, rows):
        y = y0 + cell_h * r
        d.line([(x0, y), (x1, y)], fill=LINE, width=line_w)

    return img.resize((size, size), Image.LANCZOS)


def on_canvas(width: int, height: int, icon_size: int) -> Image.Image:
    """Center the icon on a transparent canvas (tile logos)."""
    canvas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    icon = draw_icon(icon_size)
    canvas.paste(icon, ((width - icon_size) // 2, (height - icon_size) // 2), icon)
    return canvas


def main() -> None:
    ASSETS.mkdir(exist_ok=True)
    MSIX_ASSETS.mkdir(parents=True, exist_ok=True)

    draw_icon(1024).save(ASSETS / "icon-1024.png")
    draw_icon(256).save(ASSETS / "icon-256.png")

    ico_sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [draw_icon(n) for n in ico_sizes]
    frames[-1].save(
        ASSETS / "rustsheet.ico",
        sizes=[(n, n) for n in ico_sizes],
        append_images=frames[:-1],
    )

    # Square44x44Logo: app list, taskbar, Start. Plated and unplated target sizes.
    for scale in (100, 200):
        n = 44 * scale // 100
        draw_icon(n).save(MSIX_ASSETS / f"Square44x44Logo.scale-{scale}.png")
    for n in (16, 24, 32, 48, 256):
        icon = draw_icon(n)
        icon.save(MSIX_ASSETS / f"Square44x44Logo.targetsize-{n}.png")
        icon.save(MSIX_ASSETS / f"Square44x44Logo.targetsize-{n}_altform-unplated.png")

    for scale in (100, 200):
        k = scale / 100
        on_canvas(round(150 * k), round(150 * k), round(100 * k)).save(
            MSIX_ASSETS / f"Square150x150Logo.scale-{scale}.png"
        )
        on_canvas(round(310 * k), round(150 * k), round(100 * k)).save(
            MSIX_ASSETS / f"Wide310x150Logo.scale-{scale}.png"
        )
        draw_icon(round(50 * k)).save(MSIX_ASSETS / f"StoreLogo.scale-{scale}.png")

    print(f"Wrote icons to {ASSETS} and {MSIX_ASSETS}")


if __name__ == "__main__":
    main()
