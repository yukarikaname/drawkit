//! Single stamps and the smudge tool. A segment collects coverage first; these two write one
//! mark or move existing paint, so they live beside the segment builders rather than inside them.

use super::shape::coverage;
use super::*;

/// One stamped disc, centred at a fractional pixel.
pub fn stamp(surface: &mut Surface, centre: (f32, f32), radius: f32, hardness: f32, color: Rgba) {
    if radius <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let x0 = (centre.0 - radius).floor().max(0.0) as usize;
    let y0 = (centre.1 - radius).floor().max(0.0) as usize;
    let x1 = ((centre.0 + radius).ceil() as isize).max(0) as usize;
    let y1 = ((centre.1 + radius).ceil() as isize).max(0) as usize;
    for y in y0..=y1.min(surface.height().saturating_sub(1)) {
        for x in x0..=x1.min(surface.width().saturating_sub(1)) {
            let dx = x as f32 - centre.0;
            let dy = y as f32 - centre.1;
            let amount = coverage((dx * dx + dy * dy).sqrt(), radius, hardness);
            if amount <= 0.0 {
                continue;
            }
            let existing = surface.get(x, y);
            surface.set(
                x,
                y,
                over([color[0], color[1], color[2], color[3] * amount], existing),
            );
        }
    }
}

/// Drag paint from where the brush was to where it is now.
///
/// The source is sampled from a snapshot of the whole touched rectangle, so a pixel written earlier
/// in the same segment never becomes the source for a later one. `strength` is how much of the
/// carried colour lands, and the falloff uses the same hardness as the brush.
pub fn smudge(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
    strength: f32,
) {
    let strength = strength.clamp(0.0, 1.0);
    if radius <= 0.0 || strength <= 0.0 {
        return;
    }
    let mut phase = 0.0;
    let Some((mask, min_x, min_y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        &Brush::from_hardness(hardness),
        0,
        &mut phase,
    ) else {
        return;
    };
    let left = (from.0.min(to.0) - radius).floor().max(0.0) as usize;
    let top = (from.1.min(to.1) - radius).floor().max(0.0) as usize;
    let right = ((from.0.max(to.0) + radius).ceil().max(0.0) as usize + 1).min(surface.width());
    let bottom = ((from.1.max(to.1) + radius).ceil().max(0.0) as usize + 1).min(surface.height());
    let region = crate::surface::Rect::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    );
    if region.is_empty() {
        return;
    }
    let saved = surface.snapshot(region);
    let dx = from.0 - to.0;
    let dy = from.1 - to.1;
    for my in 0..mask.height {
        for mx in 0..mask.width {
            let amount = mask.get(mx, my) * strength;
            if amount <= 0.0 {
                continue;
            }
            let x = min_x + mx;
            let y = min_y + my;
            let sx = x as f32 + dx;
            let sy = y as f32 + dy;
            if sx < region.x as f32
                || sy < region.y as f32
                || sx >= (region.x + region.width) as f32
                || sy >= (region.y + region.height) as f32
            {
                continue;
            }
            let source = saved[(sy as usize - region.y) * region.width + (sx as usize - region.x)];
            if source[3] <= 0.0 {
                continue;
            }
            let destination = surface.get(x, y);
            surface.set(
                x,
                y,
                over(
                    [source[0], source[1], source[2], source[3] * amount],
                    destination,
                ),
            );
        }
    }
    surface.mark(region);
}
