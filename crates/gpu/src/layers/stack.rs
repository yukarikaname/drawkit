use std::collections::{HashMap, HashSet};

use crate::FrameFormat;
use crate::composite::{Blend, ClipSource, Source, TileResources, TileTargets};

/// The largest tile edge. Smaller canvases use one tile of the canvas size.
const MAX_TILE: u32 = 1024;

mod access;
pub(super) mod commit;
mod compose;
mod eviction;
mod fill;
mod layers;
mod linear;
mod swap;
mod upload;

pub(crate) struct TileTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: usize,
    /// Created with RENDER_ATTACHMENT, so a commit can draw into it again.
    renderable: bool,
}

struct Tile {
    /// Empty on a backup-less stack; the core surface is the source of truth there.
    backup: Vec<u8>,
    nonzero: usize,
    gpu: Option<TileTexture>,
    used: u64,
    /// The GPU texture changed without the CPU backup following it yet.
    stale: bool,
    /// The texture was evicted and only the shell can upload the pixels back.
    missing: bool,
}

struct Layer {
    tiles: HashMap<(u32, u32), Tile>,
    /// A layer uploaded as fully transparent still has no tiles, but it must not be re-uploaded
    /// by the shell on every frame.
    known: bool,
}

pub struct LayerStack {
    width: u32,
    height: u32,
    /// Format of every resident tile.
    format: FrameFormat,
    tile_width: u32,
    tile_height: u32,
    layers: Vec<Option<Layer>>,
    budget_bytes: usize,
    resident_bytes: usize,
    /// Keep a CPU copy of every painted tile. An offscreen compositor has no shell to ask, so it
    /// needs the copy; the live canvas re-uploads evicted tiles from the core surface instead.
    backups: bool,
    clock: u64,
    /// The last composed canvas, kept between presents so a stroke only recomposes its tiles.
    output: Option<wgpu::Texture>,
    /// The same canvas with the active layer left out, for an erase preview.
    output_skip: Option<wgpu::Texture>,
    /// Textures the journal dropped, kept for the next commit. A replay commits thousands of
    /// strokes; creating a tile texture for each one costs more than the coverage it stores.
    texture_pool: Vec<TileTexture>,
    /// Where the canvas is on screen, for viewport-aware eviction. `None` keeps the old
    /// coldest-first order, which is what an offscreen compositor wants.
    residency: Option<crate::layers::TileResidency>,
    dirty_tiles: HashSet<(u32, u32)>,
    /// Tiles the undo journal still needs in place. Chosen by the owner of the journal, because
    /// the stack does not see steps.
    pinned: HashSet<(usize, (u32, u32))>,
}

pub struct RectUpload<'a> {
    pub index: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

/// The same rectangle in straight linear Display P3 floats, for an extended-range stack.
pub struct LinearUpload<'a> {
    pub index: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [f32],
}

/// Which resident layer to composite, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerSpec {
    pub layer: usize,
    pub opacity: f32,
    pub visible: bool,
    pub blend: Blend,
    /// A fill layer supplies its colour here and has no texture.
    pub fill: Option<[f32; 4]>,
    /// Show this layer only through the alpha of the layer below it.
    pub clip: Option<usize>,
    /// The group this layer belongs to, for an isolated composite.
    pub parent: Option<usize>,
}

