# Lighting estimator

The mannequin has a three-light rig. This directory trains the small model that estimates a key
light from a shaded reference image, so the painter does not have to place the light by hand.

Everything runs locally:

- data is rendered by the Rust mannequin, not photographed or downloaded;
- training uses **MLX on the Apple Metal GPU**;
- inference is a plain Rust forward pass over the weights in
  `crates/figure/assets/lighting_weights.bin`.

## Environment

```sh
cd training/lighting
uv sync                     # creates .venv with MLX and NumPy
.venv/bin/python -c 'import mlx.core as mx; print(mx.default_device())'
```

The `.venv` lives in the repository (ignored by git); no system Python packages and no `/tmp`
training directory.

## Data

```sh
cargo run --release -p drawkit-figure --example lighting_dataset -- 6000 96
```

writes `training/lighting/data/lighting.bin`: 6000 grayscale 96x96 renders, each followed by
`key x, key y, key z, intensity, ambient, warmth`. The camera varies slightly around the front,
the body preset and pose vary, and the paper colour and brightness are randomized. The light is
the same `Lighting` used by the CPU/GPU renderers, so the label is exact.

The dataset is about 220 MB, ignored by git, and reproducible from the seed in the example.

Anime references are mostly a head, hair and cel bands, not a full mannequin. Add a synthetic
portrait set in the same format:

```sh
training/lighting/.venv/bin/python training/lighting/anime_data.py --count 8000
```

`anime_data.py` draws ellipsoid heads with hair, face marks, cel bands and an outline in NumPy.
It is still synthetic, but it puts the model in the right visual family. `train.py` concatenates
`data/lighting.bin` and `data/anime.bin` when the latter exists.

## Train

```sh
training/lighting/.venv/bin/python training/lighting/train.py
```

The network is four stride-2 convolutions (1-16-32-64-96) followed by global average pooling and
a 9-bin spatial pooling layer and a 96-64-5 MLP. It predicts a unit key direction, intensity and ambient; grayscale cannot identify colour temperature, so warmth stays a manual control. Training prints
mean and p90 angular error plus the scalar errors and keeps the best epoch.

The weights are written to `crates/figure/assets/lighting_weights.bin`. Rebuild the Rust
libraries after retraining; the file is embedded with `include_bytes!`.

## Scope

The model sees **shaded** images. A line drawing has no shading, so light direction is not
recoverable from it; the shell keeps the manual light controls and the heuristic fallback for
that case. The estimator proposes a rig, and the painter can still drag intensity or angle.
