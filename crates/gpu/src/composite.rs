//! GPU layer compositing.
//!
//! The document is still a CPU value in this slice of the work; what moves to the GPU is the
//! per-pixel loop that combines the layers. One full-screen pass per layer, ping-ponging between two
//! premultiplied targets, then one readback. The pass implements every blend mode the core knows,
//! which a single hardware-alpha target could not do: a fragment shader cannot read the colour it
//! is painting over.

use wgpu::util::DeviceExt;

mod tile;

pub use tile::{TileResources, TileTargets};

/// The blend mode numbering matches `drawkit_core::document::BlendMode::as_i32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Darken,
    Lighten,
    Color,
    Luminosity,
}

impl Blend {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Overlay,
            4 => Self::Add,
            5 => Self::Darken,
            6 => Self::Lighten,
            7 => Self::Color,
            8 => Self::Luminosity,
            _ => Self::Normal,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
            Self::Screen => 2,
            Self::Overlay => 3,
            Self::Add => 4,
            Self::Darken => 5,
            Self::Lighten => 6,
            Self::Color => 7,
            Self::Luminosity => 8,
        }
    }
}

/// A layer as the compositor sees it: straight RGBA8, an opacity multiplier and a blend mode.
pub struct Layer<'a> {
    pub pixels: &'a [u8],
    pub opacity: f32,
    pub visible: bool,
    pub blend: Blend,
}

impl<'a> Layer<'a> {
    pub fn new(pixels: &'a [u8], opacity: f32) -> Self {
        Self {
            pixels,
            opacity,
            visible: true,
            blend: Blend::Normal,
        }
    }

    pub fn blended(mut self, blend: Blend) -> Self {
        self.blend = blend;
        self
    }
}

pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    opacity: f32,
    mode: u32,
    use_fill: u32,
    use_clip: u32,
    fill: [f32; 4],
    clip_fill: [f32; 4],
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    pipeline_for(device, FORMAT)
}

/// The same pipeline, rendering into `format`. A resident extended-range stack composites into
/// `Rgba16Float` so highlights above paper white survive the per-layer passes.
pub fn pipeline_for(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("composite"),
        source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("composite"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                // The shader returns premultiplied colour and the formula already accounts for
                // the backdrop, so hardware blending stays off.
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn target(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(crate) fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, label: &str) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

/// One layer already on the GPU. A resident layer stack uses this to composite without uploading
/// the pixels again.
pub enum ClipSource<'a> {
    View(&'a wgpu::TextureView),
    Fill([f32; 4]),
}

pub struct Source<'a> {
    /// `None` is a fill layer: the shader uses `fill` instead of sampling a texture.
    pub view: Option<&'a wgpu::TextureView>,
    pub fill: [f32; 4],
    pub opacity: f32,
    pub visible: bool,
    pub blend: Blend,
    /// The layer's alpha is multiplied by this source.
    pub clip: Option<ClipSource<'a>>,
}

/// Composite textures that are already resident. `sources` is bottom layer first.
pub fn composite_sources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    sources: &[Source<'_>],
    width: u32,
    height: u32,
) -> Vec<u8> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Vec::new();
    }
    match composite_sources_to_texture(device, queue, pipeline, sources, width, height) {
        Some(texture) => crate::readback::read_texture(device, queue, &texture, width, height),
        None => vec![0; width as usize * height as usize * 4],
    }
}

/// The same passes, but the result stays on the GPU and is returned as a texture. The canvas
/// surface copies it into its painting texture instead of reading it back.
pub fn composite_sources_to_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    sources: &[Source<'_>],
    width: u32,
    height: u32,
) -> Option<wgpu::Texture> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return None;
    }
    let first = target(device, "composite a", width, height, FORMAT);
    let second = target(device, "composite b", width, height, FORMAT);
    let first_view = first.create_view(&Default::default());
    let second_view = second.create_view(&Default::default());
    let fallback = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("composite fill"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &fallback,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[0, 0, 0, 0],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let fallback_view = fallback.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    clear(&mut encoder, &first_view, "composite clear");

    let layout = pipeline.get_bind_group_layout(0);
    let mut resources = Vec::new();
    let mut passes = 0;
    for source in sources {
        if !source.visible || source.opacity <= 0.0 {
            continue;
        }
        let (use_clip, clip_fill) = match source.clip {
            Some(ClipSource::Fill(fill)) => (2, fill),
            Some(ClipSource::View(_)) => (1, [0.0; 4]),
            None => (0, [0.0; 4]),
        };
        let params = Params {
            opacity: source.opacity.clamp(0.0, 1.0),
            mode: source.blend.as_u32(),
            use_fill: u32::from(source.view.is_none()),
            use_clip,
            fill: source.fill,
            clip_fill,
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("composite params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view = source.view.unwrap_or(&fallback_view);
        let (backdrop, destination) = if passes % 2 == 0 {
            (&first_view, &second_view)
        } else {
            (&second_view, &first_view)
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite layer"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(match &source.clip {
                        Some(ClipSource::View(view)) => view,
                        _ => &fallback_view,
                    }),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: destination,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        passes += 1;
        resources.push((uniform, bind_group));
    }
    queue.submit(Some(encoder.finish()));
    if passes == 0 {
        return None;
    }
    let last = if passes % 2 == 0 { first } else { second };
    drop(resources);
    drop(fallback);
    Some(last)
}

pub fn composite(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    layers: &[Layer<'_>],
    width: u32,
    height: u32,
) -> Vec<u8> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Vec::new();
    }
    let expected = width as usize * height as usize * 4;
    let mut prepared = Vec::new();
    for layer in layers {
        if !layer.visible || layer.opacity <= 0.0 || layer.pixels.len() < expected {
            continue;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("composite layer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            layer.pixels,
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
        let view = texture.create_view(&Default::default());
        prepared.push((texture, view, layer.opacity, layer.blend));
    }
    let sources: Vec<Source<'_>> = prepared
        .iter()
        .map(|(_, view, opacity, blend)| Source {
            view: Some(view),
            fill: [0.0; 4],
            opacity: *opacity,
            visible: true,
            blend: *blend,
            clip: None,
        })
        .collect();
    let pixels = composite_sources(device, queue, pipeline, &sources, width, height);
    drop(prepared);
    pixels
}

#[cfg(test)]
mod parity;
#[cfg(test)]
mod tests;