impl LayerSpec {
    pub fn new(layer: usize) -> Self {
        Self {
            layer,
            opacity: 1.0,
            visible: true,
            blend: Blend::Normal,
            fill: None,
            clip: None,
            parent: None,
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

    pub fn clip(mut self, layer: usize) -> Self {
        self.clip = Some(layer);
        self
    }

    pub fn parent(mut self, parent: usize) -> Self {
        self.parent = Some(parent);
        self
    }
}

fn nonzero_pixel(pixel: &[u8]) -> bool {
    pixel[0] | pixel[1] | pixel[2] | pixel[3] != 0
}

impl LayerStack {
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_format(width, height, FrameFormat::Rgba8)
    }

    /// A stack whose tiles are `format`. Eight-bit documents keep `Rgba8Unorm`; an extended-range
    /// surface uses `Rgba16Float` and uploads premultiplied linear values instead.
    ///
    /// This is the offscreen compositor mode: every painted tile keeps a CPU backup, so an
    /// evicted texture can be materialised again without a shell.
    pub fn with_format(width: u32, height: u32, format: FrameFormat) -> Self {
        let tile_width = width.clamp(1, MAX_TILE);
        let tile_height = height.clamp(1, MAX_TILE);
        Self {
            width,
            height,
            format,
            tile_width,
            tile_height,
            layers: Vec::new(),
            budget_bytes: 1 << 30,
            resident_bytes: 0,
            backups: true,
            clock: 0,
            output: None,
            output_skip: None,
            texture_pool: Vec::new(),
            residency: None,
            dirty_tiles: HashSet::new(),
            pinned: HashSet::new(),
        }
    }

    /// The live canvas mode: a commit writes its texture and keeps no CPU copy. An evicted tile
    /// is marked missing and `missing_rects` tells the shell what to upload again.
    pub fn without_backups(width: u32, height: u32, format: FrameFormat) -> Self {
        Self {
            backups: false,
            ..Self::with_format(width, height, format)
        }
    }

    /// Whether an evicted texture can be rebuilt from a CPU backup.
    pub fn keeps_backups(&self) -> bool {
        self.backups
    }

    /// Bytes held by the CPU tile backups. A live stack reports zero.
    pub fn backup_bytes(&self) -> usize {
        self.layers
            .iter()
            .flatten()
            .flat_map(|layer| layer.tiles.values())
            .map(|tile| tile.backup.len())
            .sum()
    }

    /// Keep dropped tile textures for reuse. The pool is capped: a replay only needs as many as
    /// the largest stroke touched.
    pub(crate) fn recycle_textures(&mut self, mut tiles: Vec<TileTexture>) {
        const POOL_LIMIT: usize = 48;
        tiles.retain(|tile| tile.renderable);
        let room = POOL_LIMIT.saturating_sub(self.texture_pool.len());
        self.texture_pool
            .extend(tiles.drain(..tiles.len().min(room)));
    }

    /// A tile texture from the pool, or a new one when the pool is empty.
    fn take_tile_texture(&mut self, device: &wgpu::Device, bytes: usize) -> TileTexture {
        if let Some(tile) = self.texture_pool.pop() {
            return tile;
        }
        self.create_tile_texture(device, bytes)
    }

    /// A fresh tile texture. WebGPU zero-fills it, which a partially written upload relies on.
    fn create_tile_texture(&self, device: &wgpu::Device, bytes: usize) -> TileTexture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("committed tile"),
            size: wgpu::Extent3d {
                width: self.tile_width,
                height: self.tile_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format.texture(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        TileTexture {
            texture,
            view,
            bytes,
            renderable: true,
        }
    }

    /// Tell the stack which canvas rectangle is on screen, so eviction can keep what is visible
    /// and preload its ring. `view` is in canvas pixels, `zoom` is device pixels per canvas pixel.
    pub fn set_view(&mut self, view: (f32, f32, f32, f32), zoom: f32) {
        self.residency = Some(crate::layers::TileResidency::new(
            (self.width, self.height),
            self.tile_width,
            view,
            zoom,
        ));
    }

    pub fn format(&self) -> FrameFormat {
        self.format
    }

    pub fn bytes_per_pixel(&self) -> usize {
        self.format.bytes_per_pixel()
    }

    pub fn set_budget(&mut self, bytes: usize) {
        let tile = self.tile_width as usize * self.tile_height as usize * self.bytes_per_pixel();
        self.budget_bytes = bytes.max(tile);
        self.evict_cold(None);
    }

    pub fn budget(&self) -> usize {
        self.budget_bytes
    }

    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }

    /// Whether the shell still needs to upload this layer at all. A backup-less stack can still
    /// lose individual tiles to eviction; those come from `missing_rects`, not here.
    pub fn layer_missing(&self, index: usize) -> bool {
        self.layers
            .get(index)
            .is_none_or(|layer| layer.as_ref().is_none_or(|l| !l.known))
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

    fn coords(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let columns = self.width.div_ceil(self.tile_width);
        let rows = self.height.div_ceil(self.tile_height);
        (0..rows).flat_map(move |y| (0..columns).map(move |x| (x, y)))
    }

    fn tile_bytes(&self) -> usize {
        self.tile_width as usize * self.tile_height as usize * self.bytes_per_pixel()
    }

    fn next_tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Everything that changes the whole stack (a new layer, a reorder, a layer spec) marks every
    /// tile. A stroke only marks the tiles its dirty rectangle touched.
    pub fn mark_all_dirty(&mut self) {
        let coords: Vec<_> = self.coords().collect();
        self.dirty_tiles.extend(coords);
    }

    /// Clear one layer's tiles and its GPU residency.
    fn drop_layer(&mut self, index: usize) {
        let freed = match self.layers.get_mut(index) {
            Some(slot) => slot.take().map_or(0, |layer| {
                layer
                    .tiles
                    .into_values()
                    .filter_map(|tile| tile.gpu.map(|gpu| gpu.bytes))
                    .sum()
            }),
            None => 0,
        };
        self.resident_bytes = self.resident_bytes.saturating_sub(freed);
    }

    pub fn remove(&mut self, index: usize) {
        self.drop_layer(index);
        self.mark_all_dirty();
    }

    /// Materialise a tile on the GPU from its backup. Returns false when the tile is empty or
    /// when the stack keeps no backups and the texture is gone.
    fn ensure_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        coord: (u32, u32),
    ) -> bool {
        let tick = self.next_tick();
        let (width, height) = (self.tile_width, self.tile_height);
        let bytes = self.tile_bytes();
        let stride = width * self.bytes_per_pixel() as u32;
        let backups = self.backups;
        let Some(layer) = self.layers.get_mut(index).and_then(Option::as_mut) else {
            return false;
        };
        let Some(tile) = layer.tiles.get_mut(&coord) else {
            return false;
        };
        tile.used = tick;
        if tile.gpu.is_some() {
            return true;
        }
        if !backups {
            return false;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format.texture(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tile.backup,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&Default::default());
        tile.gpu = Some(TileTexture {
            texture,
            view,
            bytes,
            renderable: false,
        });
        self.resident_bytes += bytes;
        self.evict_cold(Some(coord));
        true
    }
}
