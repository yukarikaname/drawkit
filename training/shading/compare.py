#!/usr/bin/env python3
"""Train several small architectures on the same shading data and compare their masks."""

import argparse
import json
from pathlib import Path

import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
import numpy as np

from models import build
from train import SIZE, load_dataset


def loss_fn(model, line, light, target):
    probability = mx.sigmoid(model(line, light))
    eps = 1e-6
    # A highlight is a small fraction of the canvas; equal channel weights and a positive
    # weight keep the second head from collapsing to zero in the first epochs.
    losses = []
    for channel, positive in ((0, 1.5), (1, 4.0)):
        value = target[..., channel]
        weight = 1.0 + (positive - 1.0) * value
        losses.append(-mx.mean(weight * (value * mx.log(probability[..., channel] + eps)
                                         + (1.0 - value) * mx.log(1.0 - probability[..., channel] + eps))))
    bce = losses[0] + losses[1]
    intersection = mx.sum(probability * target, axis=(1, 2, 3))
    dice = 1.0 - mx.mean((2.0 * intersection + 1.0)
                         / (mx.sum(probability, axis=(1, 2, 3))
                            + mx.sum(target, axis=(1, 2, 3)) + 1.0))
    return bce + dice


def evaluate(model, line, light, target):
    probability = np.asarray(mx.sigmoid(model(mx.array(line), mx.array(light))))
    predicted = (probability > 0.5).astype(np.float32)
    scores = []
    for channel in range(2):
        p = predicted[..., channel]
        t = target[..., channel] > 0.5
        scores.append((2 * np.sum(p * t) + 1) / (p.sum() + t.sum() + 1))
    return scores


def run(name, masks, light, train_index, val_index, epochs, batch, lr):
    model = build(name)
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
        best = max(best, tuple(scores))
        print(f"{name:12s} epoch {epoch + 1:02d} loss {total / len(order):.4f} "
              f"dice {scores[0]:.3f}/{scores[1]:.3f}", flush=True)
    return {"shadow": float(best[0]), "highlight": float(best[1])}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path,
                        default=Path(__file__).parent / "data" / "shading.bin")
    parser.add_argument("--archs", default="unet,film,film_nocoord,attention")
    parser.add_argument("--limit", type=int, default=5000)
    parser.add_argument("--epochs", type=int, default=8)
    parser.add_argument("--batch", type=int, default=32)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--out", type=Path,
                        default=Path(__file__).parent / "out" / "compare.json")
    args = parser.parse_args()
    masks, light, width = load_dataset(args.data)
    if width != SIZE:
        raise SystemExit("wrong dataset size")
    masks, light = masks[: args.limit], light[: args.limit]
    rng = np.random.default_rng(5)
    order = rng.permutation(len(masks))
    split = int(len(order) * 0.9)
    results = {}
    for name in args.archs.split(","):
        results[name] = run(name.strip(), masks, light, order[:split], order[split:],
                            args.epochs, args.batch, args.lr)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(results, indent=2))
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
