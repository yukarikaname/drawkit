#!/usr/bin/env python3
"""Train the small on-device lighting estimator with MLX.

The dataset comes from the Rust mannequin renderer:

    cargo run --release -p drawkit-figure --example lighting_dataset -- 6000 96

then:

    training/lighting/.venv/bin/python training/lighting/train.py

MLX uses the Apple GPU (Metal) by default. The script writes one flat little-endian file so the
Rust side needs no ONNX runtime or tensor framework:

    "DKLW" u32 version=1
    for every tensor: u32 length, f32 values

Tensor order is the order in `LightingNet::forward` in crates/figure/src/light_model.rs.
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
DIRECTION = 3
SCALARS = 2  # intensity, ambient; warmth is a display choice, not recoverable from grayscale
MAGIC = b"DKLW"


def load_dataset(path: Path) -> tuple[np.ndarray, np.ndarray, int]:
    with path.open("rb") as file:
        if file.read(4) != b"DKLT":
            raise SystemExit(f"{path} is not a lighting dataset")
        version, count, width, height, channels = struct.unpack("<5I", file.read(20))
        if version != 1 or channels != 1 or width != height:
            raise SystemExit("unsupported dataset layout")
        raw = np.fromfile(file, dtype="<f4")
    stride = width * height + 6
    if raw.size != count * stride:
        raise SystemExit(f"expected {count * stride} floats, found {raw.size}")
    raw = raw.reshape(count, stride)
    images = raw[:, : width * height].reshape(count, width, height, 1)
    labels = raw[:, width * height :]
    return images.astype(np.float32), labels.astype(np.float32), width


def standardise(images: np.ndarray) -> np.ndarray:
    """Remove exposure and paper value; the network should read the shape of the shading."""
    mean = images.mean(axis=(1, 2, 3), keepdims=True)
    std = images.std(axis=(1, 2, 3), keepdims=True) + 1e-4
    return (images - mean) / std


def augment(images: np.ndarray, labels: np.ndarray, rng: np.random.Generator) -> None:
    """Horizontal flip is a real pose/light symmetry; brightness keeps the model from
    leaning on the paper value alone."""
    flip = rng.random(len(images)) < 0.5
    if flip.any():
        images[flip] = images[flip, :, ::-1, :]
        labels[flip, 0] *= -1
    gamma = rng.uniform(0.85, 1.18, size=(len(images), 1, 1, 1)).astype(np.float32)
    images **= gamma


class LightingNet(nn.Module):
    def __init__(self):
        super().__init__()
        self.c1 = nn.Conv2d(1, 16, 3, stride=2, padding=1)
        self.c2 = nn.Conv2d(16, 32, 3, stride=2, padding=1)
        self.c3 = nn.Conv2d(32, 64, 3, stride=2, padding=1)
        self.c4 = nn.Conv2d(64, 96, 3, stride=2, padding=1)
        self.f1 = nn.Linear(9 * 96, 64)
        self.f2 = nn.Linear(64, 5)

    def __call__(self, x):
        x = nn.relu(self.c1(x))
        x = nn.relu(self.c2(x))
        x = nn.relu(self.c3(x))
        x = nn.relu(self.c4(x))
        # Keep the spatial layout: global average pooling erased which side of the face was lit.
        batch = x.shape[0]
        x = mx.mean(x.reshape(batch, 3, 2, 3, 2, 96), axis=(2, 4)).reshape(batch, 9 * 96)
        x = nn.relu(self.f1(x))
        return self.f2(x)


def loss_fn(model, images, labels):
    output = model(images)
    direction = output[:, :DIRECTION]
    direction = direction / (mx.linalg.norm(direction, axis=1, keepdims=True) + 1e-6)
    cosine = mx.sum(direction * labels[:, :DIRECTION], axis=1)
    direction_loss = mx.mean(1.0 - cosine)
    scalars = output[:, DIRECTION:] - labels[:, DIRECTION:DIRECTION + SCALARS]
    return direction_loss + 0.5 * mx.mean(scalars * scalars)


def evaluate(model, images, labels) -> dict[str, float]:
    output = np.asarray(model(mx.array(images)))
    direction = output[:, :3]
    direction /= np.linalg.norm(direction, axis=1, keepdims=True) + 1e-6
    cosine = np.clip(np.sum(direction * labels[:, :3], axis=1), -1.0, 1.0)
    error = np.degrees(np.arccos(cosine))
    intensity = np.abs(output[:, 3] - labels[:, 3])
    ambient = np.abs(output[:, 4] - labels[:, 4])
    return {
        "angle_deg": float(np.mean(error)),
        "angle_p90": float(np.percentile(error, 90)),
        "intensity_mae": float(np.mean(intensity)),
        "ambient_mae": float(np.mean(ambient)),
    }


def save_weights(model, path: Path) -> None:
    tensors = [
        model.c1.weight,
        model.c1.bias,
        model.c2.weight,
        model.c2.bias,
        model.c3.weight,
        model.c3.bias,
        model.c4.weight,
        model.c4.bias,
        model.f1.weight,
        model.f1.bias,
        model.f2.weight,
        model.f2.bias,
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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path,
                        default=Path(__file__).parent / "data" / "lighting.bin")
    parser.add_argument("--anime", type=Path,
                        default=Path(__file__).parent / "data" / "anime.bin")
    parser.add_argument("--out", type=Path,
                        default=Path(__file__).resolve().parents[2]
                        / "crates" / "figure" / "assets" / "lighting_weights.bin")
    parser.add_argument("--epochs", type=int, default=26)
    parser.add_argument("--batch", type=int, default=64)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()

    if not mx.metal.is_available():
        raise SystemExit("MLX Metal is not available; run on an Apple Silicon Mac")
    print("device:", mx.default_device())

    images, labels, width = load_dataset(args.data)
    if args.anime.exists():
        anime_images, anime_labels, anime_width = load_dataset(args.anime)
        if anime_width != width:
            raise SystemExit("the mannequin and anime datasets disagree on size")
        images = np.concatenate([images, anime_images])
        labels = np.concatenate([labels, anime_labels])
        print(f"added {len(anime_images)} anime samples")
    if width != SIZE:
        raise SystemExit(f"the model expects {SIZE}x{SIZE}, dataset is {width}x{width}")
    if args.limit:
        images, labels = images[: args.limit], labels[: args.limit]
    rng = np.random.default_rng(args.seed)
    order = rng.permutation(len(images))
    split = int(len(order) * 0.9)
    train_index, val_index = order[:split], order[split:]
    train_images = images[train_index]
    train_labels = labels[train_index]
    val_images = standardise(images[val_index])
    val_labels = labels[val_index]
    print(f"train {len(train_images)}, val {len(val_images)}")

    model = LightingNet()
    mx.eval(model.parameters())
    optimizer = optim.AdamW(learning_rate=args.lr, weight_decay=1e-4)
    loss_and_grad = nn.value_and_grad(model, loss_fn)
    best = math.inf
    steps = 0
    for epoch in range(args.epochs):
        epoch_images = train_images.copy()
        epoch_labels = train_labels.copy()
        augment(epoch_images, epoch_labels, rng)
        epoch_images = standardise(epoch_images)
        order = rng.permutation(len(epoch_images))
        total = 0.0
        for start in range(0, len(order), args.batch):
            index = order[start : start + args.batch]
            batch_images = mx.array(epoch_images[index])
            batch_labels = mx.array(epoch_labels[index])
            loss, grads = loss_and_grad(model, batch_images, batch_labels)
            optimizer.update(model, grads)
            mx.eval(model.parameters(), optimizer.state)
            total += float(loss) * len(index)
            steps += 1
        metrics = evaluate(model, val_images, val_labels)
        score = metrics["angle_deg"] + 40.0 * metrics["intensity_mae"]
        print(f"epoch {epoch + 1:02d} loss {total / len(order):.4f} "
              f"angle {metrics['angle_deg']:.1f}° p90 {metrics['angle_p90']:.1f}° "
              f"intensity {metrics['intensity_mae']:.3f} "
              f"ambient {metrics['ambient_mae']:.3f}")
        if score < best:
            best = score
            save_weights(model, args.out)
    print(f"best score {best:.2f}, steps {steps}")


if __name__ == "__main__":
    main()
