//! One segment's coverage mask, and the spacing phase that carries between segments.

use super::shape::{coverage, jitter, noise, segment_hash, shaped_distance};
use super::*;

/// The salt the shader uses for the primary tip's grain.
const GRAIN_SALT: u32 = 0x51ED_2701;
/// The salt the shader uses for the primary and the second tip's jitter.
const PRIMARY_JITTER_SALT: u32 = 0x9E37_79B1;
const DUAL_JITTER_SALT: u32 = 0x85EB_CA77;

/// How far a stamp can reach from the segment's centre line.
///
/// A primary stamp reaches its radius plus its jitter. A second tip adds its offset, its own
/// radius and its own jitter. The mask rectangle has to cover all of it or the stamps at the ends
/// of a segment are cut off. `stroke.wgsl` computes the same reach.
fn spread(radius: f32, brush: &Brush) -> f32 {
    let mut reach = radius * (1.0 + brush.jitter.max(0.0));
    if brush.dual > 0.0 {
        let secondary = Brush::for_kind(BrushKind::from_i32(brush.dual_kind as i32));
        reach += radius * (2.0 * brush.dual_scale.max(0.0) + secondary.jitter.max(0.0));
    }
    reach
}

/// The coverage of one segment, and where its top-left corner sits on the canvas.
///
/// Both the brush and the eraser want this shape: stamp the discs, take the maximum, composite
/// once. The only difference is whether that composite adds colour or removes it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn segment_mask(
    width: usize,
    height: usize,
    points: ((f32, f32), (f32, f32)),
    radius: f32,
    brush: &Brush,
    seed: u64,
    phase: &mut f32,
    texture: Option<&BrushTexture>,
) -> Option<(Mask, usize, usize)> {
    let (from, to) = points;
    if radius <= 0.0 || width == 0 || height == 0 {
        return None;
    }
    // Jitter and the second tip move a stamp centre, so the touched rectangle is larger than the
    // nominal disc.
    let reach = spread(radius, brush);
    let min_x = (from.0.min(to.0) - reach).floor().max(0.0) as usize;
    let min_y = (from.1.min(to.1) - reach).floor().max(0.0) as usize;
    let max_x = ((from.0.max(to.0) + reach).ceil().max(0.0) as usize).min(width.saturating_sub(1));
    let max_y = ((from.1.max(to.1) + reach).ceil().max(0.0) as usize).min(height.saturating_sub(1));
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let mut mask = Mask::new(max_x - min_x + 1, max_y - min_y + 1);
    let segment = segment_hash(seed, from, to);
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let distance = (dx * dx + dy * dy).sqrt();
    let (perpendicular_x, perpendicular_y) = if distance > 0.0001 {
        (-dy / distance, dx / distance)
    } else {
        // The shader falls back to the stroke direction (0, 1) and then takes its perpendicular,
        // so a dot's second tip lands on -x; a direction here would put it on +y and split the dot
        // differently on the two paths.
        (-1.0, 0.0)
    };

    let mut plot = |tip: &Brush, cx: f32, cy: f32, scale: f32, weight: f32| {
        let local_radius = radius * scale;
        let angle = tip.angle.to_radians();
        let x0 = ((cx - local_radius).floor().max(0.0) as usize).max(min_x);
        let y0 = ((cy - local_radius).floor().max(0.0) as usize).max(min_y);
        let x1 = ((cx + local_radius).ceil().max(0.0) as usize).min(max_x);
        let y1 = ((cy + local_radius).ceil().max(0.0) as usize).min(max_y);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let ox = x as f32 - cx;
                let oy = y as f32 - cy;
                let shaped = shaped_distance(ox, oy, angle, tip.roundness);
                let mut amount = coverage(shaped, local_radius, tip.hardness) * weight;
                if amount <= 0.0 {
                    continue;
                }
                if tip.grain > 0.0 {
                    amount *= 1.0 - tip.grain * noise(x as u32, y as u32, GRAIN_SALT, seed);
                }
                if tip.paper > 0.0 {
                    let grain = texture.map_or_else(
                        || paper_value(x, y),
                        |map| map.sample(x as f32 / width as f32, y as f32 / height as f32),
                    );
                    amount *= 1.0 - tip.paper * grain;
                }
                if tip.texture > 0.0 {
                    let grain = texture.map_or_else(
                        || tip_value(ox, oy),
                        |map| {
                            map.sample(
                                (ox / local_radius + 1.0) * 0.5,
                                (oy / local_radius + 1.0) * 0.5,
                            )
                        },
                    );
                    amount *= 1.0 - tip.texture * grain;
                }
                if amount <= 0.0 {
                    continue;
                }
                let mx = x - min_x;
                let my = y - min_y;
                mask.set(mx, my, mask.get(mx, my).max(amount));
            }
        }
    };
    let mut ordinal = 0_u32;
    let mut stamp = |tip: &Brush, cx: f32, cy: f32, scale: f32, weight: f32| {
        let slot = ordinal;
        ordinal = ordinal.wrapping_add(1);
        let (jitter_x, jitter_y) = jitter(segment, slot, tip.jitter, radius, PRIMARY_JITTER_SALT);
        let x = cx + jitter_x;
        let y = cy + jitter_y;
        plot(tip, x, y, scale, weight);
        if scale == 1.0 && brush.dual > 0.0 {
            let secondary = Brush::for_kind(BrushKind::from_i32(brush.dual_kind as i32));
            let offset = radius * brush.dual_scale;
            let base = (x + perpendicular_x * offset, y + perpendicular_y * offset);
            let (jitter_x, jitter_y) =
                jitter(segment, slot, secondary.jitter, radius, DUAL_JITTER_SALT);
            plot(
                &secondary,
                base.0 + jitter_x,
                base.1 + jitter_y,
                brush.dual_scale,
                brush.dual,
            );
        }
    };

    let spacing = (radius * brush.spacing.max(0.02)).max(0.5);
    if *phase < 0.0 {
        // Legacy single-segment spacing used by the preview path: both endpoints are stamped and
        // the count is rounded up from the segment length.
        let steps = (distance / spacing).ceil() as usize;
        if steps == 0 {
            stamp(brush, to.0, to.1, 1.0, 1.0);
        } else {
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                stamp(brush, from.0 + dx * t, from.1 + dy * t, 1.0, 1.0);
            }
        }
        *phase = 0.0;
        return if mask.is_empty() {
            None
        } else {
            Some((mask, min_x, min_y))
        };
    }
    // `phase` is the distance from the current point to the next stamp, carried across input
    // segments. Recomputing a local 0..=steps sequence per segment stamped the shared endpoint
    // twice and rounded the spacing differently at every sample, which showed up as darker dots.
    if distance <= 0.0001 {
        if *phase <= 0.0001 {
            stamp(brush, to.0, to.1, 1.0, 1.0);
            *phase = spacing;
        }
    } else {
        let mut next = phase.clamp(0.0, spacing);
        let mut travelled = 0.0;
        if next <= 0.0001 {
            stamp(brush, from.0, from.1, 1.0, 1.0);
            next = spacing;
        }
        while travelled + next <= distance + 0.0001 {
            travelled += next;
            let t = (travelled / distance).clamp(0.0, 1.0);
            stamp(brush, from.0 + dx * t, from.1 + dy * t, 1.0, 1.0);
            next = spacing;
        }
        *phase = (next - (distance - travelled)).clamp(0.0, spacing);
        if *phase <= 0.0001 {
            *phase = spacing;
        }
    }
    if mask.is_empty() {
        None
    } else {
        Some((mask, min_x, min_y))
    }
}

fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut value = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ seed;
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    (value & 0xFFFF) as f32 / 65535.0
}

fn paper_value(x: usize, y: usize) -> f32 {
    let x = x as u32;
    let y = y as u32;
    hash(x / 3, y / 3, 0x51ED_2701) * 0.65 + hash(x / 11, y / 11, 0x9E37_79B9) * 0.35
}

fn tip_value(x: f32, y: f32) -> f32 {
    hash(
        (x * 3.0) as i32 as u32,
        (y * 3.0) as i32 as u32,
        0xC2B2_AE35,
    )
}
