//! Layers that stay on the GPU.
//!
//! The compositor used to upload every layer on every call, which is fine for a one-off comparison
//! and wrong for a canvas: a stroke touches one rectangle, not the whole stack. A `LayerStack` owns
//! one texture per layer; an edit uploads the layer that changed and the composite reads the
//! resident textures. Thumbnails, brush stamps and the compositor can all use the same textures.

use std::cell::{Cell, RefCell};

use crate::composite::{Blend, FORMAT, Source, composite_sources, composite_sources_to_texture};

struct LayerTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: usize,
}

pub struct LayerStack {
    width: u32,
    height: u32,
    layers: Vec<Option<LayerTexture>>,
    budget_bytes: usize,
    resident_bytes: usize,
    last_used: RefCell<Vec<u64>>,
    clock: Cell<u64>,
}

pub struct RectUpload<'a> {
    pub index: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

/// Which resident layer to composite, and how.
#[derive(Clone, Copy, Debug)]
pub struct LayerSpec {
    pub layer: usize,
    pub opacity: f32,
    pub visible: bool,
    pub blend: Blend,
    /// A fill layer supplies its colour here and has no texture.
    pub fill: Option<[f32; 4]>,
}

impl LayerSpec {
    pub fn new(layer: usize) -> Self {
        Self {
            layer,
            opacity: 1.0,
            visible: true,
            blend: Blend::Normal,
            fill: None,
        }
    }

    pub fn blend(mut self, blend: Blend) -> Self {
        self.blend = blend;
        self
    }

    pub fn fill(mut self, fill: [f32; 4]) -> Self {
        self.fill = Some(fill);
        self
    }
}

impl LayerStack {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            layers: Vec::new(),
            budget_bytes: 1 << 30,
            resident_bytes: 0,
            last_used: RefCell::new(Vec::new()),
            clock: Cell::new(0),
        }
    }

    pub fn set_budget(&mut self, bytes: usize) {
        self.budget_bytes = bytes.max(64 << 20);
        self.evict_cold(usize::MAX);
    }

    pub fn budget(&self) -> usize {
        self.budget_bytes
    }

    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }

    pub fn layer_missing(&self, index: usize) -> bool {
        self.layers.get(index).is_none_or(Option::is_none)
    }

    fn touch(&self, index: usize) {
        let mut used = self.last_used.borrow_mut();
        if used.len() <= index {
            used.resize(index + 1, 0);
        }
        let tick = self.clock.get() + 1;
        self.clock.set(tick);
        used[index] = tick;
    }

    /// Drop least-recently-used layers until the stack fits, never dropping `keep`.
    fn evict_cold(&mut self, keep: usize) {
        while self.resident_bytes > self.budget_bytes {
            let victim = (0..self.layers.len())
                .filter(|index| *index != keep)
                .filter(|index| self.layers[*index].is_some())
                .min_by_key(|index| self.last_used.borrow().get(*index).copied().unwrap_or(0));
            let Some(victim) = victim else {
                break;
            };
            if let Some(layer) = self.layers[victim].take() {
                self.resident_bytes = self.resident_bytes.saturating_sub(layer.bytes);
            }
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(Option::is_none)
    }

    /// Upload one layer's straight RGBA8 pixels. Reuses the texture when the size did not change.
    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        pixels: &[u8],
    ) -> bool {
        let expected = self.width as usize * self.height as usize * 4;
        if self.width == 0 || self.height == 0 || pixels.len() < expected {
            return false;
        }
        if self.layers.len() <= index {
            self.layers.resize_with(index + 1, || None);
        }
        if self.layers[index].is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("resident layer"),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            self.resident_bytes += expected;
            self.layers[index] = Some(LayerTexture {
                texture,
                view,
                bytes: expected,
            });
        }
        let Some(layer) = &self.layers[index] else {
            return false;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &layer.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels[..expected],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.touch(index);
        self.evict_cold(index);
        true
    }

    /// Upload one dirty rectangle of a layer. The texture stays allocated; the rest of the
    /// previous contents is untouched.
    pub fn update_rect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        request: RectUpload<'_>,
    ) -> bool {
        let RectUpload {
            index,
            x,
            y,
            width,
            height,
            pixels,
        } = request;
        if width == 0
            || height == 0
            || x + width > self.width
            || y + height > self.height
            || pixels.len() < (width * height * 4) as usize
        {
            return false;
        }
        if self.layers.get(index).is_none_or(Option::is_none)
            && !self.set(
                device,
                queue,
                index,
                &vec![0; (self.width * self.height * 4) as usize],
            )
        {
            return false;
        }
        let Some(Some(layer)) = self.layers.get(index) else {
            return false;
        };
        let row = (width * 4) as usize;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &layer.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels[..row * height as usize],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.touch(index);
        self.evict_cold(index);
        true
    }

    pub fn remove(&mut self, index: usize) {
        if let Some(slot) = self.layers.get_mut(index)
            && let Some(layer) = slot.take()
        {
            self.resident_bytes = self.resident_bytes.saturating_sub(layer.bytes);
        }
    }

    /// Composite into a texture that stays on the GPU, for the canvas surface to copy and draw.
    pub fn composite_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        specs: &[LayerSpec],
    ) -> Option<wgpu::Texture> {
        for spec in specs {
            self.touch(spec.layer);
        }
        let sources: Vec<Source<'_>> = specs
            .iter()
            .filter_map(|spec| match spec.fill {
                Some(fill) => Some(Source {
                    view: None,
                    fill,
                    opacity: spec.opacity,
                    visible: spec.visible,
                    blend: spec.blend,
                }),
                None => {
                    let layer = self.layers.get(spec.layer)?.as_ref()?;
                    Some(Source {
                        view: Some(&layer.view),
                        fill: [0.0; 4],
                        opacity: spec.opacity,
                        visible: spec.visible,
                        blend: spec.blend,
                    })
                }
            })
            .collect();
        composite_sources_to_texture(device, queue, pipeline, &sources, self.width, self.height)
    }

    /// Downsample one resident layer for the palette.
    pub fn thumbnail(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::ComputePipeline,
        index: usize,
        out_width: u32,
        out_height: u32,
    ) -> Vec<f32> {
        let Some(Some(layer)) = self.layers.get(index) else {
            return Vec::new();
        };
        crate::thumbnail::run_view(
            device,
            queue,
            pipeline,
            crate::thumbnail::ViewRequest {
                view: &layer.view,
                width: self.width,
                height: self.height,
                out_width,
                out_height,
            },
        )
    }

    pub fn composite(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        specs: &[LayerSpec],
    ) -> Vec<u8> {
        for spec in specs {
            self.touch(spec.layer);
        }
        let sources: Vec<Source<'_>> = specs
            .iter()
            .filter_map(|spec| match spec.fill {
                Some(fill) => Some(Source {
                    view: None,
                    fill,
                    opacity: spec.opacity,
                    visible: spec.visible,
                    blend: spec.blend,
                }),
                None => {
                    let layer = self.layers.get(spec.layer)?.as_ref()?;
                    Some(Source {
                        view: Some(&layer.view),
                        fill: [0.0; 4],
                        opacity: spec.opacity,
                        visible: spec.visible,
                        blend: spec.blend,
                    })
                }
            })
            .collect();
        composite_sources(device, queue, pipeline, &sources, self.width, self.height)
    }
}

#[cfg(test)]
mod tests;
