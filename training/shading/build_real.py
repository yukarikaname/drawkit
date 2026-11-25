#!/usr/bin/env python3
"""Build line/shadow/highlight pairs from side-by-side Pixiv process pages.

The artist's process pages put the line art on the left and the painted version on the right.
Each half is cropped to its content, resized to 96x96 and stored in the DKSH format used by the
shading trainer. Only local research data; the images are not committed.
"""

import argparse
import struct
from pathlib import Path

import cv2
import numpy as np

SIZE = 96


def content_crop(image):
    gray = cv2.cvtColor(image, cv2.COLOR_BGR2GRAY)
    mask = (gray < 244).astype(np.uint8)
    if mask.sum() < 50:
        return None
    ys, xs = np.where(mask > 0)
    x0, x1 = xs.min(), xs.max() + 1
    y0, y1 = ys.min(), ys.max() + 1
    pad = max(x1 - x0, y1 - y0) // 12 + 4
    x0, y0 = max(0, x0 - pad), max(0, y0 - pad)
    x1, y1 = min(image.shape[1], x1 + pad), min(image.shape[0], y1 + pad)
    # Keep the aspect ratio by padding the shorter side with paper.
    crop = image[y0:y1, x0:x1]
    height, width = crop.shape[:2]
    side = max(height, width)
    canvas = np.full((side, side, 3), 255, dtype=np.uint8)
    canvas[(side - height) // 2:(side - height) // 2 + height,
           (side - width) // 2:(side - width) // 2 + width] = crop
    return cv2.resize(canvas, (SIZE, SIZE), interpolation=cv2.INTER_AREA)


def masks_from_shaded(painted, line):
    gray = cv2.cvtColor(painted, cv2.COLOR_BGR2GRAY).astype(np.float32) / 255.0
    base = cv2.GaussianBlur(gray, (0, 0), 4.0)
    shadow = np.clip((base - gray - 0.02) / 0.12, 0.0, 1.0)
    highlight = np.clip((gray - base - 0.02) / 0.12, 0.0, 1.0)
    shadow[line > 0.6] = 0.0
    highlight[line > 0.6] = 0.0
    return shadow.astype(np.float32), highlight.astype(np.float32)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, default=Path(__file__).parent / "real" / "process_all")
    parser.add_argument("--out", type=Path, default=Path(__file__).parent / "data" / "real.bin")
    parser.add_argument("--preview", type=Path, default=Path(__file__).parent / "out" / "real_pairs.png")
    args = parser.parse_args()
    files = sorted(args.input.glob("*.png"))
    pairs = []
    for path in files:
        image = cv2.imread(str(path))
        if image is None:
            continue
        width = image.shape[1]
        left = content_crop(image[:, : width // 2])
        right = content_crop(image[:, width // 2:])
        if left is None or right is None:
            continue
        line = 1.0 - cv2.cvtColor(left, cv2.COLOR_BGR2GRAY).astype(np.float32) / 255.0
        if not 0.005 < line.mean() < 0.35:
            continue
        shadow, highlight = masks_from_shaded(right, line)
        pairs.append((path.name, line, shadow, highlight, right))
        print(path.name, "ink", round(float(line.mean()), 3),
              "shadow", round(float(shadow.mean()), 3),
              "highlight", round(float(highlight.mean()), 3))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("wb") as file:
        file.write(b"DKSH")
        for value in (1, len(pairs), SIZE, SIZE, 3):
            file.write(struct.pack("<I", value))
        for _, line, shadow, highlight, _ in pairs:
            for mask in (line, shadow, highlight):
                file.write((mask * 255.0).round().astype(np.uint8).tobytes())
            light = np.array([-0.35, 0.85, 0.4], dtype="<f4")
            light /= np.linalg.norm(light)
            file.write(light.tobytes())
    print("wrote", len(pairs), "pairs to", args.out)
    if pairs:
        args.preview.parent.mkdir(parents=True, exist_ok=True)
        rows = pairs[:8]
        preview = np.full((len(rows) * SIZE, SIZE * 3, 3), 255, dtype=np.uint8)
        for index, (_, line, shadow, highlight, right) in enumerate(rows):
            preview[index * SIZE:(index + 1) * SIZE, :SIZE] = cv2.cvtColor(
                ((1.0 - line) * 255).astype(np.uint8), cv2.COLOR_GRAY2BGR)
            for column, mask in ((1, shadow), (2, highlight)):
                color = np.array([90, 80, 160], np.float32) if column == 1 else np.array([80, 230, 250], np.float32)
                tile = (1.0 - line[..., None]) * 255
                tile = tile * (1.0 - mask[..., None] * 0.6) + color * mask[..., None] * 0.6
                preview[index * SIZE:(index + 1) * SIZE, column * SIZE:(column + 1) * SIZE] = \
                    cv2.cvtColor(np.clip(tile, 0, 255).astype(np.uint8), cv2.COLOR_RGB2BGR)
        cv2.imwrite(str(args.preview), preview)


if __name__ == "__main__":
    main()
