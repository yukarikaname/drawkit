# Shading proposal

The shadow/highlight model is trained separately from the light-direction estimator in
`training/lighting`. It takes a line drawing and a key direction and returns two soft masks.

## Data

- `shading_data.py` generates a controlled synthetic benchmark with exact masks. It is used only
  for architecture comparison, not as the final training distribution.
- Real pairs come from high-bookmark Pixiv process and lighting works. Download them locally into
  `real/`; the images are not committed. Keep the artist split intact so validation is by artist.

## Compare

```sh
training/lighting/.venv/bin/python training/shading/compare.py --limit 5000 --epochs 8
```

The result is written to `out/compare.json`; the table and the decision are in
`docs/shading-architecture.md`. The selected architecture is FiLM + coordinate channels + one
bottleneck self-attention block.

## Train and preview

```sh
training/lighting/.venv/bin/python training/shading/train_final.py \
    --epochs 24 --line path/to/lines.png --out out/shade.png
```

## ShadeSketch baseline

`shadesketch/` contains the local research harness for the CVPR 2020 ShadeSketch graph. Its models
and dataset are **CC BY-NC 4.0** and must not be shipped with the MIT application. They are used
only to study the teacher and to generate research comparisons.

## Runtime

The chosen model will be exported to ONNX and loaded with `tract-onnx`, the same pure-Rust runtime
already used by `crates/lineart`. The shell receives an editable layer, not a generated image.
