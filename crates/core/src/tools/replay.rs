use super::*;

pub(super) fn apply_replay_command(scratch: &mut Editor, command: Command, phase: &mut f32) {
    match command {
        Command::Stroke {
            layer,
            from,
            to,
            radius,
            pressure,
            color,
            brush: kind,
            seed,
            tilt,
            azimuth,
            roll,
            ..
        } => {
            let pen = scratch
                .brushes
                .get(kind as usize)
                .copied()
                .unwrap_or_else(|| Brush::for_kind(kind))
                .with_input(tilt, azimuth, roll);
            scratch.brush = pen;
            let painted = pen.radius_at(radius, pressure);
            let ink = [
                color[0],
                color[1],
                color[2],
                color[3] * pen.alpha_at(pressure),
            ];
            let rect = scratch.bounds(from, to, painted);
            let layer = layer.min(scratch.document.layers.len() - 1);
            let alpha_locked = scratch.document.layers[layer].alpha_locked;
            let surface = &mut scratch.document.layers[layer].surface;
            if alpha_locked {
                brush::stroke_locked_with_phase(
                    surface,
                    (from, to),
                    painted,
                    &pen,
                    ink,
                    seed,
                    phase,
                );
            } else {
                brush::stroke_with_phase(surface, (from, to), painted, &pen, ink, seed, phase);
            }
            scratch.document.layers[layer].surface.mark(rect);
        }
        Command::Erase {
            layer,
            from,
            to,
            radius,
            pressure,
            brush: kind,
            seed,
            tilt,
            azimuth,
            roll,
            ..
        } => {
            let pen = scratch
                .brushes
                .get(kind as usize)
                .copied()
                .unwrap_or_else(|| Brush::for_kind(kind))
                .with_input(tilt, azimuth, roll);
            scratch.brush = pen;
            let painted = pen.radius_at(radius, pressure);
            let rect = scratch.bounds(from, to, painted);
            let layer = layer.min(scratch.document.layers.len() - 1);
            brush::erase_with_phase(
                &mut scratch.document.layers[layer].surface,
                (from, to),
                painted,
                &pen,
                seed,
                phase,
            );
            scratch.document.layers[layer].surface.mark(rect);
        }
        Command::Fill {
            layer,
            x,
            y,
            tolerance,
            color,
            close,
            expand,
            feather: feather_radius,
        } => {
            let (width, height) = (scratch.document.width(), scratch.document.height());
            let rect = Rect::new(0, 0, width, height);
            let composite: Vec<Rgba> = scratch
                .document
                .composite_rect(rect)
                .into_iter()
                .map(|pixel| over(pixel, PAPER))
                .collect();
            let sealed = close_ink(&composite, width, height, PAPER, close);
            let mask = flood(&sealed, width, height, (x, y), tolerance);
            let grown = grow(&mask, expand);
            let core = erode(&mask, 1);
            let soft = edge_coverage(&composite, &grown, &core, PAPER);
            let soft = feather(&soft, feather_radius);
            let layer = layer.min(scratch.document.layers.len() - 1);
            scratch.document.layers[layer]
                .surface
                .blend_mask(&soft, color);
        }
        Command::Smudge {
            layer,
            from,
            to,
            radius,
            pressure,
            strength,
            brush: kind,
        } => {
            let pen = scratch
                .brushes
                .get(kind as usize)
                .copied()
                .unwrap_or_else(|| Brush::for_kind(kind));
            scratch.brush = pen;
            let painted = pen.radius_at(radius, pressure);
            let rect = scratch.bounds(from, to, painted);
            let layer = layer.min(scratch.document.layers.len() - 1);
            let surface = &mut scratch.document.layers[layer].surface;
            brush::smudge(surface, from, to, painted, pen.hardness, strength);
            scratch.document.layers[layer].surface.mark(rect);
        }
        Command::Clear { layer, color } => {
            let (width, height) = (scratch.document.width(), scratch.document.height());
            let layer = layer.min(scratch.document.layers.len() - 1);
            let surface = &mut scratch.document.layers[layer].surface;
            for y in 0..height {
                for x in 0..width {
                    surface.set(x, y, color);
                }
            }
        }
    }
}
