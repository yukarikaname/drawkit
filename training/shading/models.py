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
    raise SystemExit(f"unknown architecture {name}")
