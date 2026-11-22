"""Nova Commander's icon: a two-panel window in FAR's colours, drawn as pixel art.

python -B assets/icon/draw_icon.py

Writes nova-commander.ico (16, 24, 32, 48, 64, 128, 256; PNG entries) and nova-commander.png (256) beside
this script. The 128 picture is drawn pixel by pixel with its own font; 256 is it doubled; the small ones
are drawn on their own, simpler, so that they stay readable.
"""

import io
import pathlib
import struct

from PIL import Image, ImageDraw

HERE = pathlib.Path(__file__).resolve().parent

CLEAR = (0, 0, 0, 0)
BLACK = (0, 0, 0, 255)
NAVY = (16, 16, 150, 255)
CYAN = (40, 215, 240, 255)
LIGHT = (150, 240, 255, 255)
YELLOW = (255, 226, 0, 255)
WHITE = (240, 240, 240, 255)
GRAY = (200, 200, 200, 255)
DARK = (110, 110, 110, 255)

# A narrow pixel font: each glyph its rows, '#' a pixel.
FONT = {
    "A": [".##.", "#..#", "#..#", "####", "#..#", "#..#", "#..#"],
    "B": ["###.", "#..#", "#..#", "###.", "#..#", "#..#", "###."],
    "C": [".##.", "#..#", "#...", "#...", "#...", "#..#", ".##."],
    "D": ["###.", "#..#", "#..#", "#..#", "#..#", "#..#", "###."],
    "E": ["####", "#...", "#...", "###.", "#...", "#...", "####"],
    "F": ["####", "#...", "#...", "###.", "#...", "#...", "#..."],
    "I": ["###", ".#.", ".#.", ".#.", ".#.", ".#.", "###"],
    "K": ["#..#", "#.#.", "##..", "##..", "#.#.", "#..#", "#..#"],
    "L": ["#...", "#...", "#...", "#...", "#...", "#...", "####"],
    "M": ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"],
    "N": ["#..#", "##.#", "##.#", "#.##", "#.##", "#..#", "#..#"],
    "O": [".##.", "#..#", "#..#", "#..#", "#..#", "#..#", ".##."],
    "P": ["###.", "#..#", "#..#", "###.", "#...", "#...", "#..."],
    "R": ["###.", "#..#", "#..#", "###.", "#.#.", "#..#", "#..#"],
    "S": [".###", "#...", "#...", ".##.", "...#", "...#", "###."],
    "T": ["###", ".#.", ".#.", ".#.", ".#.", ".#.", ".#."],
    "U": ["#..#", "#..#", "#..#", "#..#", "#..#", "#..#", ".##."],
    "W": ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"],
    "X": ["#..#", "#..#", ".##.", ".##.", ".##.", "#..#", "#..#"],
    ":": [".", "#", ".", ".", ".", "#", "."],
    ".": [".", ".", ".", ".", ".", ".", "#"],
    "\\": ["#...", "#...", ".#..", ".#..", "..#.", "..#.", "...#"],
}

# The key bar's tiny font.
TINY = {
    "F": ["###", "#..", "##.", "#..", "#.."],
    "1": [".#.", "##.", ".#.", ".#.", "###"],
    "2": ["##.", "..#", ".#.", "#..", "###"],
    "3": ["##.", "..#", ".#.", "..#", "##."],
    "4": ["#.#", "#.#", "###", "..#", "..#"],
    "5": ["###", "#..", "##.", "..#", "##."],
    "6": [".##", "#..", "###", "#.#", "###"],
    "7": ["###", "..#", ".#.", ".#.", ".#."],
    "8": ["###", "#.#", "###", "#.#", "###"],
}

