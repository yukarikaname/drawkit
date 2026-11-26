//! One layer as a premultiplied preview, read from the resident tile backups.

use super::*;

impl LayerStack {
    /// Downsample one layer for the palette, reading the CPU backups so a cold layer does not
    /// allocate a full-canvas texture. Four probes per output pixel on each axis keep an 8K layer
    /// affordable, and only the layer's own pixels are averaged: no stack, no paper.
    pub fn thumbnail(&self, index: usize, out_width: u32, out_height: u32) -> Vec<f32> {
        // The preview reads the CPU backup, which a live stack does not keep. Refuse instead of
        // materialising a backup on demand.
        if !self.backups {
            return Vec::new();
        }
        let Some(Some(layer)) = self.layers.get(index) else {
            return Vec::new();
        };
        if self.width == 0
            || self.height == 0
            || out_width == 0
            || out_height == 0
            || out_width > 512
            || out_height > 512
        {
            return Vec::new();
        }
        let bpp = self.bytes_per_pixel();
        let stride_x = (self.width / (out_width * 4).max(1)).max(1);
        let stride_y = (self.height / (out_height * 4).max(1)).max(1);
        let cells = out_width as usize * out_height as usize;
        let mut sums = vec![[0.0f32; 4]; cells];
        let mut counts = vec![0u32; cells];
        for y in (0..self.height).step_by(stride_y as usize) {
            let row = (y * out_height / self.height).min(out_height - 1);
            for x in (0..self.width).step_by(stride_x as usize) {
                let column = (x * out_width / self.width).min(out_width - 1);
                let pixel = self.layer_pixel(layer, x, y, bpp);
                let cell = (row * out_width + column) as usize;
                sums[cell][0] += pixel[0] * pixel[3];
                sums[cell][1] += pixel[1] * pixel[3];
                sums[cell][2] += pixel[2] * pixel[3];
                sums[cell][3] += pixel[3];
                counts[cell] += 1;
            }
        }
        let mut result = vec![0.0f32; cells * 4];
        for (cell, count) in counts.iter().enumerate() {
            let count = (*count).max(1) as f32;
            for channel in 0..4 {
                result[cell * 4 + channel] = sums[cell][channel] / count;
            }
        }
        result
    }

    /// One straight pixel from a tile backup. An eight-bit stack stores encoded bytes; a
    /// half-float stack stores straight linear Display P3, which is encoded for the palette.
    fn layer_pixel(&self, layer: &Layer, x: u32, y: u32, bpp: usize) -> [f32; 4] {
        let tx = x / self.tile_width;
        let ty = y / self.tile_height;
        let Some(tile) = layer.tiles.get(&(tx, ty)) else {
            return [0.0; 4];
        };
        let local_x = (x - tx * self.tile_width) as usize;
        let local_y = (y - ty * self.tile_height) as usize;
        let index = (local_y * self.tile_width as usize + local_x) * bpp;
        if bpp == 4 {
            let bytes = &tile.backup[index..index + 4];
            return [
                f32::from(bytes[0]) / 255.0,
                f32::from(bytes[1]) / 255.0,
                f32::from(bytes[2]) / 255.0,
                f32::from(bytes[3]) / 255.0,
            ];
        }
        let mut pixel = [0.0f32; 4];
        for (channel, value) in pixel.iter_mut().enumerate() {
            let at = index + channel * 2;
            let bits = u16::from_le_bytes([tile.backup[at], tile.backup[at + 1]]);
            *value = half::f16::from_bits(bits).to_f32();
        }
        let encoded = |value: f32| {
            let value = value.clamp(0.0, 1.0);
            if value <= 0.003_130_8 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            }
        };
        [
            encoded(pixel[0]),
            encoded(pixel[1]),
            encoded(pixel[2]),
            pixel[3].clamp(0.0, 1.0),
        ]
    }
}
