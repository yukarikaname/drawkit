//! The field itself: textures, pipelines, injection, stepping, reading and the deposit pack.

mod deposit;
mod entries;
mod new;
mod read;

use super::{DT, MAX_STAMPS, Params, WORKGROUP, WetSettings};

pub use deposit::WetDeposit;

pub struct WetField {
    canvas: (u32, u32),
    scale: f32,
    width: u32,
    height: u32,
    state0: [wgpu::Texture; 2],
    state1: [wgpu::Texture; 2],
    /// The pair being read; a pass writes the other one.
    current: usize,
    step_pipeline: wgpu::ComputePipeline,
    inject_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
    /// Pack the current pair into the straight RGBA a layer bake samples.
    deposit_pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    stamps: wgpu::Buffer,
    peak: wgpu::Buffer,
    /// `[a -> b, b -> a]` for each pipeline.
    step_groups: [wgpu::BindGroup; 2],
    inject_groups: [wgpu::BindGroup; 2],
    reduce_groups: [wgpu::BindGroup; 2],
    settings: WetSettings,
    /// Seconds that did not fill a whole step, so a caller can pass frame time.
    pending: f32,
    /// A step has run, so the field may hold settled pigment.
    stepped: bool,
}

impl WetField {
    /// Field texels, not document pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn canvas_size(&self) -> (u32, u32) {
        self.canvas
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn settings(&self) -> WetSettings {
        self.settings
    }

    /// Add water and pigment in a soft disc. Coordinates and radius are document pixels.
    pub fn inject(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        centre: (u32, u32),
        radius: f32,
        water: f32,
        colour: [f32; 3],
    ) {
        let scale = self.scale;
        self.inject_stamps(
            device,
            queue,
            &[[
                centre.0 as f32 * scale,
                centre.1 as f32 * scale,
                (radius * scale).max(0.5),
                water,
            ]],
            colour,
        );
    }

    /// Add a stroke segment as a run of stamps. A segment longer than `MAX_STAMPS` stamps is
    /// spaced out rather than split; the brush splits its own segments.
    #[allow(clippy::too_many_arguments)]
    pub fn inject_stroke(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        water: f32,
        colour: [f32; 3],
    ) {
        let scale = self.scale;
        let (from, to) = (
            (from.0 * scale, from.1 * scale),
            (to.0 * scale, to.1 * scale),
        );
        let radius = (radius * scale).max(0.5);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        let spacing = (radius * 0.35).max(0.75);
        let count = ((length / spacing).ceil() as usize + 1).min(MAX_STAMPS);
        let mut stamps = Vec::with_capacity(count);
        for index in 0..count {
            let t = if count <= 1 {
                0.0
            } else {
                index as f32 / (count - 1) as f32
            };
            stamps.push([from.0 + dx * t, from.1 + dy * t, radius, water]);
        }
        self.inject_stamps(device, queue, &stamps, colour);
    }

    fn inject_stamps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stamps: &[[f32; 4]],
        colour: [f32; 3],
    ) {
        if stamps.is_empty() {
            return;
        }
        let count = stamps.len().min(MAX_STAMPS);
        let mut packed = vec![0f32; MAX_STAMPS * 4];
        for (index, stamp) in stamps[..count].iter().enumerate() {
            packed[index * 4..index * 4 + 4].copy_from_slice(stamp);
        }
        queue.write_buffer(&self.stamps, 0, bytemuck::cast_slice(&packed));
        let mut params = Params::step(self.width, self.height, DT, self.settings);
        params.count = count as u32;
        params.colour_r = colour[0];
        params.colour_g = colour[1];
        params.colour_b = colour[2];
        params.colour_a = 1.0;
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wet inject"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.inject_pipeline);
            pass.set_bind_group(0, &self.inject_groups[self.current], &[]);
            pass.dispatch_workgroups(
                self.width.div_ceil(WORKGROUP),
                self.height.div_ceil(WORKGROUP),
                1,
            );
        }
        queue.submit(Some(encoder.finish()));
        self.current = 1 - self.current;
    }

    /// Advance the field by fixed 1/60 s steps.
    pub fn advance(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, steps: usize) {
        if steps == 0 {
            return;
        }
        self.stepped = true;
        let mut encoder = device.create_command_encoder(&Default::default());
        for _ in 0..steps {
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("wet step"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.step_pipeline);
                pass.set_bind_group(0, &self.step_groups[self.current], &[]);
                pass.dispatch_workgroups(
                    self.width.div_ceil(WORKGROUP),
                    self.height.div_ceil(WORKGROUP),
                    1,
                );
            }
            self.current = 1 - self.current;
        }
        queue.submit(Some(encoder.finish()));
    }

    /// Advance by elapsed seconds, keeping the remainder for the next call. At most eight steps
    /// run in one call so a stalled frame does not turn into a simulation spiral.
    pub fn advance_seconds(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, seconds: f32) {
        self.pending += seconds.clamp(0.0, 0.25);
        let steps = (self.pending / DT) as usize;
        let run = steps.min(8);
        if run == 0 {
            return;
        }
        self.pending -= run as f32 * DT;
        self.advance(device, queue, run);
    }

    /// The largest water value in the field. This reads back, so a shell should ask it when it
    /// wants to bake or stop stepping, not every frame.
    pub fn peak_water(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> f32 {
        queue.write_buffer(&self.peak, 0, &0u32.to_le_bytes());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wet peak"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduce_pipeline);
            pass.set_bind_group(0, &self.reduce_groups[self.current], &[]);
            pass.dispatch_workgroups(
                self.width.div_ceil(WORKGROUP),
                self.height.div_ceil(WORKGROUP),
                1,
            );
        }
        queue.submit(Some(encoder.finish()));
        let bytes = crate::readback::read_buffer(device, queue, &self.peak, 4);
        f32::from_bits(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// True when nothing is wet enough to keep evolving.
    pub fn is_dry(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        self.peak_water(device, queue) <= self.settings.dry * 2.0
    }
}
