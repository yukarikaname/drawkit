use super::*;

enum ClipPlan {
    None,
    Fill([f32; 4]),
    Tile(usize),
    Empty,
}

impl LayerStack {
    /// Composite the stack without one layer. The erase preview shows this where the stroke lands,
    /// so an eraser never has to reveal the layer it is removing.
    pub fn composite_skip(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        specs: &[LayerSpec],
        skip: usize,
    ) -> Option<wgpu::Texture> {
        let saved = self.output.take();
        let saved_dirty = std::mem::take(&mut self.dirty_tiles);
        self.output = self.output_skip.take();
        self.dirty_tiles = self.coords().collect();
        let filtered: Vec<LayerSpec> = specs
            .iter()
            .copied()
            .filter(|spec| spec.layer != skip)
            .collect();
        let result = self.composite_texture(device, queue, pipeline, &filtered);
        self.output_skip = self.output.take();
        self.output = saved;
        self.dirty_tiles = saved_dirty;
        result
    }

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
        let created = self.output.is_none();
        let output = match &self.output {
            Some(output) => output.clone(),
            None => {
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
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                self.output = Some(output.clone());
                output
            }
        };
        if created {
            self.dirty_tiles = self.coords().collect();
        } else if self.dirty_tiles.is_empty() {
            return Some(output);
        }
        let output_view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        if created {
            crate::composite::clear(&mut encoder, &output_view, "tiled composite clear");
        }
        let targets = TileTargets::new(device, queue, self.tile_width, self.tile_height);
        let mut resources = TileResources::default();
        let mut coords: Vec<_> = self.dirty_tiles.drain().collect();
        coords.sort_unstable();
        for (tx, ty) in coords {
            let mut present = Vec::with_capacity(specs.len());
            let mut clips = Vec::with_capacity(specs.len());
            for spec in specs {
                let visible = spec.visible && spec.opacity > 0.0;
                if !visible {
                    present.push(false);
                    clips.push(ClipPlan::None);
                    continue;
                }
                present.push(
                    spec.fill.is_some() || self.ensure_tile(device, queue, spec.layer, (tx, ty)),
                );
                clips.push(match spec.clip {
                    None => ClipPlan::None,
                    Some(base) => match specs.get(base) {
                        None => ClipPlan::Empty,
                        Some(spec) if !spec.visible => ClipPlan::Empty,
                        Some(spec) => {
                            if let Some(fill) = spec.fill {
                                ClipPlan::Fill(fill)
                            } else if self.ensure_tile(device, queue, base, (tx, ty)) {
                                ClipPlan::Tile(base)
                            } else {
                                ClipPlan::Empty
                            }
                        }
                    },
                });
            }
            let sources: Vec<_> = specs
                .iter()
                .zip(&present)
                .zip(&clips)
                .filter_map(|((spec, present), clip)| {
                    if !present {
                        return None;
                    }
                    let clip = match clip {
                        ClipPlan::None => None,
                        ClipPlan::Fill(fill) => Some(ClipSource::Fill(*fill)),
                        ClipPlan::Tile(base) => {
                            let view = &self.layers[*base]
                                .as_ref()?
                                .tiles
                                .get(&(tx, ty))?
                                .gpu
                                .as_ref()?
                                .view;
                            Some(ClipSource::View(view))
                        }
                        ClipPlan::Empty => return None,
                    };
                    match spec.fill {
                        Some(fill) => Some(Source {
                            view: None,
                            fill,
                            opacity: spec.opacity,
                            visible: true,
                            blend: spec.blend,
                            clip,
                        }),
                        None => {
                            let tile = self.layers[spec.layer].as_ref()?.tiles.get(&(tx, ty))?;
                            Some(Source {
                                view: tile.gpu.as_ref().map(|gpu| &gpu.view),
                                fill: [0.0; 4],
                                opacity: spec.opacity,
                                visible: true,
                                blend: spec.blend,
                                clip,
                            })
                        }
                    }
                })
                .collect();
            let texture = if sources.is_empty() {
                targets.clear_tile(&mut encoder)
            } else if let Some(texture) =
                targets.render(device, &mut encoder, pipeline, &sources, &mut resources)
            {
                texture
            } else {
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
