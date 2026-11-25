use super::*;

enum ClipPlan {
    None,
    Fill([f32; 4]),
    Tile(usize),
    Group(usize),
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
        // Keep the indices stable: parent and clip are positions in this array. Hiding the layer
        // is enough, and a hidden group hides its subtree.
        let mut filtered = specs.to_vec();
        for spec in &mut filtered {
            if spec.layer == skip {
                spec.visible = false;
            }
        }
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
                    format: self.format.texture(),
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
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); specs.len()];
        for (index, spec) in specs.iter().enumerate() {
            if let Some(parent) = spec.parent
                && parent < specs.len()
                && parent != index
            {
                groups[parent].push(index);
            }
        }
        let mut group_slots = vec![None; specs.len()];
        let mut slot = 1usize;
        for (index, children) in groups.iter().enumerate() {
            if !children.is_empty() {
                group_slots[index] = Some(slot);
                slot += 1;
            }
        }
        let roots: Vec<usize> = specs
            .iter()
            .enumerate()
            .filter(|(index, spec)| {
                spec.parent
                    .is_none_or(|parent| parent >= specs.len() || parent == *index)
            })
            .map(|(index, _)| index)
            .collect();
        let targets = TileTargets::new(
            device,
            queue,
            self.tile_width,
            self.tile_height,
            self.format.texture(),
            slot,
        );
        let mut resources = TileResources::default();
        let mut coords: Vec<_> = self.dirty_tiles.drain().collect();
        coords.sort_unstable();
        for coord in coords {
            // Materialise every leaf and clip base this tile needs before the immutable borrow
            // of the layer list the recursive composite takes.
            for (index, spec) in specs.iter().enumerate() {
                if !spec.visible || spec.opacity <= 0.0 || group_slots[index].is_some() {
                    continue;
                }
                if spec.fill.is_none() {
                    self.ensure_tile(device, queue, spec.layer, coord);
                }
                if let Some(base) = spec.clip
                    && base < specs.len()
                    && group_slots[base].is_none()
                    && specs[base].fill.is_none()
                {
                    self.ensure_tile(device, queue, base, coord);
                }
            }
            let rendered = render_list(
                &roots,
                0,
                0,
                specs,
                &groups,
                &group_slots,
                &self.layers,
                &targets,
                coord,
                device,
                &mut encoder,
                pipeline,
                &mut resources,
            );
            let Some((texture, _)) = rendered else {
                let Some((texture, _)) = targets.clear(0, &mut encoder) else {
                    continue;
                };
                copy_tile(
                    &mut encoder,
                    texture,
                    &output,
                    coord,
                    (self.width, self.height),
                    (self.tile_width, self.tile_height),
                );
                continue;
            };
            copy_tile(
                &mut encoder,
                texture,
                &output,
                coord,
                (self.width, self.height),
                (self.tile_width, self.tile_height),
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
        // The CPU walk reads eight-bit backups; an extended-range stack has none.
        if self.format != crate::FrameFormat::Rgba8 {
            return Vec::new();
        }
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

fn clip_plan(spec: &LayerSpec, specs: &[LayerSpec], group_slots: &[Option<usize>]) -> ClipPlan {
    let Some(base) = spec.clip else {
        return ClipPlan::None;
    };
    let Some(base_spec) = specs.get(base) else {
        return ClipPlan::Empty;
    };
    if !base_spec.visible || base_spec.opacity <= 0.0 {
        return ClipPlan::Empty;
    }
    if group_slots.get(base).copied().flatten().is_some() {
        return ClipPlan::Group(base);
    }
    match base_spec.fill {
        Some(fill) => ClipPlan::Fill(fill),
        None => ClipPlan::Tile(base),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_list<'a>(
    indices: &[usize],
    slot: usize,
    depth: usize,
    specs: &[LayerSpec],
    groups: &[Vec<usize>],
    group_slots: &[Option<usize>],
    layers: &'a [Option<Layer>],
    targets: &'a TileTargets,
    coord: (u32, u32),
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    resources: &mut TileResources,
) -> Option<(&'a wgpu::Texture, &'a wgpu::TextureView)> {
    // A corrupted parent chain must not recurse forever.
    if depth > 32 {
        return None;
    }
    let mut sources: Vec<Source<'a>> = Vec::new();
    for &index in indices {
        let Some(spec) = specs.get(index) else {
            continue;
        };
        if !spec.visible || spec.opacity <= 0.0 {
            continue;
        }
        let clip = match clip_plan(spec, specs, group_slots) {
            ClipPlan::None => None,
            ClipPlan::Fill(fill) => Some(ClipSource::Fill(fill)),
            ClipPlan::Tile(base) => {
                let Some(view) = layers
                    .get(base)
                    .and_then(Option::as_ref)
                    .and_then(|layer| layer.tiles.get(&coord))
                    .and_then(|tile| tile.gpu.as_ref())
                    .map(|gpu| &gpu.view)
                else {
                    continue;
                };
                Some(ClipSource::View(view))
            }
            ClipPlan::Group(base) => {
                let Some(view) = group_slots
                    .get(base)
                    .copied()
                    .flatten()
                    .and_then(|slot| targets.view(slot))
                else {
                    continue;
                };
                Some(ClipSource::View(view))
            }
            ClipPlan::Empty => continue,
        };
        if let Some(child_slot) = group_slots.get(index).copied().flatten() {
            let Some((_, view)) = render_list(
                &groups[index],
                child_slot,
                depth + 1,
                specs,
                groups,
                group_slots,
                layers,
                targets,
                coord,
                device,
                encoder,
                pipeline,
                resources,
            ) else {
                continue;
            };
            sources.push(Source {
                view: Some(view),
                fill: [0.0; 4],
                opacity: spec.opacity,
                visible: true,
                blend: spec.blend,
                clip,
            });
            continue;
        }
        if let Some(fill) = spec.fill {
            sources.push(Source {
                view: None,
                fill,
                opacity: spec.opacity,
                visible: true,
                blend: spec.blend,
                clip,
            });
            continue;
        }
        let Some(tile) = layers
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|layer| layer.tiles.get(&coord))
            .and_then(|tile| tile.gpu.as_ref())
        else {
            continue;
        };
        sources.push(Source {
            view: Some(&tile.view),
            fill: [0.0; 4],
            opacity: spec.opacity,
            visible: true,
            blend: spec.blend,
            clip,
        });
    }
    targets.render(slot, device, encoder, pipeline, &sources, resources)
}

fn copy_tile(
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
    output: &wgpu::Texture,
    coord: (u32, u32),
    canvas: (u32, u32),
    tile: (u32, u32),
) {
    let (tx, ty) = coord;
    let width = (canvas.0 - tx * tile.0).min(tile.0);
    let height = (canvas.1 - ty * tile.1).min(tile.1);
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: output,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: tx * tile.0,
                y: ty * tile.1,
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
