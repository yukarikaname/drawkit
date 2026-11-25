#!/usr/bin/env python3
"""Generate stylised anime head renders with exact key-light labels.

The mannequin dataset teaches body shading, but an anime reference is mostly a head, hair and
cel bands. This generator draws those directly in NumPy: ellipsoid normals, a key light, two or
three cel bands, an outline and a small set of face marks. It is synthetic, but it is in the same
visual family as the reference sheets the estimator is meant for.

    training/lighting/.venv/bin/python training/lighting/anime_data.py --count 12000

writes the same DKLT file the Rust mannequin example writes, so train.py can concatenate both.
"""

import argparse
import struct
from pathlib import Path

import numpy as np


SIZE = 96


def ellipsoid(u, v, center, radii):
    x = (u - center[0]) / radii[0]
    y = (v - center[1]) / radii[1]
    inside = x * x + y * y <= 1.0
    z = np.sqrt(np.clip(1.0 - x * x - y * y, 0.0, 1.0))
    normal = np.stack([x, y, z], axis=-1)
    return inside, normal


def rotate(normal, yaw, pitch):
    cos_y, sin_y = np.cos(yaw), np.sin(yaw)
    cos_p, sin_p = np.cos(pitch), np.sin(pitch)
    x = normal[..., 0] * cos_y + normal[..., 2] * sin_y
    z = -normal[..., 0] * sin_y + normal[..., 2] * cos_y
    y = normal[..., 1] * cos_p - z * sin_p
    z = normal[..., 1] * sin_p + z * cos_p
    return np.stack([x, y, z], axis=-1)


def shade(normal, light, intensity, ambient, levels):
    diffuse = np.clip(np.sum(normal * light, axis=-1), 0.0, 1.0)
    value = ambient + diffuse * intensity
    if levels:
        value = np.round(value * levels) / levels
    return np.clip(value, 0.0, 1.0)


def paint(image, mask, albedo, value):
    image[mask] = np.clip(albedo[mask] * value[mask] if np.ndim(albedo) else albedo * value[mask], 0, 1)


def edge(mask):
    up = np.roll(mask, 1, axis=0)
    down = np.roll(mask, -1, axis=0)
    left = np.roll(mask, 1, axis=1)
    right = np.roll(mask, -1, axis=1)
    return mask & ~(up & down & left & right)


