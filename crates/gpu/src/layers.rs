//! Layers that stay on the GPU.
//!
//! The compositor used to upload every layer on every call, which is fine for a one-off comparison
//! and wrong for a canvas: a stroke touches one rectangle, not the whole stack. A `LayerStack` owns
//! one texture per layer; an edit uploads the layer that changed and the composite reads the
//! resident textures. Thumbnails, brush stamps and the compositor can all use the same textures.

use crate::composite::{Blend, FORMAT, Source, composite_sources, composite_sources_to_texture};

struct LayerTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

pub struct LayerStack {
    width: u32,
    height: u32,
    layers: Vec<Option<LayerTexture>>,
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
            self.layers[index] = Some(LayerTexture { texture, view });
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
        true
    }

    pub fn remove(&mut self, index: usize) {
        if let Some(slot) = self.layers.get_mut(index) {
            *slot = None;
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
mod tests {
    use super::*;
    use crate::Renderer;

    fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        pixel
            .iter()
            .copied()
            .cycle()
            .take((width * height * 4) as usize)
            .collect()
    }

    #[test]
    fn a_resident_stack_composites_and_updates() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut stack = LayerStack::new(2, 2);
        let red = solid(2, 2, [255, 0, 0, 255]);
        let blue = solid(2, 2, [0, 0, 255, 128]);
        assert!(stack.set(renderer.device(), renderer.queue(), 0, &red));
        assert!(stack.set(renderer.device(), renderer.queue(), 1, &blue));
        let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
        assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
        assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);

        // Change the bottom layer without touching the top one; the new pixels must be used.
        let green = solid(2, 2, [0, 255, 0, 255]);
        assert!(stack.set(renderer.device(), renderer.queue(), 0, &green));
        let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
        assert!(out[1] > 100, "the bottom layer did not update: {}", out[1]);
        assert!(out[0] < 130, "red did not fall: {}", out[0]);
    }

    #[test]
    fn a_fill_spec_composites_without_a_texture() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut stack = LayerStack::new(2, 2);
        stack.set(
            renderer.device(),
            renderer.queue(),
            0,
            &solid(2, 2, [255, 0, 0, 255]),
        );
        let mut fill = LayerSpec::new(1).fill([0.0, 0.0, 1.0, 0.5]);
        fill.opacity = 1.0;
        let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), fill]);
        assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
        assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);
    }

    #[test]
    fn a_resident_layer_can_make_its_thumbnail() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (8u32, 6u32);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let index = (y * width as usize + x) * 4;
                pixels[index] = (x * 30) as u8;
                pixels[index + 1] = (y * 40) as u8;
                pixels[index + 2] = 90;
                pixels[index + 3] = if x < 4 { 255 } else { 80 };
            }
        }
        let mut stack = LayerStack::new(width, height);
        stack.set(renderer.device(), renderer.queue(), 0, &pixels);
        let resident = renderer.layer_thumbnail(&stack, 0, 4, 3);
        let uploaded = renderer.thumbnail(&pixels, width, height, 4, 3);
        assert_eq!(resident, uploaded);
    }

    #[test]
    fn a_hidden_or_removed_layer_leaves_the_composite() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut stack = LayerStack::new(2, 2);
        stack.set(
            renderer.device(),
            renderer.queue(),
            0,
            &solid(2, 2, [255, 0, 0, 255]),
        );
        stack.set(
            renderer.device(),
            renderer.queue(),
            1,
            &solid(2, 2, [0, 0, 255, 255]),
        );
        let mut top = LayerSpec::new(1);
        top.visible = false;
        let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), top]);
        assert!(out[0] > 250 && out[2] < 5, "hidden layer showed: {out:?}");

        stack.remove(1);
        let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
        assert!(out[0] > 250 && out[2] < 5, "removed layer showed: {out:?}");
    }
}
