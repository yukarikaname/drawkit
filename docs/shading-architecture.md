# Shading model: architecture selection

The goal is not a generated image. The model takes line art and a key-light direction and proposes
two soft masks, one for shadow and one for highlight. The shell paints those masks into an editable
layer. A wrong proposal is erased; it never becomes unrecoverable pixels.

## Candidates

| model | shape | notes |
|---|---|---|
| U-Net baseline | 4-channel input (line + three constant light planes) | smallest, easy to export |
| U-Net + FiLM | line + coordinate channels; light modulates each block through FiLM | the closest cheap analogue of ShadeSketch's conditioning |
| U-Net + FiLM, no coordinates | as above, without x/y channels | tests whether position matters |
| U-Net + FiLM + attention | one self-attention block at the 12x12 bottleneck | hybrid CNN/Transformer |
| SegFormer-lite | three-stage hierarchical Transformer (24x24, 12x12, 6x6) with a CNN decoder | full Transformer experiment, trained on Metal |
| ShadeSketch | conditional GAN generator: FiLM, coordinate channels, 5 self-attention blocks, 162 convolutions | strong teacher, 39 MB TensorFlow graph, CC BY-NC 4.0 |
| SmartShadow | hierarchical CNNs plus interactive shadow brushes | strong research tool, no public inference model |

ShadeSketch is not a U-Net. It keeps the input resolution with convolutions, injects the light
direction through FiLM, adds coordinate channels, and uses self-attention. That is why the hybrid
candidate below copies those three ideas but keeps the small encoder-decoder.

## Benchmark

The controlled benchmark is 5,000 samples from `training/shading/shading_data.py`: synthetic
line-art heads, shadow/highlight masks from the same ellipsoid normals, and a random key direction.
Every architecture sees the same split, optimizer, batch size and eight epochs. This benchmark is
for **relative architecture ranking**, not for claiming real-world anime quality; the real-data
evaluation is separate.

| architecture | shadow Dice | highlight Dice |
|---|---:|---:|
| U-Net baseline | 0.921 | 0.749 |
| FiLM, no coordinates | 0.921 | 0.748 |
| FiLM + coordinates | 0.928 | 0.762 |
| **FiLM + coordinates + bottleneck attention** | **0.952** | **0.858** |
| SegFormer-lite, 8 epochs | 0.877 | 0.792 |
| SegFormer-lite, 20 epochs | 0.881 | 0.803 |

## Choice

The hybrid wins by a clear margin. The result matches the ShadeSketch lesson: global relationships
matter, but a full Transformer is not needed for a soft two-channel mask. A single self-attention
block at the lowest resolution is enough to let the face/hair regions exchange context, while the
CNN keeps local line detail and stays cheap enough for ONNX and tablets.

The hierarchical Transformer was trained on the same Metal GPU and data. It improved with a longer
schedule but plateaued below the hybrid. With 5k synthetic samples and no large-scale pretraining,
the Transformer is data-hungry; a pretrained Swin/SegFormer backbone would be the next thing to
test if the real paired set reaches tens of thousands of examples. It is not the current choice.

The production target is therefore:

```text
line + x/y coordinates  ->  U-Net encoder
light direction         ->  FiLM on every block
12x12 feature           ->  one self-attention block
                         ->  U-Net decoder -> shadow / highlight logits
```

Next steps:

- build the training set from high-bookmark Pixiv process/lighting works, split by artist;
- export the chosen model to ONNX and run it through `tract-onnx` in Rust;
- keep ShadeSketch as a non-shipping teacher and reject its CC BY-NC weights from release builds;
- reject DiT/diffusion for this task: the output is a mask, not an image, and the latency and
  non-determinism do not fit the canvas.
