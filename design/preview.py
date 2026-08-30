# -*- coding: utf-8 -*-
"""Контрольный лист: знак в реальных размерах иконки, увеличенный для осмотра.

Каждый размер считается честно — с супердискретизацией и усреднением, как это
делает системный масштабатор, — и только потом растягивается методом ближайшего
соседа. Так видно ровно то, что увидит Windows, а не сглаженную картинку.
"""
import os
import struct
import zlib

from build_icon import CORNER_RADIUS, GRID, MARK, PLATE, SIZE

SIZES = [16, 24, 32, 48]
ZOOM = 9
GAP = 12
BG = (0x2A, 0x2F, 0x38)

SS = 4


def sample(gx, gy):
    """Цвет знака в точке сетки 32x32 или None, если это плашка."""
    for x, y, w, h, color in MARK:
        if x <= gx < x + w and y <= gy < y + h:
            return color
    return None


def inside_plate(gx, gy):
    r = CORNER_RADIUS / float(SIZE) * GRID
    cx = min(max(gx, r), GRID - r)
    cy = min(max(gy, r), GRID - r)
    return (gx - cx) ** 2 + (gy - cy) ** 2 <= r * r


def render(size):
    """Пиксели size x size как список списков (r, g, b)."""
    out = []
    step = GRID / float(size)
    for py in range(size):
        row = []
        for px in range(size):
            acc = [0.0, 0.0, 0.0]
            for sy in range(SS):
                for sx in range(SS):
                    gx = (px + (sx + 0.5) / SS) * step
                    gy = (py + (sy + 0.5) / SS) * step
                    if not inside_plate(gx, gy):
                        c = BG
                    else:
                        c = sample(gx, gy) or PLATE
                    acc[0] += c[0]
                    acc[1] += c[1]
                    acc[2] += c[2]
            n = float(SS * SS)
            row.append((int(acc[0] / n), int(acc[1] / n), int(acc[2] / n)))
        out.append(row)
    return out


def main():
    tiles = [(s, render(s)) for s in SIZES]
    width = sum(s * ZOOM for s, _ in tiles) + GAP * (len(tiles) + 1)
    height = max(s * ZOOM for s, _ in tiles) + GAP * 2

    canvas = [[BG for _ in range(width)] for _ in range(height)]
    x_cursor = GAP
    for size, pixels in tiles:
        y_off = GAP
        for py in range(size):
            for px in range(size):
                c = pixels[py][px]
                for zy in range(ZOOM):
                    for zx in range(ZOOM):
                        canvas[y_off + py * ZOOM + zy][x_cursor + px * ZOOM + zx] = c
        x_cursor += size * ZOOM + GAP

    raw = bytearray()
    for row in canvas:
        raw.append(0)
        for r, g, b in row:
            raw += bytes((r, g, b))

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    body = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon-preview.png")
    with open(path, "wb") as f:
        f.write(body)
    print("написан", path, "— размеры:", ", ".join(str(s) for s in SIZES))


if __name__ == "__main__":
    main()
