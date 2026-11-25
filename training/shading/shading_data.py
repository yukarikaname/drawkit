#!/usr/bin/env python3
"""Generate line-art / shadow / highlight triples with exact geometry labels.

The existing lighting model answers "where is the key light". This dataset answers the next
question: given only the drawing's lines, which pixels are shadow and which are highlight for a
chosen light. The renderer is the same ellipsoid head as the lighting set, but now it keeps the
normal and diffuse maps instead of collapsing them into a shaded image.

    training/lighting/.venv/bin/python training/shading/shading_data.py --count 12000
"""

import argparse
import struct
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lighting"))
from anime_data import edge, ellipsoid, rotate  # noqa: E402

SIZE = 96


def dilate(mask, radius):
    out = mask.copy()
    for _ in range(radius):
        out = (
            out
            | np.roll(out, 1, 0)
            | np.roll(out, -1, 0)
            | np.roll(out, 1, 1)
            | np.roll(out, -1, 1)
        )
    return out


def blur(mask):
    total = mask.astype(np.float32)
    for axis in (0, 1):
        total = total + np.roll(total, 1, axis) + np.roll(total, -1, axis)
    return np.clip(total / 9.0, 0.0, 1.0)


def draw(rng, u, v):
    normal = np.zeros((SIZE, SIZE, 3), dtype=np.float32)
    normal[..., 2] = 1.0
    diffuse = np.zeros((SIZE, SIZE), dtype=np.float32)
    coverage = np.zeros((SIZE, SIZE), dtype=bool)
    internal = np.zeros((SIZE, SIZE), dtype=bool)

    az = rng.uniform(-np.pi, np.pi)
    el = rng.uniform(0.02, 1.35)
    light = np.array([np.cos(el) * np.sin(az), np.sin(el), np.cos(el) * np.cos(az)])
    yaw = rng.uniform(-0.5, 0.5)
    pitch = rng.uniform(-0.25, 0.25)
    shift = yaw * 0.18

    def put(mask, source_normal, shade_normal=True):
        rotated = rotate(source_normal, yaw, pitch) if shade_normal else source_normal
        diffuse[mask] = np.sum(rotated * light, axis=-1)[mask]
        normal[mask] = rotated[mask]
        coverage[mask] = True

    shoulder, shoulder_n = ellipsoid(u, v, (0.0, -0.78), (0.85, 0.32))
    put(shoulder, shoulder_n)
    neck, neck_n = ellipsoid(u, v, (shift * 0.5, -0.45), (0.16, 0.26))
    put(neck, neck_n)
    back, back_n = ellipsoid(u, v, (0.0, 0.02), (0.48, 0.58))
    put(back, back_n)

    rx = rng.uniform(0.27, 0.36)
    ry = rng.uniform(0.34, 0.43)
    head, head_n = ellipsoid(u, v, (0.0, 0.0), (rx, ry))
    put(head, head_n)
    internal |= edge(head) & ~edge(back)

    fringe_masks = []
    for _ in range(rng.integers(3, 6)):
        cap, cap_n = ellipsoid(
            u,
            v,
            (rng.uniform(-0.24, 0.24), rng.uniform(0.16, 0.34)),
            (rng.uniform(0.06, 0.16), rng.uniform(0.06, 0.16)),
        )
        put(cap, cap_n)
        fringe_masks.append(cap)
        internal |= edge(cap)
    for side in (-1.0, 1.0):
        lock, lock_n = ellipsoid(
            u,
            v,
            (side * (rx + 0.02), -0.04 - rng.uniform(0.0, 0.10)),
            (rng.uniform(0.07, 0.12), rng.uniform(0.18, 0.30)),
        )
        put(lock, lock_n)
        internal |= edge(lock)

    # Face marks are drawing lines, not shaded regions.
    marks = np.zeros_like(coverage)
    eye_y = 0.02 + pitch * 0.08
    for side in (-1.0, 1.0):
        eye_x = side * 0.15 + shift
        eye, _ = ellipsoid(u, v, (eye_x, eye_y), (0.070, 0.090))
        marks |= eye
        iris, _ = ellipsoid(u, v, (eye_x + shift * 0.15, eye_y), (0.032, 0.045))
        marks |= iris
        brow, _ = ellipsoid(u, v, (eye_x, eye_y + 0.10), (0.080, 0.020))
        marks |= brow
    mouth, _ = ellipsoid(u, v, (shift * 0.4, -0.22), (0.06, 0.014))
    marks |= mouth

    line = (edge(coverage) | internal | marks).astype(np.float32)
    line = dilate(line > 0, int(rng.integers(0, 3))).astype(np.float32)
    holes = rng.random(line.shape) < rng.uniform(0.0, 0.12)
    line[holes] = 0.0
    line[~coverage & (rng.random(line.shape) < 0.002)] = 0.0

    shadow = (coverage & (diffuse < 0.34) & (line < 0.5)).astype(np.float32)
    highlight = (coverage & (diffuse > 0.72) & (line < 0.5)).astype(np.float32)
    shadow = blur(shadow)
    highlight = blur(highlight)
    if rng.random() < 0.5:
        shadow = np.clip(shadow * rng.uniform(0.85, 1.0), 0.0, 1.0)
        highlight = np.clip(highlight * rng.uniform(0.85, 1.0), 0.0, 1.0)
    return line, shadow, highlight, light.astype(np.float32)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--count", type=int, default=12000)
    parser.add_argument("--out", type=Path,
                        default=Path(__file__).parent / "data" / "shading.bin")
    parser.add_argument("--seed", type=int, default=23)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(args.seed)
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    u = (x + 0.5) / SIZE * 2.0 - 1.0
    v = -((y + 0.5) / SIZE * 2.0 - 1.0)

    with args.out.open("wb") as file:
        file.write(b"DKSH")
        for value in (1, args.count, SIZE, SIZE, 3):
            file.write(struct.pack("<I", value))
        for index in range(args.count):
            line, shadow, highlight, light = draw(rng, u, v)
            for mask in (line, shadow, highlight):
                file.write((mask * 255.0).round().astype(np.uint8).tobytes())
            file.write(light.astype("<f4").tobytes())
            if (index + 1) % 1000 == 0:
                print(f"{index + 1}/{args.count}", flush=True)
    print(f"wrote {args.count} samples to {args.out}")


if __name__ == "__main__":
    main()
