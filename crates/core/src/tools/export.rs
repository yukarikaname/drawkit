//! Layer and raster export helpers.

use super::*;

impl Editor {
    /// One layer as premultiplied linear Display P3, for an HDR layer export.
    fn linear_layer(&self, layer: &Layer, width: usize, height: usize) -> Vec<[f32; 4]> {
        let matrix = self.document.color_space.to_display_p3();
        let mut out = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let pixel = if layer.kind == LayerKind::Fill {
                    layer.fill
                } else {
                    layer.surface.get(x, y)
                };
                let alpha = pixel[3].clamp(0.0, 1.0);
                let linear = |i: usize| {
                    matrix[i][0] * self.document.color_space.decode(pixel[0])
                        + matrix[i][1] * self.document.color_space.decode(pixel[1])
                        + matrix[i][2] * self.document.color_space.decode(pixel[2])
                };
                out.push([
                    linear(0).max(0.0) * alpha,
                    linear(1).max(0.0) * alpha,
                    linear(2).max(0.0) * alpha,
                    alpha,
                ]);
            }
        }
        out
    }

    pub fn export_layers(&self, directory: &std::path::Path) -> usize {
        if std::fs::create_dir_all(directory).is_err() {
            return 0;
        }
        let (width, height) = (self.document.width(), self.document.height());
        let wide = self.document.hdr();
        let mut written = 0;
        for (index, layer) in self.document.layers.iter().enumerate() {
            if layer.reference || layer.kind == LayerKind::Group {
                continue;
            }
            let name = format!(
                "{:02}-{}.{}",
                index + 1,
                safe_name(&layer.name),
                if wide { "exr" } else { "png" }
            );
            let path = directory.join(name);
            let saved = if wide {
                let pixels = self.linear_layer(layer, width, height);
                drawkit_formats::exr::write(&path, width, height, &pixels).is_ok()
            } else {
                let encoded = match layer.kind {
                    LayerKind::Fill => {
                        crate::layer_png::encode_fill(layer.fill, width, height, false)
                    }
                    LayerKind::Paint => crate::layer_png::encode_layer(
                        &layer.surface,
                        Rect::new(0, 0, width, height),
                        false,
                    ),
                    LayerKind::Group => continue,
                };
                encoded.is_ok_and(|bytes| std::fs::write(&path, bytes).is_ok())
            };
            if saved {
                written += 1;
            }
        }
        written
    }
}
