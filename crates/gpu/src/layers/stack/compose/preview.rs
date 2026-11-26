//! The live stroke blended into one layer before that layer joins the stack.
//!
//! The preview pass reads the layer's resident tile and the coverage buffer with the same shader a
//! commit uses, so the stroke shows the pixels the commit will write. Everything the stack does
//! around it - the layer's opacity and blend mode, a group's isolation, a clip base - is unchanged,
//! which is what keeps the preview at the layer's place instead of over the finished frame.

use wgpu::util::DeviceExt;

use super::*;

/// One frame's stroke. `color[3]` is the commit's coverage multiplier, so the pass reproduces the
/// commit's `cov = coverage * alpha` exactly.
pub(super) struct PreviewStroke<'a> {
    pub(super) layer: usize,
    pub(super) rect: (u32, u32, u32, u32),
    pub(super) coverage: &'a wgpu::Buffer,
    pub(super) pipeline: &'a wgpu::RenderPipeline,
    pub(super) color: [f32; 4],
    pub(super) wet: f32,
    pub(super) mode: u32,
    pub(super) smudge: [f32; 2],
}

/// The tile the preview was rendered into, for `render_list` to sample in place of the layer.
#[derive(Clone, Copy)]
pub(super) struct PreviewTile<'a> {
    pub(super) layer: usize,
    pub(super) view: &'a wgpu::TextureView,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    origin: [u32; 2],
    size: [u32; 2],
    canvas: [u32; 2],
    alpha: f32,
    _pad: f32,
    color: [f32; 4],
    wet: f32,
    _pad2: f32,
    smudge: [f32; 2],
    mode: u32,
    _pad4: [u32; 7],
}

impl LayerStack {
    /// Render the stroke over one tile of the previewed layer. The layer keeps its own tiles; this
    /// writes a scratch target that the composite pass then treats as the layer's pixels.
    pub(super) fn render_preview_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        stroke: &PreviewStroke<'_>,
        coord: (u32, u32),
        target: &wgpu::TextureView,
    ) -> bool {
        if !self.ensure_preview_tile(device, queue, stroke.layer, coord) {
            return false;
        }
        let Some(old) = self.layers[stroke.layer]
            .as_ref()
            .and_then(|layer| layer.tiles.get(&coord))
            .and_then(|tile| tile.gpu.as_ref())
            .map(|gpu| gpu.texture.clone())
        else {
            return false;
        };
        let old_view = old.create_view(&Default::default());
        let params = Params {
            origin: [coord.0 * self.tile_width, coord.1 * self.tile_height],
            size: [self.tile_width, self.tile_height],
            canvas: [self.width, self.height],
            alpha: stroke.color[3],
            _pad: 0.0,
            color: [stroke.color[0], stroke.color[1], stroke.color[2], 1.0],
            wet: stroke.wet,
            _pad2: 0.0,
            smudge: stroke.smudge,
            mode: stroke.mode,
            _pad4: [0; 7],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("stroke preview params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let layout = stroke.pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stroke preview"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&old_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: stroke.coverage.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stroke preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_pipeline(stroke.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        true
    }

    /// A tile the shell never painted still needs a texture before a stroke can land on it, the
    /// same way a commit materialises the tile it is about to write.
    fn ensure_preview_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        coord: (u32, u32),
    ) -> bool {
        let tile_bytes = self.tile_bytes();
        let Some(layer) = self.layers.get_mut(index).and_then(Option::as_mut) else {
            return false;
        };
        layer.tiles.entry(coord).or_insert_with(|| Tile {
            backup: vec![0; tile_bytes],
            nonzero: 0,
            gpu: None,
            used: 0,
            stale: false,
        });
        self.ensure_tile(device, queue, index, coord)
    }
}
