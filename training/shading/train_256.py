#!/usr/bin/env python3
"""Train the attention U-Net on 512px real pairs with random 256px crops."""

import argparse
from pathlib import Path

import cv2
import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
import numpy as np

from compare import loss_fn
from models import build
from train import load_dataset

CROP = 256


def batch(masks, indices, rng, flip=True):
    lines = []
    targets = []
    lights = []
    light = np.array([-0.35, 0.85, 0.4], dtype=np.float32)
    light /= np.linalg.norm(light)
    for index in indices:
        height, width = masks[index].shape[:2]
        x = rng.integers(0, width - CROP + 1)
        y = rng.integers(0, height - CROP + 1)
        tile = masks[index][y:y + CROP, x:x + CROP].copy()
        direction = light.copy()
        if flip and rng.random() < 0.5:
            tile = tile[:, ::-1, :]
            direction[0] *= -1
        lines.append(tile[..., 0:1])
        targets.append(tile[..., 1:])
        lights.append(direction)
    return (np.stack(lines).astype(np.float32), np.array(lights, dtype=np.float32),
            np.stack(targets).astype(np.float32))


def dice(probability, target):
    predicted = (probability > 0.5).astype(np.float32)
    scores = []
    for channel in range(2):
        p = predicted[..., channel]
        t = target[..., channel] > 0.5
        scores.append((2 * np.sum(p * t) + 1) / (p.sum() + t.sum() + 1))
    return scores


def evaluate(model, masks, indices, rng):
    scores = [0.0, 0.0]
    for index in indices:
        line, light, target = batch(masks, [index], rng, flip=False)
        probability = np.asarray(mx.sigmoid(model(mx.array(line), mx.array(light))))[0]
        current = dice(probability, target[0])
        scores = [scores[0] + current[0], scores[1] + current[1]]
    return [score / max(len(indices), 1) for score in scores]


def render(model, path, output):
    image = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
    height, width = image.shape
    line = cv2.resize(image, (CROP, CROP)).astype(np.float32) / 255.0
    ink = 1.0 - line
    light = np.array([-0.35, 0.85, 0.4], dtype=np.float32)
    light /= np.linalg.norm(light)
    probability = np.asarray(mx.sigmoid(model(
        mx.array(ink[None, :, :, None]), mx.array(light[None, :]))))[0]
    shadow = cv2.resize(probability[..., 0], (width, height), interpolation=cv2.INTER_CUBIC)
    highlight = cv2.resize(probability[..., 1], (width, height), interpolation=cv2.INTER_CUBIC)
    paper = image.astype(np.float32)[..., None] / 255.0
    shadow_color = np.array([0.38, 0.33, 0.62], dtype=np.float32)
    highlight_color = np.array([1.0, 0.95, 0.72], dtype=np.float32)
    paper = paper * (1.0 - shadow[..., None] * 0.55) + shadow_color * shadow[..., None] * 0.55
    paper = paper * (1.0 - highlight[..., None] * 0.45) + highlight_color * highlight[..., None] * 0.45
    out = Path(output)
    out.parent.mkdir(parents=True, exist_ok=True)
    cv2.imwrite(str(out), cv2.cvtColor(np.clip(paper * 255, 0, 255).astype(np.uint8), cv2.COLOR_RGB2BGR))
    print("wrote", out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path,
                        default=Path(__file__).parent / "data" / "real_512.bin")
    parser.add_argument("--epochs", type=int, default=30)
    parser.add_argument("--batch", type=int, default=4)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--line", type=Path,
                        default=Path(__file__).parent / "real" / "refs" / "fumia_line.jpg")
    parser.add_argument("--out", type=Path,
                        default=Path(__file__).parent / "real" / "refs" / "fumia_shade_256.png")
    args = parser.parse_args()
    masks, _, width = load_dataset(args.data)
    if width != 512:
        raise SystemExit("expected a 512px dataset")
    rng = np.random.default_rng(17)
    order = rng.permutation(len(masks))
    split = max(1, int(len(order) * 0.9))
    train_index, val_index = order[:split], order[split:]
    model = build("attention")
    mx.eval(model.parameters())
    optimizer = optim.AdamW(learning_rate=args.lr, weight_decay=1e-4)
    loss_and_grad = nn.value_and_grad(model, loss_fn)
    for epoch in range(args.epochs):
        order = rng.permutation(train_index)
        total = 0.0
        for start in range(0, len(order), args.batch):
            index = order[start:start + args.batch]
            line, light, target = batch(masks, index, rng)
            loss, grads = loss_and_grad(model, mx.array(line), mx.array(light), mx.array(target))
            optimizer.update(model, grads)
            mx.eval(model.parameters(), optimizer.state)
            total += float(loss) * len(index)
        scores = evaluate(model, masks, val_index, rng)
        print(f"epoch {epoch + 1:02d} loss {total / len(order):.4f} "
              f"val dice {scores[0]:.3f}/{scores[1]:.3f}", flush=True)
    render(model, args.line, args.out)


if __name__ == "__main__":
    main()
