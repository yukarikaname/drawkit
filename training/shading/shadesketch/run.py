#!/usr/bin/env python3
"""Run the ShadeSketch lineshader graph on a line drawing and save its shadow layer.

The release models are CC BY-NC 4.0; this is a local research baseline, not a shippable weight.
"""

import argparse
from pathlib import Path

import cv2
import numpy as np

import tensorflow as tf

tf.compat.v1.disable_v2_behavior()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--direction", default="-1,1,-1")
    parser.add_argument("--size", type=int, default=320)
    args = parser.parse_args()
    line = cv2.imread(args.input, cv2.IMREAD_GRAYSCALE)
    if line is None:
        raise SystemExit(f"cannot read {args.input}")
    line = cv2.resize(line, (args.size, args.size))
    _, line = cv2.threshold(line, 200, 255, cv2.THRESH_BINARY)
    ink = line.astype(np.float32)[None, :, :, None] / 255.0
    direction = np.array([float(v) for v in args.direction.split(",")], dtype=np.float32)[None, :]

    graph = tf.compat.v1.GraphDef()
    with open("training/shading/shadesketch/models/lineshader.pb", "rb") as file:
        graph.ParseFromString(file.read())
    tf.import_graph_def(graph, name="lineShader")
    with tf.compat.v1.Session() as session:
        raw = session.run(
            "lineShader/conv2d_139/Tanh:0",
            {
                "lineShader/input_1:0": direction,
                "lineShader/input_2:0": 1.0 - ink,
            },
        )
    raw = np.squeeze(raw)
    shadow = (1.0 - (raw + 1.0) / 2.0)
    shadow = np.clip(shadow, 0.0, 1.0)
    shadow = cv2.resize(shadow, (line.shape[1], line.shape[0])) if False else shadow
    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    cv2.imwrite(str(out), (shadow * 255).astype(np.uint8))
    # A readable composite: paper, ink, and the proposed shadow in violet.
    paper = np.ones((*shadow.shape, 3), dtype=np.float32)
    ink3 = ink[0, :, :, 0]
    paper *= 1.0 - ink3[..., None]
    shadow_rgb = np.array([0.42, 0.36, 0.62], dtype=np.float32)
    paper = paper * (1.0 - shadow[..., None] * 0.55) + shadow_rgb * shadow[..., None] * 0.55
    cv2.imwrite(str(out.with_name(out.stem + "_comp.png")),
                cv2.cvtColor(np.clip(paper * 255, 0, 255).astype(np.uint8), cv2.COLOR_RGB2BGR))
    print("raw", raw.min(), raw.max(), "shadow mean", shadow.mean())


if __name__ == "__main__":
    main()
