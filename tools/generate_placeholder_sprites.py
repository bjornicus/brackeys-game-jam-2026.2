#!/usr/bin/env python3
"""Generate deterministic placeholder character/enemy sprite sheets.

Uses only the Python standard library so it adds no runtime or Cargo dependency.
Run from the repository root:

    python3 tools/generate_placeholder_sprites.py
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

CELL = 64
PLAYER_COLUMNS = 4
PLAYER_ROWS = 12
ENEMY_COLUMNS = 4
ENEMY_ROWS = 4
OUT_DIR = Path("assets/sprites")

DIRECTIONS = [
    ("right", (230, 70, 70, 255), (1, 0)),
    ("left", (70, 150, 240, 255), (-1, 0)),
    ("up", (90, 210, 110, 255), (0, 1)),
    ("down", (235, 205, 70, 255), (0, -1)),
]


def write_png(path: Path, width: int, height: int, pixels: bytearray) -> None:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)  # no filter
        raw.extend(pixels[y * stride : (y + 1) * stride])

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), level=9))
        + chunk(b"IEND", b"")
    )


def make_canvas(width: int, height: int) -> bytearray:
    return bytearray([0, 0, 0, 0]) * width * height


def set_px(pixels: bytearray, width: int, height: int, x: int, y: int, color: tuple[int, int, int, int]) -> None:
    if 0 <= x < width and 0 <= y < height:
        i = (y * width + x) * 4
        pixels[i : i + 4] = bytes(color)


def rect(pixels, width, height, x0, y0, x1, y1, color):
    for y in range(max(0, y0), min(height, y1)):
        for x in range(max(0, x0), min(width, x1)):
            set_px(pixels, width, height, x, y, color)


def ellipse(pixels, width, height, cx, cy, rx, ry, color):
    for y in range(cy - ry, cy + ry + 1):
        for x in range(cx - rx, cx + rx + 1):
            if ((x - cx) ** 2) * (ry**2) + ((y - cy) ** 2) * (rx**2) <= (rx**2) * (ry**2):
                set_px(pixels, width, height, x, y, color)


def arrow(pixels, width, height, ox, oy, dx, dy, color):
    cx = ox + CELL // 2
    cy = oy + CELL // 2
    if dx:
        y0, y1 = cy - 4, cy + 5
        x0, x1 = (cx, ox + 50) if dx > 0 else (ox + 14, cx)
        rect(pixels, width, height, x0, y0, x1, y1, color)
        tip_x = ox + (54 if dx > 0 else 10)
        for n in range(10):
            x = tip_x - n if dx > 0 else tip_x + n
            rect(pixels, width, height, x, cy - n, x + 1, cy + n + 1, color)
    else:
        x0, x1 = cx - 4, cx + 5
        y0, y1 = (oy + 14, cy) if dy > 0 else (cy, oy + 50)
        rect(pixels, width, height, x0, y0, x1, y1, color)
        tip_y = oy + (10 if dy > 0 else 54)
        for n in range(10):
            y = tip_y + n if dy > 0 else tip_y - n
            rect(pixels, width, height, cx - n, y, cx + n + 1, y + 1, color)


def player_sheet() -> bytearray:
    w, h = PLAYER_COLUMNS * CELL, PLAYER_ROWS * CELL
    p = make_canvas(w, h)
    for d, (_name, color, vec) in enumerate(DIRECTIONS):
        for state in range(3):
            row = d * 3 + state
            for frame in range(PLAYER_COLUMNS):
                ox, oy = frame * CELL, row * CELL
                rect(p, w, h, ox + 18, oy + 22, ox + 46, oy + 56, color)
                ellipse(p, w, h, ox + 32, oy + 18, 12, 12, (245, 220, 185, 255))
                bob = [0, 2, 0, -2][frame]
                if state == 1:  # move: animated feet
                    rect(p, w, h, ox + 18, oy + 54 + bob, ox + 28, oy + 61 + bob, (40, 40, 55, 255))
                    rect(p, w, h, ox + 36, oy + 54 - bob, ox + 46, oy + 61 - bob, (40, 40, 55, 255))
                if state == 2:  # shoot: larger weapon flash in facing direction
                    arrow(p, w, h, ox, oy, vec[0], vec[1], (255, 255, 255, 255))
                    ellipse(p, w, h, ox + 32 + vec[0] * 20, oy + 32 - vec[1] * 20, 5 + frame, 5 + frame, (150, 230, 255, 220))
                else:
                    arrow(p, w, h, ox, oy, vec[0], vec[1], (25, 25, 35, 255))
    return p


def enemy_sheet() -> bytearray:
    w, h = ENEMY_COLUMNS * CELL, ENEMY_ROWS * CELL
    p = make_canvas(w, h)
    colors = [
        (170, 70, 210, 255),  # idle
        (190, 80, 220, 255),  # move
        (80, 190, 235, 255),  # stunned
        (80, 80, 95, 255),  # death
    ]
    for row in range(ENEMY_ROWS):
        for frame in range(ENEMY_COLUMNS):
            ox, oy = frame * CELL, row * CELL
            squash = frame if row == 1 else 0
            if row == 3:
                ellipse(p, w, h, ox + 32, oy + 44 + frame * 2, 22, max(4, 14 - frame * 3), colors[row])
            else:
                ellipse(p, w, h, ox + 32, oy + 34 - squash, 22, 18 + squash, colors[row])
                ellipse(p, w, h, ox + 22, oy + 28, 4, 5, (250, 250, 250, 255))
                ellipse(p, w, h, ox + 42, oy + 28, 4, 5, (250, 250, 250, 255))
                if row == 2:
                    for ring in range(frame + 1):
                        ellipse(p, w, h, ox + 32, oy + 32, 26 + ring * 4, 22 + ring * 3, (120, 220, 255, 70))
    return p


if __name__ == "__main__":
    write_png(OUT_DIR / "character_placeholder.png", PLAYER_COLUMNS * CELL, PLAYER_ROWS * CELL, player_sheet())
    write_png(OUT_DIR / "enemy_placeholder.png", ENEMY_COLUMNS * CELL, ENEMY_ROWS * CELL, enemy_sheet())
    print("Generated assets/sprites/character_placeholder.png and enemy_placeholder.png")
