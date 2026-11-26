//! Reading the composed canvas back for an export or a Copy Merged.
//!
//! The live stack keeps no CPU backups, so the old path reads every committed tile into the core
//! and composites it on the CPU before a save. The composite already exists on the GPU, so an
//! export only has to resample it into the requested size and read that one texture back.

use super::*;

fn resample_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas export"),
        source: wgpu::ShaderSource::Wgsl(include_str!("export.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas export"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                // The source is already premultiplied and the shader returns it unchanged.
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Composite `specs` at `width` by `height` and read the frame back as premultiplied RGBA8.
///
/// The stack's own composite stays in its cache, so a present after an export still has its frame.
/// Refuses an extended-range stack, a zero or oversized target, and an empty canvas.
pub(super) fn render_export(
    layers: &mut crate::LayerStack,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    specs: &[crate::LayerSpec],
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let (canvas_width, canvas_height) = layers.size();
    let limit = device.limits().max_texture_dimension_2d;
    if width == 0
        || height == 0
        || canvas_width == 0
        || canvas_height == 0
        || width > limit
        || height > limit
        // The composite runs in the stack's own format; an extended-range frame would need a tone
        // map first, so the shell keeps its CPU export there.
        || layers.format() != crate::FrameFormat::Rgba8
    {
        return None;
    }
    let source = layers.composite_texture(device, queue, pipeline, specs)?;
    if width == canvas_width && height == canvas_height {
        return Some(crate::readback::read_texture(
            device, queue, &source, width, height,
        ));
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("canvas export"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("canvas export"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    let pipeline = resample_pipeline(device);
    let source_view = source.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("canvas export"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&source_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas export"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    Some(crate::readback::read_texture(
        device, queue, &target, width, height,
    ))
}

impl CanvasSurface {
    /// Composite the current layer specs into `width` by `height` premultiplied RGBA8.
    ///
    /// At the canvas size the cached composite is read directly. A scaled export renders through
    /// its own target, so the cache the next present copies from is left intact.
    pub fn export_frame(&mut self, width: u32, height: u32) -> Option<Vec<u8>> {
        if self.hdr {
            return None;
        }
        let (specs, hidden) = self.export_specs();
        // Reference layers are on screen but not in an export, so their frames differ. Recompose
        // every tile for the export, then restore the on-screen frame below.
        if hidden {
            self.layers.mark_all_dirty();
        }
        let pixels = render_export(
            &mut self.layers,
            &self.device,
            &self.queue,
            &self.composite_pipeline,
            &specs,
            width,
            height,
        );
        if hidden {
            // Put the reference layers back into the cache now: a present or a pick must not see
            // the export's frame.
            self.layers.mark_all_dirty();
            let _ = self.layers.composite_texture(
                &self.device,
                &self.queue,
                &self.composite_pipeline,
                &self.layer_specs,
            );
        }
        pixels
    }

    /// Leave layer `index` out of an export. The canvas still shows it.
    pub fn set_export_reference(&mut self, index: usize, reference: bool) {
        if self.export_references.len() <= index {
            self.export_references.resize(index + 1, false);
        }
        self.export_references[index] = reference;
    }

    /// The specs to composite for an export, and whether a reference layer was hidden.
    fn export_specs(&self) -> (Vec<crate::LayerSpec>, bool) {
        let hidden = self
            .layer_specs
            .iter()
            .enumerate()
            .any(|(index, _)| self.export_references.get(index).copied().unwrap_or(false));
        if !hidden {
            return (self.layer_specs.clone(), false);
        }
        let mut specs = self.layer_specs.clone();
        for (index, spec) in specs.iter_mut().enumerate() {
            if self.export_references.get(index).copied().unwrap_or(false) {
                spec.visible = false;
            }
        }
        (specs, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameFormat, LayerSpec, LayerStack, Renderer};

    fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        pixel
            .iter()
            .copied()
            .cycle()
            .take((width * height * 4) as usize)
            .collect()
    }

    fn rgba(width: u32, pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    }

    fn export(
        stack: &mut LayerStack,
        renderer: &Renderer,
        specs: &[LayerSpec],
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        let pipeline = crate::composite::pipeline(renderer.device());
        render_export(
            stack,
            renderer.device(),
            renderer.queue(),
            &pipeline,
            specs,
            width,
            height,
        )
    }

    #[test]
    fn a_red_layer_exports_red() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (8u32, 8u32);
        let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
        assert!(stack.set(
            renderer.device(),
            renderer.queue(),
            0,
            &solid(width, height, [255, 0, 0, 255])
        ));
        let pixels =
            export(&mut stack, &renderer, &[LayerSpec::new(0)], width, height).expect("an export");
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        assert_eq!(rgba(width, &pixels, 3, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn a_transparent_document_exports_transparent() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (8u32, 8u32);
        let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
        let pixels =
            export(&mut stack, &renderer, &[LayerSpec::new(0)], width, height).expect("an export");
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        assert!(pixels.iter().all(|value| *value == 0));
    }

    #[test]
    fn a_half_scale_export_has_half_the_pixels_and_the_same_average() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (16u32, 8u32);
        let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
        let mut canvas = solid(width, height, [0, 0, 255, 255]);
        for y in 0..height {
            for x in 0..width / 2 {
                let at = ((y * width + x) * 4) as usize;
                canvas[at..at + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        assert!(stack.set(renderer.device(), renderer.queue(), 0, &canvas));
        let pixels = export(&mut stack, &renderer, &[LayerSpec::new(0)], 8, 4).expect("an export");
        assert_eq!(pixels.len(), 8 * 4 * 4);
        // Half red and half blue in, half red and half blue out: a scale rather than a crop.
        let mut sums = [0u64; 4];
        for pixel in pixels.as_chunks::<4>().0 {
            for (sum, value) in sums.iter_mut().zip(pixel) {
                *sum += u64::from(*value);
            }
        }
        let count = (8 * 4) as u64;
        let average = sums.map(|sum| sum / count);
        assert!(
            (127..=128).contains(&average[0]),
            "red average: {average:?}"
        );
        assert!(
            (127..=128).contains(&average[2]),
            "blue average: {average:?}"
        );
        assert_eq!(average[3], 255, "alpha average: {average:?}");
    }

    #[test]
    fn an_export_leaves_the_presented_frame_in_the_cache() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (16u32, 16u32);
        let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
        assert!(stack.set(
            renderer.device(),
            renderer.queue(),
            0,
            &solid(width, height, [10, 200, 30, 255])
        ));
        let specs = [LayerSpec::new(0)];
        // What the next present copies from, before the export.
        let before = renderer.composite_stack(&mut stack, &specs);

        export(&mut stack, &renderer, &specs, 8, 8).expect("a scaled export");
        // A present composites from the same cache; the export must not have replaced it with the
        // half-sized frame or with a cleared target.
        assert_eq!(renderer.composite_stack(&mut stack, &specs), before);

        // The canvas-size export reads that same cache directly; it must not consume it either.
        export(&mut stack, &renderer, &specs, width, height).expect("a full-size export");
        let after = renderer.composite_stack(&mut stack, &specs);
        assert_eq!(after, before);
        assert_eq!(rgba(width, &after, 1, 1), [10, 200, 30, 255]);
    }

    #[test]
    fn an_export_refuses_an_extended_range_stack() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (8u32, 8u32);
        let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba16);
        assert!(
            export(&mut stack, &renderer, &[LayerSpec::new(0)], width, height).is_none(),
            "a half-float stack has no eight-bit export"
        );
    }

    /// Not part of the suite: `cargo test -p drawkit-gpu -- --ignored --nocapture measure`.
    ///
    /// The old export path reads every layer back into the core and composites on the CPU; the
    /// new one reads the composite the GPU already holds. Times and bytes are printed, not
    /// asserted, because they depend on the machine.
    #[test]
    #[ignore = "4096 measurement"]
    fn measure_export_at_4096() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (device, queue) = (renderer.device(), renderer.queue());
        let (side, count) = (4096u32, 4usize);
        let mut stack = LayerStack::without_backups(side, side, FrameFormat::Rgba8);
        for index in 0..count {
            let pixel = [(index * 60) as u8, 40, 200, 255];
            assert!(stack.set(device, queue, index, &solid(side, side, pixel)));
        }
        let specs: Vec<LayerSpec> = (0..count).map(LayerSpec::new).collect();
        // Warm the composite the way a present would.
        let _ = renderer.composite_stack(&mut stack, &specs);
        let frame_bytes = side as usize * side as usize * 4;
        let frame = frame_bytes as f64 / (1024.0 * 1024.0);

        // Before: one full readback per layer, then the CPU over-composite the core runs.
        let mut readback = std::time::Duration::ZERO;
        let mut compose = std::time::Duration::ZERO;
        let start = std::time::Instant::now();
        let mut canvas = vec![0u8; frame_bytes];
        for index in 0..count {
            let read_start = std::time::Instant::now();
            let tile = stack
                .read_layer_rect(device, queue, index, (0, 0, side, side))
                .expect("a full layer readback");
            readback += read_start.elapsed();
            let paint_start = std::time::Instant::now();
            for (out, src) in canvas
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(tile.pixels.as_chunks::<4>().0)
            {
                let rest = 255 - src[3] as u16;
                for channel in 0..3 {
                    out[channel] =
                        (src[channel] as u16 + out[channel] as u16 * rest / 255).min(255) as u8;
                }
                out[3] = (src[3] as u16 + out[3] as u16 * rest / 255).min(255) as u8;
            }
            compose += paint_start.elapsed();
        }
        let before = start.elapsed();

        // After: one composite readback at canvas size.
        let start = std::time::Instant::now();
        let exported = export(&mut stack, &renderer, &specs, side, side).expect("an export");
        let after = start.elapsed();
        assert_eq!(exported.len(), side as usize * side as usize * 4);

        println!(
            "4096 export, {count} layers: before {:.0} ms (readback {:.0} ms for {:.0} MiB, \
             CPU composite {:.0} ms for {:.0} MiB), after {:.0} ms ({:.0} MiB read back)",
            before.as_secs_f64() * 1000.0,
            readback.as_secs_f64() * 1000.0,
            frame * count as f64,
            compose.as_secs_f64() * 1000.0,
            frame,
            after.as_secs_f64() * 1000.0,
            frame,
        );
    }
}
