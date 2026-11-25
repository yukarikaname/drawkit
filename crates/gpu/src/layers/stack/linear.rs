//! Materialising half-float tiles into the CPU cache of an extended-range document.

use super::commit::LinearTile;
use super::*;

fn alpha_nonzero(bytes: &[u8]) -> bool {
    bytes[6] != 0 || bytes[7] != 0
}

impl LayerStack {
    /// Read one layer rectangle as straight linear floats and refresh its half-float backup.
    pub fn read_layer_rect_linear(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        rect: (u32, u32, u32, u32),
    ) -> Option<LinearTile> {
        let (rx, ry, rw, rh) = rect;
        let right = (rx + rw).min(self.width);
        let bottom = (ry + rh).min(self.height);
        if rx >= right || ry >= bottom {
            return None;
        }
        let (tile_width, tile_height) = (self.tile_width, self.tile_height);
        let coords: Vec<(u32, u32)> = (ry / tile_height..=(bottom - 1) / tile_height)
            .flat_map(|ty| (rx / tile_width..=(right - 1) / tile_width).map(move |tx| (tx, ty)))
            .collect();
        let width = (right - rx) as usize;
        let height = (bottom - ry) as usize;
        let mut pixels = vec![0.0f32; width * height * 4];
        for coord in coords {
            let (tx, ty) = coord;
            let left = rx.max(tx * tile_width);
            let top = ry.max(ty * tile_height);
            let span_x = right.min((tx + 1) * tile_width) - left;
            let span_y = bottom.min((ty + 1) * tile_height) - top;
            if span_x == 0 || span_y == 0 || !self.ensure_tile(device, queue, index, coord) {
                continue;
            }
            let Some(texture) = self.layers[index]
                .as_ref()
                .and_then(|layer| layer.tiles.get(&coord))
                .and_then(|tile| tile.gpu.as_ref())
                .map(|gpu| gpu.texture.clone())
            else {
                continue;
            };
            let padded_row = (span_x as usize * 8).div_ceil(256) * 256;
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linear tile materialize"),
                size: (padded_row * span_y as usize) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: left - tx * tile_width,
                        y: top - ty * tile_height,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row as u32),
                        rows_per_image: Some(span_y),
                    },
                },
                wgpu::Extent3d {
                    width: span_x,
                    height: span_y,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));
            staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let Ok(mapped) = staging.slice(..).get_mapped_range() else {
                continue;
            };
            for row in 0..span_y as usize {
                for column in 0..span_x as usize {
                    let at = row * padded_row + column * 8;
                    let mut pixel = [0.0f32; 4];
                    for (channel, value) in pixel.iter_mut().enumerate() {
                        let bits = u16::from_le_bytes([
                            mapped[at + channel * 2],
                            mapped[at + channel * 2 + 1],
                        ]);
                        *value = half::f16::from_bits(bits).to_f32();
                    }
                    let target =
                        ((top - ry) as usize + row) * width + (left - rx) as usize + column;
                    pixels[target * 4..target * 4 + 4].copy_from_slice(&pixel);
                }
            }
            self.store_backup_half(
                index,
                coord,
                (left, top, span_x, span_y),
                &mapped[..],
                padded_row,
            );
            drop(mapped);
            staging.unmap();
        }
        Some(LinearTile {
            layer: index,
            x: rx,
            y: ry,
            width: width as u32,
            height: height as u32,
            pixels,
        })
    }

    /// The half-float twin of `store_backup`, so an evicted tile rehydrates with committed pixels.
    fn store_backup_half(
        &mut self,
        index: usize,
        coord: (u32, u32),
        span: (u32, u32, u32, u32),
        mapped: &[u8],
        padded_row: usize,
    ) {
        let (left, top, span_x, span_y) = span;
        let columns = self.tile_width as usize;
        let local_x = (left - coord.0 * self.tile_width) as usize;
        let local_y = (top - coord.1 * self.tile_height) as usize;
        let Some(tile) = self.layers[index]
            .as_mut()
            .and_then(|layer| layer.tiles.get_mut(&coord))
        else {
            return;
        };
        for row in 0..span_y as usize {
            let source = row * padded_row;
            let target = ((local_y + row) * columns + local_x) * 8;
            for column in 0..span_x as usize {
                let old = &tile.backup[target + column * 8..][..8];
                let new = &mapped[source + column * 8..][..8];
                match (alpha_nonzero(old), alpha_nonzero(new)) {
                    (false, true) => tile.nonzero += 1,
                    (true, false) => tile.nonzero = tile.nonzero.saturating_sub(1),
                    _ => {}
                }
                tile.backup[target + column * 8..target + (column + 1) * 8].copy_from_slice(new);
            }
        }
        tile.stale = false;
    }
}