# The keys' signs, 5 wide and 5 high.
SIGNS = [
    [".....", ".....", "#####", ".....", "....."],  # F1
    ["###..", "#.###", "#.#.#", "###.#", "..###"],  # F2: copy
    [".###.", "#####", "#.#.#", "#.#.#", "#####"],  # F3: delete
    ["...##", "..###", ".###.", "###..", "##..."],  # F4: edit
    [".#.#.", "##.##", "#####", "##.##", ".#.#."],  # F5: move both ways
    ["..#..", "..#..", "#####", "..#..", "..#.."],  # F6: plus
    ["##...", "#####", "#####", "#####", "#####"],  # F7: folder
    ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"],  # F8: cross
]


def glyphs(draw: ImageDraw.ImageDraw, x: int, y: int, text: str, font: dict, colour) -> int:
    for ch in text:
        rows = font[ch]
        for dy, row in enumerate(rows):
            for dx, cell in enumerate(row):
                if cell == "#":
                    draw.point((x + dx, y + dy), fill=colour)
        x += len(rows[0]) + 1
    return x


def pattern(draw: ImageDraw.ImageDraw, x: int, y: int, rows: list[str], colour) -> None:
    for dy, row in enumerate(rows):
        for dx, cell in enumerate(row):
            if cell == "#":
                draw.point((x + dx, y + dy), fill=colour)


def folder(draw: ImageDraw.ImageDraw, x: int, y: int, hollow: bool) -> None:
    """An 8 x 7 folder: a tab on its left, a black outline."""
    draw.rectangle((x, y, x + 3, y + 1), fill=BLACK)
    draw.rectangle((x, y + 1, x + 7, y + 6), fill=BLACK)
    if not hollow:
        draw.rectangle((x + 1, y + 1, x + 2, y + 1), fill=YELLOW)
        draw.rectangle((x + 1, y + 2, x + 6, y + 5), fill=YELLOW)
    else:
        draw.rectangle((x + 1, y + 2, x + 6, y + 5), fill=YELLOW)
        draw.rectangle((x + 1, y + 1, x + 2, y + 1), fill=YELLOW)


def document(draw: ImageDraw.ImageDraw, x: int, y: int) -> None:
    """A 6 x 8 page with its corner folded and three lines."""
    draw.rectangle((x, y, x + 5, y + 7), fill=WHITE)
    draw.point((x + 5, y), fill=NAVY)
    draw.point((x + 4, y), fill=GRAY)
    draw.point((x + 5, y + 1), fill=GRAY)
    for line in (y + 2, y + 4, y + 6):
        draw.line((x + 1, line, x + 4, line), fill=NAVY)


def button(draw: ImageDraw.ImageDraw, x: int, y: int, kind: str) -> None:
    """A window button 11 x 9 with its bevel and its sign."""
    draw.rectangle((x, y, x + 10, y + 8), fill=GRAY)
    draw.line((x, y, x + 10, y), fill=WHITE)
    draw.line((x, y, x, y + 8), fill=WHITE)
    draw.line((x, y + 8, x + 10, y + 8), fill=DARK)
    draw.line((x + 10, y, x + 10, y + 8), fill=DARK)
    if kind == "min":
        draw.rectangle((x + 3, y + 5, x + 7, y + 6), fill=BLACK)
    elif kind == "max":
        draw.rectangle((x + 3, y + 2, x + 7, y + 6), outline=BLACK)
        draw.line((x + 3, y + 3, x + 7, y + 3), fill=BLACK)
    else:
        draw.line((x + 3, y + 2, x + 7, y + 6), fill=BLACK)
        draw.line((x + 3, y + 6, x + 7, y + 2), fill=BLACK)


