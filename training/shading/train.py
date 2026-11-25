#!/usr/bin/env python3
"""Train the line-art shading proposal model with MLX.

The dataset is made by training/shading/shading_data.py:

    "DKSH" u32 version=1 u32 count u32 size u32 size u32 channels=3
    per sample: u8 line[size*size], u8 shadow[size*size], u8 highlight[size*size],
                f32 light[3]

The network is a small U-Net. It sees the line ink and three constant planes carrying the chosen
key direction, and returns soft shadow and highlight masks at the training resolution.

    training/lighting/.venv/bin/python training/shading/train.py --epochs 24
"""

import argparse
import math
import struct
from pathlib import Path

import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
import numpy as np


SIZE = 96
MAGIC = b"DKSW"


def load_dataset(path: Path):
    with path.open("rb") as file:
        if file.read(4) != b"DKSH":
            raise SystemExit(f"{path} is not a shading dataset")
        version, count, width, height, channels = struct.unpack("<5I", file.read(20))
        if version != 1 or channels != 3 or width != height:
            raise SystemExit("unsupported dataset layout")
        raw = np.fromfile(file, dtype=np.uint8)
    plane_size = width * height
    stride = plane_size * 3 + 12
    if raw.size != count * stride:
        raise SystemExit(f"expected {count * stride} bytes, found {raw.size}")
    masks = np.empty((count, height, width, 3), dtype=np.float32)
    light = np.empty((count, 3), dtype=np.float32)
    for index in range(count):
        start = index * stride
        planes = raw[start : start + plane_size * 3].reshape(3, height, width)
        masks[index] = planes.transpose(1, 2, 0) / 255.0
        light[index] = np.frombuffer(
            raw[start + plane_size * 3 : start + stride].tobytes(), dtype="<f4"
        )
    return masks, light, width

class ShadingNet(nn.Module):
    def __init__(self):
        super().__init__()
        self.c1 = nn.Conv2d(4, 16, 3, stride=2, padding=1)
        self.c2 = nn.Conv2d(16, 32, 3, stride=2, padding=1)
        self.c3 = nn.Conv2d(32, 64, 3, stride=2, padding=1)
        self.u2 = nn.Conv2d(64 + 32, 64, 3, padding=1)
        self.u1 = nn.Conv2d(64 + 16, 32, 3, padding=1)
        self.u0 = nn.Conv2d(32, 16, 3, padding=1)
        self.out = nn.Conv2d(16, 2, 1)

    def __call__(self, x):
        e1 = nn.relu(self.c1(x))
        e2 = nn.relu(self.c2(e1))
        bottom = nn.relu(self.c3(e2))
        up = upsample(bottom)
        up = nn.relu(self.u2(mx.concatenate([up, e2], axis=-1)))
        up = upsample(up)
        up = nn.relu(self.u1(mx.concatenate([up, e1], axis=-1)))
        up = upsample(up)
        up = nn.relu(self.u0(up))
        return self.out(up)


def upsample(x):
    return mx.repeat(mx.repeat(x, 2, axis=1), 2, axis=2)


def inputs(line, light):
    batch = line.shape[0]
    planes = mx.broadcast_to(light[:, None, None, :], (batch, SIZE, SIZE, 3))
    return mx.concatenate([line, planes], axis=-1)


def loss_fn(model, line, light, target):
    logits = model(inputs(line, light))
    probability = mx.sigmoid(logits)
    eps = 1e-6
    bce = -mx.mean(
        target * mx.log(probability + eps)
        + (1.0 - target) * mx.log(1.0 - probability + eps)
    )
    intersection = mx.sum(probability * target, axis=(1, 2, 3))
    dice = 1.0 - mx.mean(
        (2.0 * intersection + 1.0)
        / (mx.sum(probability, axis=(1, 2, 3)) + mx.sum(target, axis=(1, 2, 3)) + 1.0)
    )
    return bce + dice


