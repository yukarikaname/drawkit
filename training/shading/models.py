"""Candidate architectures for line-art shadow/highlight prediction."""

import mlx.core as mx
import mlx.nn as nn


def upsample(x):
    return mx.repeat(mx.repeat(x, 2, axis=1), 2, axis=2)


def coordinates(batch, size):
    grid = mx.arange(size, dtype=mx.float32) / max(size - 1, 1) * 2.0 - 1.0
    x = mx.broadcast_to(grid[None, None, :, None], (batch, size, size, 1))
    y = mx.broadcast_to(grid[None, :, None, None], (batch, size, size, 1))
    return mx.concatenate([x, y], axis=-1)


def inputs(line, light, use_coordinates):
    batch = line.shape[0]
    if use_coordinates:
        line = mx.concatenate([line, coordinates(batch, line.shape[1])], axis=-1)
    else:
        planes = mx.broadcast_to(light[:, None, None, :], (batch, line.shape[1], line.shape[2], 3))
        line = mx.concatenate([line, planes], axis=-1)
    return line


class FiLM(nn.Module):
    def __init__(self, channels):
        super().__init__()
        self.proj = nn.Linear(3, channels * 2)

    def __call__(self, x, light):
        values = self.proj(light)
        gamma = values[:, : x.shape[-1]][:, None, None, :]
        beta = values[:, x.shape[-1] :][:, None, None, :]
        return x * (1.0 + gamma) + beta


class SelfAttention(nn.Module):
    def __init__(self, channels):
        super().__init__()
        self.qkv = nn.Linear(channels, channels * 3)
        self.out = nn.Linear(channels, channels)

    def __call__(self, x):
        batch, height, width, channels = x.shape
        qkv = self.qkv(x).reshape(batch, height * width, 3, channels)
        q = qkv[:, :, 0, :]
        k = qkv[:, :, 1, :]
        v = qkv[:, :, 2, :]
        attention = mx.softmax(mx.matmul(q, k.transpose(0, 2, 1)) / (channels ** 0.5), axis=-1)
        out = mx.matmul(attention, v).reshape(batch, height, width, channels)
        return x + self.out(out)


class UNet(nn.Module):
    """Baseline: the light direction is three constant planes, as in the first prototype."""

    def __init__(self):
        super().__init__()
        self.c1 = nn.Conv2d(4, 16, 3, stride=2, padding=1)
        self.c2 = nn.Conv2d(16, 32, 3, stride=2, padding=1)
        self.c3 = nn.Conv2d(32, 64, 3, stride=2, padding=1)
        self.u2 = nn.Conv2d(64 + 32, 64, 3, padding=1)
        self.u1 = nn.Conv2d(64 + 16, 32, 3, padding=1)
        self.u0 = nn.Conv2d(32, 16, 3, padding=1)
        self.out = nn.Conv2d(16, 2, 1)

    def __call__(self, line, light):
        x = inputs(line, light, False)
        e1 = nn.relu(self.c1(x))
        e2 = nn.relu(self.c2(e1))
        bottom = nn.relu(self.c3(e2))
        up = nn.relu(self.u2(mx.concatenate([upsample(bottom), e2], axis=-1)))
        up = nn.relu(self.u1(mx.concatenate([upsample(up), e1], axis=-1)))
        up = nn.relu(self.u0(upsample(up)))
        return self.out(up)


class FiLMUNet(nn.Module):
    """Line + coordinate channels; the light direction modulates every block through FiLM."""

    def __init__(self, attention=False, coordinates=True):
        super().__init__()
        self.coordinates = coordinates
        channels = 3 if coordinates else 1
        self.c1 = nn.Conv2d(channels, 16, 3, stride=2, padding=1)
        self.c2 = nn.Conv2d(16, 32, 3, stride=2, padding=1)
        self.c3 = nn.Conv2d(32, 64, 3, stride=2, padding=1)
        self.u2 = nn.Conv2d(64 + 32, 64, 3, padding=1)
        self.u1 = nn.Conv2d(64 + 16, 32, 3, padding=1)
        self.u0 = nn.Conv2d(32, 16, 3, padding=1)
        self.out = nn.Conv2d(16, 2, 1)
        self.film1 = FiLM(16)
        self.film2 = FiLM(32)
        self.film3 = FiLM(64)
        self.film4 = FiLM(64)
        self.film5 = FiLM(32)
        self.film6 = FiLM(16)
        self.attention = SelfAttention(64) if attention else None

    def __call__(self, line, light):
        x = line
        if self.coordinates:
            x = mx.concatenate([line, coordinates(line.shape[0], line.shape[1])], axis=-1)
        e1 = nn.relu(self.film1(self.c1(x), light))
        e2 = nn.relu(self.film2(self.c2(e1), light))
        bottom = nn.relu(self.film3(self.c3(e2), light))
        if self.attention is not None:
            bottom = self.attention(bottom)
        up = nn.relu(self.film4(self.u2(mx.concatenate([upsample(bottom), e2], axis=-1)), light))
        up = nn.relu(self.film5(self.u1(mx.concatenate([upsample(up), e1], axis=-1)), light))
        up = nn.relu(self.film6(self.u0(upsample(up)), light))
        return self.out(up)


