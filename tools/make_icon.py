#!/usr/bin/env python3
"""Draws the Dark Messenger app icon (a lightning bolt on a dark violet
disc) and writes the Android launcher mipmaps plus the 1024px macOS icon.
Pure standard library: no Pillow needed.

    tools/make_icon.py
"""
import math
import os
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SIZES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}

# Bolt outline in unit coordinates (0..1, y down).
BOLT = [(0.58, 0.10), (0.30, 0.54), (0.47, 0.54), (0.38, 0.90), (0.72, 0.42), (0.54, 0.42), (0.66, 0.10)]


def inside(poly, x, y):
    hit = False
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            hit = not hit
        j = i
    return hit


def edge_distance(poly, x, y):
    best = 1e9
    for i in range(len(poly)):
        (ax, ay), (bx, by) = poly[i], poly[(i + 1) % len(poly)]
        dx, dy = bx - ax, by - ay
        t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
        best = min(best, math.hypot(x - ax - t * dx, y - ay - t * dy))
    return best


def shade(u, v):
    """RGBA for a point in unit space."""
    r = math.hypot(u - 0.5, v - 0.5)
    if r > 0.5:
        return (0.0, 0.0, 0.0, 0.0)
    # Violet disc, brighter toward the centre, thin rim.
    t = r / 0.5
    base = (0.10 + 0.22 * (1 - t), 0.02 + 0.04 * (1 - t), 0.20 + 0.32 * (1 - t))
    if t > 0.93:
        base = (0.55, 0.30, 0.85)
    col = base
    # Glow around the bolt, then the bolt itself.
    d = 0.0 if inside(BOLT, u, v) else edge_distance(BOLT, u, v)
    glow = math.exp(-d * 28.0)
    col = tuple(c + (g - c) * glow * 0.8 for c, g in zip(col, (0.75, 0.55, 1.0)))
    if d == 0.0:
        col = (0.97, 0.95, 1.0)
    return (*col, 1.0)


def render(size, samples=4):
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(samples):
                for sx in range(samples):
                    c = shade((x + (sx + 0.5) / samples) / size, (y + (sy + 0.5) / samples) / size)
                    for k in range(3):
                        acc[k] += c[k] * c[3]
                    acc[3] += c[3]
            n = samples * samples
            a = acc[3] / n
            rgb = [acc[k] / acc[3] if acc[3] else 0.0 for k in range(3)]
            row += bytes(int(max(0, min(1, v)) * 255 + 0.5) for v in (*rgb, a))
        rows.append(bytes(row))
    return b"".join(rows)


def write_png(path, size, raw):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(png)
    print(f"{os.path.relpath(path, ROOT)} ({size}x{size})")


for density, size in SIZES.items():
    write_png(os.path.join(ROOT, "assets", "android-res", f"mipmap-{density}", "ic_launcher.png"), size, render(size))
write_png(os.path.join(ROOT, "assets", "macos-icon.png"), 1024, render(1024, samples=1))
