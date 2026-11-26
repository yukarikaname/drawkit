//! The bake source: settled pigment packed to straight RGBA, entirely on the GPU.
//!
//! A layer bake cannot read the field pair directly: the eight channels are split across two
//! textures and the settled pigment is not in an order a tile shader expects. One small compute
//! pass packs the current pair once per bake into
//!
//! * `colour`: `(settled / density, 1 - exp(-density))`, the straight RGBA a source-over bake
//!   stores, and
//! * `mobile`: `(mobile pigment, max(mobile pigment))`, the pigment that has to stay in the wet
//!   field.
//!
//! Nothing is copied to the CPU. A caller hands the returned textures to the layer bake, or reads
//! them back only in a test or a save.

use super::WetField;

/// The settled and mobile pigment of one field state, as two resident `Rgba16Float` textures.
pub struct WetDeposit {
    colour: wgpu::Texture,
    colour_view: wgpu::TextureView,
    mobile: wgpu::Texture,
    mobile_view: wgpu::TextureView,
    width: u32,
    height: u32,
    canvas: (u32, u32),
    /// No step has run, so the field cannot have settled any pigment yet.
    empty: bool,
}

impl WetDeposit {
    /// The bake colour: straight `(settled rgb, coverage)`.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.colour
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.colour_view
    }

    /// The pigment still suspended in the water: straight `(mobile rgb, density)`.
    pub fn mobile_texture(&self) -> &wgpu::Texture {
        &self.mobile
    }

    pub fn mobile_view(&self) -> &wgpu::TextureView {
        &self.mobile_view
    }

    /// Field texels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The document size the field was built for.
    pub fn canvas_size(&self) -> (u32, u32) {
        self.canvas
    }

    /// True while no step has run, so the bake has nothing to lay down. A dry field with settled
    /// pigment is not empty: it still has a bake to apply.
    pub fn is_empty(&self) -> bool {
        self.empty
    }
}

impl WetField {
    /// Pack the current state into the two resident textures a layer bake samples.
    ///
    /// The pass is idempotent: the same field state produces the same bytes, so baking a dry field
    /// again replaces the tiles with the identical colour instead of darkening them.
    pub fn deposit(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<WetDeposit> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let texture = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: super::super::STATE,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let colour = texture("wet deposit");
        let mobile = texture("wet mobile pigment");
        let colour_view = colour.create_view(&Default::default());
        let mobile_view = mobile.create_view(&Default::default());
        let read0 = self.state0[self.current].create_view(&Default::default());
        let read1 = self.state1[self.current].create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wet deposit"),
            layout: &self.deposit_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&read0),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&read1),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&colour_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&mobile_view),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wet deposit"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.deposit_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                self.width.div_ceil(super::WORKGROUP),
                self.height.div_ceil(super::WORKGROUP),
                1,
            );
        }
        queue.submit(Some(encoder.finish()));
        Some(WetDeposit {
            colour,
            colour_view,
            mobile,
            mobile_view,
            width: self.width,
            height: self.height,
            canvas: self.canvas,
            empty: !self.stepped,
        })
    }
}