def build(name):
    if name == "unet":
        return UNet()
    if name == "film":
        return FiLMUNet(attention=False, coordinates=True)
    if name == "film_nocoord":
        return FiLMUNet(attention=False, coordinates=False)
    if name == "attention":
        return FiLMUNet(attention=True, coordinates=True)
    if name == "segformer":
        return SegFormerLite()
    raise SystemExit(f"unknown architecture {name}")


class TransformerBlock(nn.Module):
    """A small pre-norm transformer block with a depthwise Mix-FFN."""

    def __init__(self, channels):
        super().__init__()
        self.norm1 = nn.LayerNorm(channels)
        self.qkv = nn.Linear(channels, channels * 3)
        self.proj = nn.Linear(channels, channels)
        self.norm2 = nn.LayerNorm(channels)
        self.fc1 = nn.Linear(channels, channels * 4)
        self.depthwise = nn.Conv2d(channels * 4, channels * 4, 3, padding=1,
                                   groups=channels * 4)
        self.fc2 = nn.Linear(channels * 4, channels)

    def __call__(self, x):
        batch, height, width, channels = x.shape
        token = self.norm1(x).reshape(batch, height * width, channels)
        qkv = self.qkv(token).reshape(batch, height * width, 3, channels)
        q, k, v = qkv[:, :, 0], qkv[:, :, 1], qkv[:, :, 2]
        attention = mx.softmax(mx.matmul(q, k.transpose(0, 2, 1)) / (channels ** 0.5), axis=-1)
        token = self.proj(mx.matmul(attention, v)).reshape(batch, height, width, channels)
        x = x + token
        y = self.fc1(self.norm2(x))
        y = self.fc2(self.depthwise(y))
        return x + y


class SegFormerLite(nn.Module):
    """Hierarchical transformer encoder with a small CNN decoder.

    Three stages (24x24, 12x12, 6x6) keep enough spatial detail for a 96x96 mask. The light
    direction is a learned token added to every stage; the decoder fuses all three scales.
    """

    def __init__(self):
        super().__init__()
        self.patch = nn.Conv2d(3, 32, 7, stride=4, padding=3)
        self.down2 = nn.Conv2d(32, 64, 3, stride=2, padding=1)
        self.down3 = nn.Conv2d(64, 128, 3, stride=2, padding=1)
        self.block1 = TransformerBlock(32)
        self.block2 = TransformerBlock(64)
        self.block3 = TransformerBlock(128)
        self.light1 = nn.Linear(3, 32)
        self.light2 = nn.Linear(3, 64)
        self.light3 = nn.Linear(3, 128)
        self.project1 = nn.Conv2d(32, 64, 1)
        self.project2 = nn.Conv2d(64, 64, 1)
        self.project3 = nn.Conv2d(128, 64, 1)
        self.fuse = nn.Conv2d(64 * 3, 64, 3, padding=1)
        self.refine = nn.Conv2d(64, 64, 3, padding=1)
        self.head = nn.Conv2d(64, 2, 1)

    def __call__(self, line, light):
        x = mx.concatenate([line, coordinates(line.shape[0], line.shape[1])], axis=-1)
        x = nn.relu(self.patch(x))
        x = self.block1(x + self.light1(light)[:, None, None, :])
        x2 = nn.relu(self.down2(x))
        x2 = self.block2(x2 + self.light2(light)[:, None, None, :])
        x3 = nn.relu(self.down3(x2))
        x3 = self.block3(x3 + self.light3(light)[:, None, None, :])
        p1 = self.project1(x)
        p2 = self.project2(upsample(x2))
        p3 = self.project3(upsample(upsample(x3)))
        fused = nn.relu(self.fuse(mx.concatenate([p1, p2, p3], axis=-1)))
        fused = nn.relu(self.refine(fused))
        fused = upsample(upsample(fused))
        return self.head(fused)