def evaluate(model, line, light, target):
    probability = np.asarray(mx.sigmoid(model(inputs(mx.array(line), mx.array(light)))))
    predicted = (probability > 0.5).astype(np.float32)
    scores = []
    for channel in range(2):
        p = predicted[..., channel]
        t = target[..., channel] > 0.5
        intersection = np.sum(p * t)
        scores.append((2 * intersection + 1) / (p.sum() + t.sum() + 1))
    return scores


def save_weights(model, path: Path):
    tensors = [
        model.c1.weight, model.c1.bias,
        model.c2.weight, model.c2.bias,
        model.c3.weight, model.c3.bias,
        model.u2.weight, model.u2.bias,
        model.u1.weight, model.u1.bias,
        model.u0.weight, model.u0.bias,
        model.out.weight, model.out.bias,
    ]
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as file:
        file.write(MAGIC)
        file.write(struct.pack("<I", 1))
        for tensor in tensors:
            values = np.asarray(tensor, dtype="<f4").reshape(-1)
            file.write(struct.pack("<I", values.size))
            file.write(values.tobytes())
    print(f"wrote {path} ({path.stat().st_size / 1024:.1f} KB)")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path,
                        default=Path(__file__).parent / "data" / "shading.bin")
    parser.add_argument("--out", type=Path,
                        default=Path(__file__).resolve().parents[2]
                        / "crates" / "shading" / "assets" / "shading_weights.bin")
    parser.add_argument("--epochs", type=int, default=24)
    parser.add_argument("--batch", type=int, default=32)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    if not mx.metal.is_available():
        raise SystemExit("MLX Metal is not available; run on an Apple Silicon Mac")
    masks, light, width = load_dataset(args.data)
    if width != SIZE:
        raise SystemExit(f"the model expects {SIZE}x{SIZE}")
    if args.limit:
        masks, light = masks[: args.limit], light[: args.limit]
    rng = np.random.default_rng(5)
    order = rng.permutation(len(masks))
    split = int(len(order) * 0.9)
    train_index, val_index = order[:split], order[split:]
    train_masks, train_light = masks[train_index], light[train_index]
    val_masks, val_light = masks[val_index], light[val_index]
    print(f"device {mx.default_device()}, train {len(train_masks)}, val {len(val_masks)}")

    model = ShadingNet()
    mx.eval(model.parameters())
    optimizer = optim.AdamW(learning_rate=args.lr, weight_decay=1e-4)
    loss_and_grad = nn.value_and_grad(model, loss_fn)
    best = -math.inf
    for epoch in range(args.epochs):
        epoch_masks = train_masks.copy()
        epoch_light = train_light.copy()
        flip = rng.random(len(epoch_masks)) < 0.5
        if flip.any():
            epoch_masks[flip] = epoch_masks[flip, :, ::-1, :]
            epoch_light[flip, 0] *= -1
        order = rng.permutation(len(epoch_masks))
        total = 0.0
        for start in range(0, len(order), args.batch):
            index = order[start : start + args.batch]
            line = mx.array(epoch_masks[index, :, :, 0:1])
            target = mx.array(epoch_masks[index, :, :, 1:])
            light = mx.array(epoch_light[index])
            loss, grads = loss_and_grad(model, line, light, target)
            optimizer.update(model, grads)
            mx.eval(model.parameters(), optimizer.state)
            total += float(loss) * len(index)
        scores = evaluate(model, val_masks[..., 0:1], val_light, val_masks[..., 1:])
        print(f"epoch {epoch + 1:02d} loss {total / len(order):.4f} "
              f"dice shadow {scores[0]:.3f} highlight {scores[1]:.3f}")
        if scores[0] + scores[1] > best:
            best = scores[0] + scores[1]
            save_weights(model, args.out)
    print(f"best dice sum {best:.3f}")


if __name__ == "__main__":
    main()
