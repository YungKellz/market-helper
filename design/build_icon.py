# -*- coding: utf-8 -*-
"""Собирает исходный PNG иконки приложения из описания знака «Кавычки».

Знак живёт на сетке 32x32 в целых координатах, поэтому при стороне 1024
одна клетка это ровно 32 пикселя: края прямоугольников попадают точно на
границу пикселя и антиалиасинг им не нужен. Сглаживание считается только
для скруглённых углов плашки — это единственная непрямоугольная форма.

Пишем PNG вручную через zlib: на машине нет ни ImageMagick, ни Pillow,
а тянуть их ради четырёх прямоугольников незачем.
"""
import os
import struct
import zlib

SIZE = 1024
GRID = 32
UNIT = SIZE // GRID
CORNER_RADIUS = 5 * UNIT

PLATE = (0x14, 0x16, 0x1A)
ACCENT = (0x4C, 0x8D, 0xFF)
NEUTRAL = (0xE6, 0xE9, 0xEF)

# Знак «Кавычки»: две ёлочки, каждая из пяти блоков 6x6 со сдвигом на 3.
# Перекрытие в половину блока превращает лесенку в сплошной штрих —
# при касании только углами знак рассыпается на отдельные квадраты.
# Между ёлочками две пустые клетки, иначе они сливаются в шахматку.
def chevron(x0, y0, color):
    return [
        (x0 + 6, y0 + 0, 6, 6, color),
        (x0 + 3, y0 + 3, 6, 6, color),
        (x0 + 0, y0 + 6, 6, 6, color),
        (x0 + 3, y0 + 9, 6, 6, color),
        (x0 + 6, y0 + 12, 6, 6, color),
    ]


MARK = chevron(3, 7, ACCENT) + chevron(17, 7, NEUTRAL)

SUBSAMPLES = 4


def corner_coverage(px, py):
    """Доля пикселя внутри скруглённого прямоугольника, 0.0–1.0."""
    inside = 0
    step = 1.0 / SUBSAMPLES
    for sy in range(SUBSAMPLES):
        for sx in range(SUBSAMPLES):
            x = px + (sx + 0.5) * step
            y = py + (sy + 0.5) * step
            cx = min(max(x, CORNER_RADIUS), SIZE - CORNER_RADIUS)
            cy = min(max(y, CORNER_RADIUS), SIZE - CORNER_RADIUS)
            dx = x - cx
            dy = y - cy
            if dx * dx + dy * dy <= CORNER_RADIUS * CORNER_RADIUS:
                inside += 1
    return inside / float(SUBSAMPLES * SUBSAMPLES)


def build_rows():
    plate_r, plate_g, plate_b = PLATE
    # Заранее раскладываем знак в карту «пиксель -> цвет»: прямоугольников
    # мало, а проверять их для каждого из миллиона пикселей дорого.
    spans = []
    for gx, gy, gw, gh, color in MARK:
        spans.append((gy * UNIT, (gy + gh) * UNIT, gx * UNIT, (gx + gw) * UNIT, color))

    rows = []
    for y in range(SIZE):
        row = bytearray()
        row.append(0)  # фильтр PNG: без предсказания
        active = [s for s in spans if s[0] <= y < s[1]]
        near_corner = y < CORNER_RADIUS or y >= SIZE - CORNER_RADIUS

        for x in range(SIZE):
            color = None
            for _, _, x0, x1, c in active:
                if x0 <= x < x1:
                    color = c
                    break

            if color is None:
                color = (plate_r, plate_g, plate_b)

            if near_corner and (x < CORNER_RADIUS or x >= SIZE - CORNER_RADIUS):
                alpha = int(round(corner_coverage(x, y) * 255))
            else:
                alpha = 255

            row += bytes((color[0], color[1], color[2], alpha))
        rows.append(bytes(row))
    return b"".join(rows)


def chunk(tag, data):
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def write_png(path, raw):
    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    body = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(body)


def write_svg(path):
    rects = "".join(
        '\n  <rect x="{}" y="{}" width="{}" height="{}" fill="#{:02X}{:02X}{:02X}"/>'.format(
            x, y, w, h, c[0], c[1], c[2]
        )
        for (x, y, w, h, c) in MARK
    )
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="1024" height="1024">'
        '\n  <rect x="0" y="0" width="32" height="32" rx="5" fill="#{:02X}{:02X}{:02X}"/>{}\n</svg>\n'
    ).format(PLATE[0], PLATE[1], PLATE[2], rects)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(svg)


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    png_path = os.path.join(here, "app-icon.png")
    write_png(png_path, build_rows())
    write_svg(os.path.join(here, "mark.svg"))
    print("написан", png_path, os.path.getsize(png_path), "байт")
    print("написан", os.path.join(here, "mark.svg"))
