"""Generates the notice window's textures in assets/gfx/.

Deliberately unlike the game's own dialogs, so players can tell the framework
is talking and not the game: flat near-black slate, a thin teal frame and a
teal stripe down the left edge, no parchment, no gold. The close button is a
teal cross in the same style.

usage: python tools/notice_textures.py
"""
import struct
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "assets" / "gfx"

WIDTH, HEIGHT = 440, 168
FILL = (0x14, 0x19, 0x1F)
HEADER = (0x18, 0x2C, 0x2E)
HEADER_HEIGHT = 30
TEAL = (0x2F, 0xC4, 0xB2)
FRAME, STRIPE = 2, 6
CLOSE = 22


def background(x: int, y: int) -> tuple[int, int, int]:
    if x < FRAME or y < FRAME or x >= WIDTH - FRAME or y >= HEIGHT - FRAME or x < STRIPE:
        return TEAL
    if y < HEADER_HEIGHT:
        return HEADER
    if y == HEADER_HEIGHT:
        return tuple(c // 2 for c in TEAL)
    return FILL


def close(x: int, y: int) -> tuple[int, int, int]:
    if x == 0 or y == 0 or x == CLOSE - 1 or y == CLOSE - 1:
        return TEAL
    inner = 6 <= x < CLOSE - 6 and 6 <= y < CLOSE - 6
    if inner and (abs(x - y) <= 1 or abs(x + y - (CLOSE - 1)) <= 1):
        return TEAL
    return HEADER


def write_dds(path: Path, width: int, height: int, pixel) -> None:
    body = bytearray()
    for y in range(height):
        for x in range(width):
            r, g, b = pixel(x, y)
            body += bytes((b, g, r, 0xFF))
    header = bytearray(128)
    header[0:4] = b"DDS "
    # size, flags (caps|height|width|pitch|pixelformat), height, width, pitch
    struct.pack_into("<IIIII", header, 4, 124, 0x100F, height, width, width * 4)
    # pixel format: size, RGB|alpha, no fourcc, 32 bits, R, G, B, A masks
    struct.pack_into("<II4sIIIII", header, 76, 32, 0x41, b"\0\0\0\0", 32, 0xFF0000, 0xFF00, 0xFF, 0xFF000000)
    struct.pack_into("<I", header, 108, 0x1000)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(header) + bytes(body))
    print(f"wrote {path} ({width}x{height})")


if __name__ == "__main__":
    write_dds(OUT / "notice_background.dds", WIDTH, HEIGHT, background)
    write_dds(OUT / "notice_close.dds", CLOSE, CLOSE, close)
