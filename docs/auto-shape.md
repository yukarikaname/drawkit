# Auto Shape: a tiny model against the ruler

Auto Shape replaces a finished stroke with an ideal line, ellipse or rectangle when the model is
confident. Freehand is left alone, the feature is off by default, and a replacement is one undo
step. The question was not whether a neural net is fashionable, but whether it beats the
traditional geometry a drawing app would otherwise write.

## The model

A 11-16-4 tanh network, eleven geometric features, one hidden layer, about three hundred floats.
The features are aspect, closure, straightness, corner fraction, total turning, fill, radius
variance, direction reversals, path-to-bounding-box ratio, axis alignment and the share of turning
concentrated in the four sharpest corners. The model runs in `crates/core/src/shape.rs`; there is no
ONNX runtime and no network.

## Training

`scripts/train_shape_model.py` generates noisy lines (some slightly bowed), full and near-full
ellipses, sharp and rounded rectangles, and freehand splines, zigzags, spirals, arcs and wobbles.
It trains with scikit-learn and writes `target/shape_weights.npz`.

```sh
uv run --with numpy --with scikit-learn python scripts/train_shape_model.py
uv run --with numpy --with scikit-learn python scripts/evaluate_shape_model.py
```

The evaluation set is deliberately harder than the training distribution: bowed lines, rounded
rectangles, partial ellipses labelled free, and noisier freehand.

## Measured on the hard set (2,000 strokes)

| method | accuracy | macro F1 |
|---|---:|---:|
| traditional geometry (PCA line fit, RDP corners, polygon fill) | 61.95% | 0.537 |
| logistic regression | 83.70% | 0.835 |
| decision tree, depth 6 | 84.95% | 0.852 |
| gradient boosting | 87.05% | 0.873 |
| k-nearest neighbours (k=5) | 87.95% | 0.879 |
| **committed 11-16-4 MLP** | **89.45%** | **0.893** |

Raw accuracy is not the whole story for a tool that changes the painter's stroke. The model only
replaces when its confidence is high:

| confidence | strokes replaced | precision |
|---:|---:|---:|
| 0.7 | 1276 / 2000 | 96.3% |
| **0.8 (shipped)** | **960 / 2000** | **97.9%** |
| 0.9 | 656 / 2000 | 98.3% |

So on a hard distribution it edits about half the strokes and is wrong about one in fifty; the
remaining strokes stay untouched.

The Rust classifier costs **1.15 µs per call** for a 32-point stroke (`cargo run --release
-p drawkit-core --example bench_shape`).

## Why this model

The traditional recogniser is fast and explainable, but square-ish rectangles collapse the PCA
axes, noise inflates RDP corner counts, and partial ellipses look like freehand. It gets 62% on the
hard set because every threshold has to cover two neighbouring classes at once. kNN is close but
stores the training set and has a variable-time neighbourhood walk; gradient boosting is larger and
needs a runtime. The 11-16-4 MLP is the best measured accuracy that still fits in a few hundred
floats, runs in constant time and needs no dependency.

## Limits

- A partial arc is labelled `Free`; Auto Shape does not invent a closed ellipse from an arc.
- The fitted ellipse is axis-aligned in the stroke's bounding box; it does not recover the
  original rotation of a drawn ellipse.
- A rectangle is fitted to the bounding box, not an oriented minimum-area rectangle.
- The model is a suggestion, not a replacement for undo or the off switch.

The training and evaluation scripts are the source of the weights; if the feature code changes,
retrain and re-run the hard-set comparison before trusting the numbers.
