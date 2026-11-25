//! Read access to the live stroke preview and its optional paper map.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct StrokeParams {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) min_x: u32,
    pub(super) min_y: u32,
    pub(super) mask_w: u32,
    pub(super) mask_h: u32,
    pub(super) from_x: f32,
    pub(super) from_y: f32,
    pub(super) to_x: f32,
    pub(super) to_y: f32,
    pub(super) radius: f32,
    pub(super) hardness: f32,
    pub(super) spacing: f32,
    pub(super) roundness: f32,
    pub(super) angle: f32,
    pub(super) phase: f32,
    pub(super) alpha: f32,
    pub(super) _pad: f32,
    pub(super) pressure: f32,
    pub(super) flow: f32,
    pub(super) pressure_alpha: f32,
    pub(super) paper: f32,
    pub(super) tip: f32,
    pub(super) _pad2: f32,
    pub(super) dual: f32,
    pub(super) dual_scale: f32,
    pub(super) dual_hardness: f32,
    pub(super) dual_roundness: f32,
    pub(super) dual_angle: f32,
    pub(super) dual_paper: f32,
    pub(super) dual_tip: f32,
    pub(super) _pad3: f32,
    pub(super) has_map: u32,
    pub(super) _pad4: u32,
    pub(super) _pad5: [u32; 2],
    pub(super) jitter: f32,
    pub(super) grain: f32,
    pub(super) dual_jitter: f32,
    pub(super) dual_grain: f32,
    pub(super) seed_low: u32,
    pub(super) seed_high: u32,
    pub(super) _pad6: [u32; 2],
}

/// A coverage readback: the rectangle and its row-major values.
pub type CoverageRead = ((u32, u32, u32, u32), Vec<f32>);

impl StrokePreview {
    pub fn color(&self) -> Option<[f32; 4]> {
        self.stroke.as_ref().map(|stroke| stroke.color)
    }

    pub fn preview_layer(&self) -> Option<usize> {
        self.preview_layer
    }

    pub fn is_erase(&self) -> bool {
        self.erase
    }

    pub fn wet(&self) -> f32 {
        self.wet
    }

    pub fn coverage(&self) -> Option<&wgpu::Buffer> {
        self.stroke.as_ref().map(|stroke| &stroke.buffer)
    }

    pub fn dirty(&self) -> Option<(u32, u32, u32, u32)> {
        self.stroke.as_ref()?.dirty
    }

    /// Read the dirty coverage back as row-major floats, for a shell that presents through a
    /// bitmap instead of a swapchain. The copy keeps whole rows; the columns outside the stroke
    /// are dropped here.
    pub fn read(&mut self) -> Option<CoverageRead> {
        let stroke = self.stroke.as_ref()?;
        let (x0, y0, x1, y1) = stroke.dirty?;
        let width = stroke.width as u64;
        let rows = (y1 - y0 + 1) as u64;
        let offset = y0 as u64 * width * 4;
        let size = rows * width * 4;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&stroke.buffer, offset, &staging, 0, size);
        self.queue.submit(Some(encoder.finish()));
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let mapped = staging.slice(..).get_mapped_range().ok()?;
        let row_bytes = stroke.width as usize * 4;
        let columns = (x1 - x0 + 1) as usize;
        let mut coverage = Vec::with_capacity(columns * rows as usize);
        for row in 0..rows as usize {
            let start = row * row_bytes + x0 as usize * 4;
            let values: &[f32] = bytemuck::cast_slice(&mapped[start..start + columns * 4]);
            coverage.extend_from_slice(values);
        }
        drop(mapped);
        staging.unmap();
        Some(((x0, y0, x1 - x0 + 1, y1 - y0 + 1), coverage))
    }

    /// Install a grayscale map for the next stroke, or clear it for the procedural one.
    pub fn set_map(&mut self, width: u32, height: u32, data: &[u8]) {
        if width == 0 || height == 0 || data.len() < (width * height) as usize {
            self.map = None;
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("brush map"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data[..(width * height) as usize],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.map_view = texture.create_view(&Default::default());
        self.map = Some(texture);
    }

    pub fn clear_map(&mut self) {
        self.map = None;
    }
}

impl CanvasSurface {
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_begin(
        &mut self,
        width: u32,
        height: u32,
        color: [f32; 4],
        flow: f32,
        pressure_alpha: f32,
        paper: f32,
        tip: f32,
        dual: [f32; 7],
        wet: f32,
        jitter: f32,
        grain: f32,
        dual_jitter: f32,
        dual_grain: f32,
        seed: u64,
        erase: bool,
        preview_layer: u32,
    ) {
        self.stroke.begin(
            width,
            height,
            color,
            flow,
            pressure_alpha,
            paper,
            tip,
            dual,
            wet,
            jitter,
            grain,
            dual_jitter,
            dual_grain,
            seed,
            erase,
            preview_layer,
        );
    }

    pub fn stroke_end(&mut self) {
        self.stroke.end();
    }

    pub fn set_brush_map(&mut self, width: u32, height: u32, data: &[u8]) {
        self.stroke.set_map(width, height, data);
    }

    pub fn clear_brush_map(&mut self) {
        self.stroke.clear_map();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        spacing: f32,
        roundness: f32,
        angle: f32,
        pressure: f32,
    ) {
        self.stroke.segment(
            from, to, radius, hardness, spacing, roundness, angle, pressure,
        );
    }

    pub fn stroke_dirty(&self) -> Option<(u32, u32, u32, u32)> {
        self.stroke.dirty()
    }
}
