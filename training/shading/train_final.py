#!/usr/bin/env python3
"""Train the selected attention U-Net and render a shading proposal on a line drawing."""

import argparse
from pathlib import Path

import cv2
import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
import numpy as np

from compare import evaluate, loss_fn
from models import build
from train import SIZE, load_dataset


def train(model, masks, light, train_index, val_index, epochs, batch, lr):
    optimizer = optim.AdamW(learning_rate=lr, weight_decay=1e-4)
    loss_and_grad = nn.value_and_grad(model, loss_fn)
    rng = np.random.default_rng(13)
    train_masks, train_light = masks[train_index], light[train_index]
    val_masks, val_light = masks[val_index], light[val_index]
    best = (-1.0, -1.0)
    for epoch in range(epochs):
        epoch_masks = train_masks.copy()
        epoch_light = train_light.copy()
        flip = rng.random(len(epoch_masks)) < 0.5
        if flip.any():
            epoch_masks[flip] = epoch_masks[flip, :, ::-1, :]
            epoch_light[flip, 0] *= -1
        order = rng.permutation(len(epoch_masks))
        total = 0.0
        for start in range(0, len(order), batch):
            index = order[start : start + batch]
            line = mx.array(epoch_masks[index, :, :, 0:1])
            target = mx.array(epoch_masks[index, :, :, 1:])
            light_batch = mx.array(epoch_light[index])
            loss, grads = loss_and_grad(model, line, light_batch, target)
            optimizer.update(model, grads)
            mx.eval(model.parameters(), optimizer.state)
            total += float(loss) * len(index)
        scores = evaluate(model, val_masks[..., 0:1], val_light, val_masks[..., 1:])
        print(f"epoch {epoch + 1:02d} loss {total / len(order):.4f} dice {scores[0]:.3f}/{scores[1]:.3f}", flush=True)
        if sum(scores) > sum(best):
            best = tuple(scores)
    return best


def render(model, path, output):
    image = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
    if image is None:
        raise SystemExit(f"cannot read {path}")
    height, width = image.shape
    line = cv2.resize(image, (SIZE, SIZE)).astype(np.float32) / 255.0
    line = 1.0 - line  # ink = 1 on white paper
    light = np.array([-0.35, 0.85, 0.4], dtype=np.float32)
    light /= np.linalg.norm(light)
    probability = np.asarray(mx.sigmoid(model(
        mx.array(line[None, :, :, None]), mx.array(light[None, :]))))[0]
    shadow = cv2.resize(probability[..., 0], (width, height), interpolation=cv2.INTER_LINEAR)
    highlight = cv2.resize(probability[..., 1], (width, height), interpolation=cv2.INTER_LINEAR)
    paper = np.ones((height, width, 3), dtype=np.float32) * (1.0 - line[..., None])
    # The resized line is only a mask here; rebuild the composition from the original ink.
    ink = image.astype(np.float32)[..., None] / 255.0
    paper = ink
    shadow_color = np.array([0.38, 0.33, 0.62], dtype=np.float32)
    highlight_color = np.array([1.0, 0.95, 0.72], dtype=np.float32)
    paper = paper * (1.0 - shadow[..., None] * 0.55) + shadow_color * shadow[..., None] * 0.55
    paper = paper * (1.0 - highlight[..., None] * 0.45) + highlight_color * highlight[..., None] * 0.45
    out = Path(output)
    out.parent.mkdir(parents=True, exist_ok=True)
    cv2.imwrite(str(out), cv2.cvtColor(np.clip(paper * 255, 0, 255).astype(np.uint8), cv2.COLOR_RGB2BGR))
    cv2.imwrite(str(out.with_name(out.stem + "_shadow.png")), (shadow * 255).astype(np.uint8))
    cv2.imwrite(str(out.with_name(out.stem + "_highlight.png")), (highlight * 255).astype(np.uint8))
    print("wrote", out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path,
                        default=Path(__file__).parent / "data" / "shading.bin")
    parser.add_argument("--arch", default="attention")
    parser.add_argument("--epochs", type=int, default=24)
    parser.add_argument("--batch", type=int, default=32)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--line", type=Path, default=None)
    parser.add_argument("--out", type=Path, default=Path(__file__).parent / "out" / "shade.png")
    args = parser.parse_args()
    if not mx.metal.is_available():
        raise SystemExit("MLX Metal is not available")
    masks, light, width = load_dataset(args.data)
    if width != SIZE:
        raise SystemExit("wrong dataset size")
    rng = np.random.default_rng(5)
    order = rng.permutation(len(masks))
    split = int(len(order) * 0.9)
    model = build(args.arch)
    mx.eval(model.parameters())
    scores = train(model, masks, light, order[:split], order[split:],
                   args.epochs, args.batch, args.lr)
    print("best dice", scores)
    if args.line:
        render(model, args.line, args.out)


if __name__ == "__main__":
    main()