def window(draw: ImageDraw.ImageDraw, n: int) -> None:
    """The window's outline, bevel, cyan frame and blue inside for a picture of `n` pixels."""
    t = max(1, n // 48)
    draw.rectangle((0, 0, n - 1, n - 1), fill=BLACK)
    draw.rectangle((1, 1, n - 2, n - 2), fill=LIGHT)
    draw.rectangle((1 + t, 1 + t, n - 2, n - 2), fill=CYAN)
    draw.rectangle((1 + 2 * t, 1 + 2 * t, n - 2 - t, n - 2 - t), fill=NAVY)


def big() -> Image.Image:
    n = 128
    image = Image.new("RGBA", (n, n), CLEAR)
    draw = ImageDraw.Draw(image)
    window(draw, n)
    # The title bar: the program's sign, the buttons, a light line under them.
    draw.rectangle((7, 7, 21, 16), fill=BLACK)
    draw.rectangle((8, 8, 20, 15), fill=YELLOW)
    draw.line((8, 10, 20, 10), fill=BLACK)
    draw.line((13, 8, 13, 15), fill=BLACK)
    draw.line((15, 13, 18, 13), fill=BLACK)
    for x, kind in ((86, "min"), (98, "max"), (110, "close")):
        button(draw, x, 7, kind)
    draw.line((5, 19, 122, 19), fill=GRAY)
    # The panels with their headers.
    for left, title in ((7, "A:\\"), (65, "B:\\")):
        draw.rectangle((left, 25, left + 55, 104), outline=CYAN, width=2)
        middle = left + 28
        draw.rectangle((middle - 11, 21, middle + 11, 29), fill=CYAN)
        glyphs(draw, middle - 5, 22, title, FONT, NAVY)
    rows = [32, 43, 54, 65, 76, 87]
    left_items = ["BIN\\", "DOC\\", "UTIL\\", "DATA\\", "READ.ME", "LIST.TXT"]
    right_items = ["WORK\\", "MEDIA\\", "TEMP\\", "BACKUP\\", "TODO.TXT", "INFO.TXT"]
    for icon_x, text_x, items in ((11, 21, left_items), (69, 79, right_items)):
        for row, (y, name) in enumerate(zip(rows, items)):
            cursor = items is left_items and row == 2
            if cursor:
                draw.rectangle((icon_x - 2, y - 2, icon_x + 47, y + 8), fill=YELLOW)
            if name.endswith("\\"):
                folder(draw, icon_x, y, cursor)
            else:
                document(draw, icon_x + 1, y - 1)
            glyphs(draw, text_x, y, name, FONT, NAVY if cursor else WHITE)
    # The key bar under a line, its halves parted.
    draw.line((5, 107, 122, 107), fill=CYAN)
    for key in range(8):
        x = 7 + key * 14 + (2 if key >= 4 else 0)
        draw.rectangle((x, 110, x + 8, 117), fill=GRAY)
        glyphs(draw, x + 1, 111, f"F{key + 1}", TINY, BLACK)
        pattern(draw, x + 10, 111, SIGNS[key], YELLOW)
    draw.line((64, 109, 64, 118), fill=CYAN)
    draw.rectangle((5, 120, 122, 122), fill=CYAN)
    return image


# 16 pixels drawn by hand: too few for the drawn layout.
TINY_ICON = [
    "KKKKKKKKKKKKKKKK",
    "KCCCCCCCCCCCCCCK",
    "KCYYNNNNNNNGGGCK",
    "KCCCCCCCCCCCCCCK",
    "KCNNNNNNCNNNNNCK",
    "KCYNWWWNCYNWWNCK",
    "KCNNNNNNCNNNNNCK",
    "KCYYYYYYCYNWWWCK",
    "KCNNNNNNCNNNNNCK",
    "KCWNWWWNCWNWWNCK",
    "KCNNNNNNCNNNNNCK",
    "KCWNWWNNCWNWWWCK",
    "KCNNNNNNCNNNNNCK",
    "KCGYGYGYGYGYGYCK",
    "KCCCCCCCCCCCCCCK",
    "KKKKKKKKKKKKKKKK",
]
TINY_COLOURS = {"K": BLACK, "C": CYAN, "N": NAVY, "Y": YELLOW, "W": WHITE, "G": GRAY}


def tiny() -> Image.Image:
    image = Image.new("RGBA", (16, 16), CLEAR)
    for y, row in enumerate(TINY_ICON):
        for x, cell in enumerate(row):
            image.putpixel((x, y), TINY_COLOURS[cell])
    return image


def small(n: int) -> Image.Image:
    """The window at `n` pixels without text: rows as bars, the cursor a yellow bar."""
    image = Image.new("RGBA", (n, n), CLEAR)
    draw = ImageDraw.Draw(image)
    window(draw, n)
    t = max(1, n // 48)
    inner = 1 + 2 * t
    last = n - 2 - t
    title = max(3, n // 8)
    # The program's sign and, when there is room, the buttons.
    sign = max(2, n // 10)
    draw.rectangle((inner + 1, inner + 1, inner + sign + sign // 2, inner + sign), fill=YELLOW)
    if n >= 32:
        size = max(3, n // 14)
        for i in range(3):
            x = last - 1 - (i + 1) * (size + 1) + 1
            draw.rectangle((x, inner + 1, x + size - 1, inner + size - 1), fill=GRAY)
        draw.line((inner, inner + title, last, inner + title), fill=GRAY)
    top = inner + title + (2 if n >= 32 else 1)
    keys = max(2, n // 10) if n >= 24 else 0
    bottom = last - keys - (1 if n >= 24 else 0)
    middle = n // 2
    for left, right in ((inner + 1, middle - 1), (middle, last - 1)):
        draw.rectangle((left, top, right, bottom), outline=CYAN)
    step = max(2, (bottom - top - 2) // 6)
    lengths = [0.5, 0.55, 0.6, 0.6, 0.7, 0.75]
    for panel, (left, right) in enumerate(((inner + 1, middle - 1), (middle, last - 1))):
        width = right - left - 3
        for row in range(min(6, (bottom - top - 2) // step)):
            y = top + 2 + row * step
            if y > bottom - 2:
                break
            length = max(1, int(width * lengths[(row + panel) % 6]))
            if panel == 0 and row == 2:
                draw.rectangle((left + 1, y, right - 1, y + max(0, step - 2)), fill=YELLOW)
                continue
            colour = YELLOW if row < 4 else WHITE
            thick = 1 if step >= 4 else 0
            draw.rectangle((left + 2, y, left + 2 + thick, y + thick), fill=colour)
            draw.rectangle((left + 4 + thick, y, left + 2 + length, y + thick), fill=WHITE)
    if keys:
        y = last - keys
        slots = 8 if n >= 32 else 4
        width = (last - inner - 1) / slots
        for key in range(slots):
            x = int(inner + 1 + key * width)
            draw.rectangle((x, y, x + max(1, int(width) // 2 - 1), last - 1), fill=GRAY)
            sign = (x + int(width) // 2, y + 1, x + int(width) - 2, last - 2)
            if sign[2] >= sign[0] and sign[3] >= sign[1]:
                draw.rectangle(sign, fill=YELLOW)
    return image


def ico(images: list[Image.Image], path: pathlib.Path) -> None:
    """An .ico of PNG entries, the sizes as given."""
    blobs = []
    for image in images:
        buffer = io.BytesIO()
        image.save(buffer, "PNG", optimize=True)
        blobs.append(buffer.getvalue())
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    for image, blob in zip(images, blobs):
        side = image.width if image.width < 256 else 0
        entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    path.write_bytes(header + entries + b"".join(blobs))


def main() -> None:
    picture = big()
    doubled = picture.resize((256, 256), Image.NEAREST)
    sizes = [tiny()] + [small(n) for n in (24, 32, 48, 64)]
    ico(sizes + [picture, doubled], HERE / "nova-commander.ico")
    doubled.save(HERE / "nova-commander.png", optimize=True)
    # A sheet of every size for looking at them (not shipped).
    sheet = Image.new("RGBA", (256 + 128 + 64 * 2 + 48 * 2 + 32 * 2 + 40, 260), (60, 60, 60, 255))
    x = 4
    for image in [doubled, picture] + [s.resize((s.width * 2, s.width * 2), Image.NEAREST) for s in reversed(sizes)]:
        sheet.paste(image, (x, 2), image)
        x += image.width + 4
    sheet.save(HERE.parents[1] / "tmp" / "icon_sheet.png")


if __name__ == "__main__":
    main()