def sample(rng, u, v):
    background = rng.uniform(0.52, 0.95)
    gradient = rng.uniform(-0.06, 0.06)
    image = np.clip(background + gradient * v, 0.0, 1.0)
    coverage = np.zeros_like(image, dtype=bool)

    az = rng.uniform(-np.pi, np.pi)
    el = rng.uniform(0.02, 1.35)
    light = np.array([np.cos(el) * np.sin(az), np.sin(el), np.cos(el) * np.cos(az)])
    intensity = rng.uniform(0.45, 1.15)
    ambient = rng.uniform(0.12, 0.48)
    levels = 0 if rng.random() < 0.15 else int(rng.integers(3, 7))

    yaw = rng.uniform(-0.55, 0.55)
    pitch = rng.uniform(-0.28, 0.28)
    shift = yaw * 0.18

    # Shoulders and neck.
    shoulder_mask, shoulder_n = ellipsoid(u, v, (0.0, -0.78), (0.85, 0.32))
    shoulder_value = shade(rotate(shoulder_n, yaw, pitch), light, intensity * 0.7, ambient, levels)
    cloth = rng.uniform(0.18, 0.85)
    paint(image, shoulder_mask, cloth, shoulder_value)
    coverage |= shoulder_mask

    neck_mask, neck_n = ellipsoid(u, v, (shift * 0.5, -0.45), (0.16, 0.26))
    skin = rng.uniform(0.70, 0.96)
    skin_value = shade(rotate(neck_n, yaw, pitch), light, intensity, ambient, levels)
    paint(image, neck_mask, skin * 0.92, skin_value)
    coverage |= neck_mask

    # Back hair sits behind the face; the face is painted over it.
    hair = rng.uniform(0.10, 0.78)
    back_mask, back_n = ellipsoid(u, v, (0.0, 0.02), (0.48, 0.58))
    back_value = shade(rotate(back_n, yaw, pitch), light, intensity, ambient, levels)
    paint(image, back_mask, hair, back_value)
    coverage |= back_mask

    # Head.
    rx = rng.uniform(0.27, 0.36)
    ry = rng.uniform(0.34, 0.43)
    head_mask, head_n = ellipsoid(u, v, (0.0, 0.0), (rx, ry))
    head_value = shade(rotate(head_n, yaw, pitch), light, intensity, ambient, levels)
    paint(image, head_mask, skin, head_value)
    coverage |= head_mask

    # Fringe: a few overlapping caps, each with its own ellipsoid normal.
    for _ in range(rng.integers(3, 6)):
        fx = rng.uniform(-0.24, 0.24)
        fy = rng.uniform(0.16, 0.34)
        frx = rng.uniform(0.06, 0.16)
        fry = rng.uniform(0.06, 0.16)
        mask, normal = ellipsoid(u, v, (fx, fy), (frx, fry))
        value = shade(rotate(normal, yaw, pitch), light, intensity, ambient, levels)
        paint(image, mask, hair * rng.uniform(0.85, 1.15), value)
        coverage |= mask

    # Side locks frame the face.
    for side in (-1.0, 1.0):
        mask, normal = ellipsoid(
            u, v, (side * (rx + 0.02), -0.04 - rng.uniform(0.0, 0.10)),
            (rng.uniform(0.07, 0.12), rng.uniform(0.18, 0.30)),
        )
        value = shade(rotate(normal, yaw, pitch), light, intensity, ambient, levels)
        paint(image, mask, hair, value * 0.9)
        coverage |= mask

    # Face marks are flat dark shapes, dimmed on the shadow side by the same shade value.
    eye_y = 0.02 + pitch * 0.08
    for side in (-1.0, 1.0):
        eye_x = side * 0.15 + shift
        eye_mask, _ = ellipsoid(u, v, (eye_x, eye_y), (0.065, 0.085))
        eye_value = shade(np.array([0.0, 0.0, 1.0]), light, intensity * 0.6, ambient, 0)
        paint(image, eye_mask, 0.10, np.full_like(image, eye_value))
        iris, _ = ellipsoid(u, v, (eye_x + shift * 0.15, eye_y), (0.032, 0.045))
        paint(image, iris, 0.35, np.full_like(image, 0.25 + 0.35 * ambient))
        brow, _ = ellipsoid(u, v, (eye_x, eye_y + 0.10), (0.075, 0.018))
        paint(image, brow, 0.18, np.full_like(image, 0.5 + 0.3 * ambient))
        coverage |= eye_mask | iris | brow

    mouth, _ = ellipsoid(u, v, (shift * 0.4, -0.22), (0.06, 0.014))
    paint(image, mouth, 0.35, np.full_like(image, 0.35 + 0.3 * ambient))
    coverage |= mouth

    # Silhouette and hair boundary lines.
    line = edge(coverage) | (edge(head_mask) & ~head_mask)
    image[line] = np.minimum(image[line], 0.10 + rng.uniform(0.0, 0.12))
    image = np.clip(image + rng.normal(0.0, 0.006, image.shape), 0.0, 1.0)

    labels = np.array([light[0], light[1], light[2], intensity, ambient, 0.0], dtype=np.float32)
    return image.astype(np.float32), labels


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--count", type=int, default=12000)
    parser.add_argument("--out", type=Path,
                        default=Path(__file__).parent / "data" / "anime.bin")
    parser.add_argument("--seed", type=int, default=11)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(args.seed)
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    u = (x + 0.5) / SIZE * 2.0 - 1.0
    v = -((y + 0.5) / SIZE * 2.0 - 1.0)

    with args.out.open("wb") as file:
        file.write(b"DKLT")
        for value in (1, args.count, SIZE, SIZE, 1):
            file.write(struct.pack("<I", value))
        for index in range(args.count):
            image, labels = sample(rng, u, v)
            file.write(image.astype("<f4").tobytes())
            file.write(labels.astype("<f4").tobytes())
            if (index + 1) % 1000 == 0:
                print(f"{index + 1}/{args.count}", flush=True)
    print(f"wrote {args.count} samples to {args.out}")


if __name__ == "__main__":
    main()
