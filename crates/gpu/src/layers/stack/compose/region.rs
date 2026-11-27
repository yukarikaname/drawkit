//! Partial composites: one canvas rectangle instead of the whole canvas.
//!
//! The layer passes are already per tile. `render_list` draws a tile into a tile-sized target and
//! `copy_tile` places it in the output, so a partial present needs no viewport, scissor or
//! uniform in `composite.wgsl`; only the destination copy is clipped to the region and shifted
//! by its origin. That is the smaller change of the two, and it leaves the full-canvas pass
//! byte-identical, which is what the export and thumbnail paths still use.

/// A canvas rectangle: origin x, origin y, width, height.
pub(crate) type Region = (u32, u32, u32, u32);

/// Clamp `region` to the canvas. `None` when it has no area.
pub(crate) fn clamp(region: Region, canvas_width: u32, canvas_height: u32) -> Option<Region> {
    let (x, y, width, height) = region;
    let x0 = x.min(canvas_width);
    let y0 = y.min(canvas_height);
    let x1 = x.saturating_add(width).min(canvas_width);
    let y1 = y.saturating_add(height).min(canvas_height);
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// Whether a tile overlaps the region at all.
pub(crate) fn tile_in_region(
    coord: (u32, u32),
    tile_width: u32,
    tile_height: u32,
    region: Region,
) -> bool {
    let (x, y, width, height) = region;
    let left = coord.0 * tile_width;
    let top = coord.1 * tile_height;
    left < x + width && left + tile_width > x && top < y + height && top + tile_height > y
}

/// Copy the part of a tile that falls inside `region` into the region-sized output.
pub(super) fn copy_tile(
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
    output: &wgpu::Texture,
    coord: (u32, u32),
    canvas: (u32, u32),
    tile: (u32, u32),
    region: Region,
) {
    let (tx, ty) = coord;
    let (rx, ry, rw, rh) = region;
    let left = tx * tile.0;
    let top = ty * tile.1;
    let x0 = left.max(rx);
    let y0 = top.max(ry);
    let x1 = left.saturating_add(tile.0).min(rx + rw).min(canvas.0);
    let y1 = top.saturating_add(tile.1).min(ry + rh).min(canvas.1);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: source,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: x0 - left,
                y: y0 - top,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: output,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: x0 - rx,
                y: y0 - ry,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: x1 - x0,
            height: y1 - y0,
            depth_or_array_layers: 1,
        },
    );
}
