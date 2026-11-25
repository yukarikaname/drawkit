#!/usr/bin/env python3
"""Build aligned line/shadow/highlight pairs from coloured art and Anime2Sketch lines."""

import argparse
import struct
from pathlib import Path

import cv2
import numpy as np

SIZE = 96


def masks(colour, ink):
    gray = cv2.cvtColor(colour, cv2.COLOR_BGR2GRAY).astype(np.float32) / 255.0
    base = cv2.GaussianBlur(gray, (0, 0), 5.0)
    shadow = np.clip((base - gray - 0.015) / 0.10, 0.0, 1.0)
    highlight = np.clip((gray - base - 0.015) / 0.10, 0.0, 1.0)
    shadow[ink > 0.65] = 0.0
    highlight[ink > 0.65] = 0.0
    return shadow.astype(np.float32), highlight.astype(np.float32)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--images", type=Path, default=Path(__file__).parent / "real" / "safe")
    parser.add_argument("--lines", type=Path, default=Path(__file__).parent / "real" / "lines_a2s")
    parser.add_argument("--out", type=Path, default=Path(__file__).parent / "data" / "real_large.bin")
    args = parser.parse_args()
    line_files = {path.stem: path for path in args.lines.iterdir() if path.is_file()}
    pairs = []
    for path in sorted(args.images.iterdir()):
        line_path = line_files.get(path.stem)
        if line_path is None:
            continue
        colour = cv2.imread(str(path))
        line = cv2.imread(str(line_path), cv2.IMREAD_GRAYSCALE)
        if colour is None or line is None:
            continue
        colour = cv2.resize(colour, (SIZE, SIZE), interpolation=cv2.INTER_AREA)
        line = cv2.resize(line, (SIZE, SIZE), interpolation=cv2.INTER_AREA)
        ink = 1.0 - line.astype(np.float32) / 255.0
        if not 0.015 < ink.mean() < 0.42:
            continue
        shadow, highlight = masks(colour, ink)
        if not (0.002 < shadow.mean() < 0.5 and 0.002 < highlight.mean() < 0.5):
            continue
        pairs.append((ink, shadow, highlight))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("wb") as file:
        file.write(b"DKSH")
        for value in (1, len(pairs), SIZE, SIZE, 3):
            file.write(struct.pack("<I", value))
        for ink, shadow, highlight in pairs:
            for mask in (ink, shadow, highlight):
                file.write((mask * 255.0).round().astype(np.uint8).tobytes())
            light = np.array([-0.35, 0.85, 0.4], dtype="<f4")
            light /= np.linalg.norm(light)
            file.write(light.tobytes())
    print("wrote", len(pairs), "pairs")


if __name__ == "__main__":
    main()
