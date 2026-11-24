use super::*;

impl LayerStack {
    /// Composite into a texture that stays on the GPU, one tile at a time.
    pub fn composite_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        specs: &[LayerSpec],
    ) -> Option<wgpu::Texture> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let mut coords = HashSet::new();
        let mut has_fill = false;
        for spec in specs {
            if !spec.visible || spec.opacity <= 0.0 {
                continue;
            }
            if spec.fill.is_some() {
                has_fill = true;
            } else if let Some(layer) = self.layers.get(spec.layer).and_then(Option::as_ref) {
                coords.extend(layer.tiles.keys().copied());
            }
        }
        if has_fill {
            coords.extend(self.coords());
        }
        if coords.is_empty() {
            return None;
        }
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tiled composite"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        crate::composite::clear(&mut encoder, &output_view, "tiled composite clear");
        let targets = TileTargets::new(device, queue, self.tile_width, self.tile_height);
        let mut resources = TileResources::default();
        let mut coords: Vec<_> = coords.into_iter().collect();
        coords.sort_unstable();
        for (tx, ty) in coords {
            let mut present = Vec::with_capacity(specs.len());
            for spec in specs {
                if !spec.visible || spec.opacity <= 0.0 {
                    present.push(false);
                } else if spec.fill.is_some() {
                    present.push(true);
                } else {
                    present.push(self.ensure_tile(device, queue, spec.layer, (tx, ty)));
                }
            }
            let sources: Vec<_> = specs
                .iter()
                .zip(&present)
                .filter_map(|(spec, present)| {
                    if !present {
                        return None;
                    }
                    match spec.fill {
                        Some(fill) => Some(Source {
                            view: None,
                            fill,
                            opacity: spec.opacity,
                            visible: true,
                            blend: spec.blend,
                        }),
                        None => {
                            let tile = self.layers[spec.layer].as_ref()?.tiles.get(&(tx, ty))?;
                            Some(Source {
                                view: tile.gpu.as_ref().map(|gpu| &gpu.view),
                                fill: [0.0; 4],
                                opacity: spec.opacity,
                                visible: true,
                                blend: spec.blend,
                            })
                        }
                    }
                })
                .collect();
            let Some(texture) =
                targets.render(device, &mut encoder, pipeline, &sources, &mut resources)
            else {
                continue;
            };
            let width = (self.width - tx * self.tile_width).min(self.tile_width);
            let height = (self.height - ty * self.tile_height).min(self.tile_height);
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: tx * self.tile_width,
                        y: ty * self.tile_height,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        queue.submit(Some(encoder.finish()));
        drop(resources);
        Some(output)
    }

    /// Downsample one layer for the palette, reading the CPU backups so a cold layer does not
    /// allocate a full-canvas texture.
    pub fn thumbnail(&self, index: usize, out_width: u32, out_height: u32) -> Vec<f32> {
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
        let mut result = vec![0.0f32; out_width as usize * out_height as usize * 4];
        for oy in 0..out_height {
            let top = oy * self.height / out_height;
            let bottom = ((oy + 1) * self.height / out_height).max(top + 1);
            for ox in 0..out_width {
                let left = ox * self.width / out_width;
                let right = ((ox + 1) * self.width / out_width).max(left + 1);
                let mut sum = [0.0f32; 4];
                let mut count = 0.0f32;
                for y in top..bottom {
                    for x in left..right {
                        let pixel = self.layer_pixel(layer, x, y);
                        let alpha = pixel[3] as f32 / 255.0;
                        sum[0] += pixel[0] as f32 / 255.0 * alpha;
                        sum[1] += pixel[1] as f32 / 255.0 * alpha;
                        sum[2] += pixel[2] as f32 / 255.0 * alpha;
                        sum[3] += alpha;
                        count += 1.0;
                    }
                }
                let target = (oy as usize * out_width as usize + ox as usize) * 4;
                for channel in 0..4 {
                    result[target + channel] = sum[channel] / count.max(1.0);
                }
            }
        }
        result
    }

    fn layer_pixel(&self, layer: &Layer, x: u32, y: u32) -> [u8; 4] {
        let tx = x / self.tile_width;
        let ty = y / self.tile_height;
        let Some(tile) = layer.tiles.get(&(tx, ty)) else {
            return [0; 4];
        };
        let local_x = (x - tx * self.tile_width) as usize;
        let local_y = (y - ty * self.tile_height) as usize;
        let index = (local_y * self.tile_width as usize + local_x) * 4;
        [
            tile.backup[index],
            tile.backup[index + 1],
            tile.backup[index + 2],
            tile.backup[index + 3],
        ]
    }

    pub fn composite(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        specs: &[LayerSpec],
    ) -> Vec<u8> {
        match self.composite_texture(device, queue, pipeline, specs) {
            Some(texture) => {
                crate::readback::read_texture(device, queue, &texture, self.width, self.height)
            }
            None => vec![0; self.width as usize * self.height as usize * 4],
        }
    }
}
