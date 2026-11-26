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
            strength,
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
                strength,
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
            smart,
        } => {
            let layer = layer.min(scratch.document.layers.len() - 1);
            let under = scratch.document.layers[layer].surface.get(x, y)[3] < 0.5;
            let (width, height) = (scratch.document.width(), scratch.document.height());
            let rect = Rect::new(0, 0, width, height);
            let composite: Vec<Rgba> = scratch
                .document
                .composite_rect(rect)
                .into_iter()
                .map(|pixel| over(pixel, PAPER))
                .collect();
            let Some(soft) = crate::fill::mask_for_fill(
                &composite,
                width,
                height,
                (x, y),
                tolerance,
                crate::fill::FillOptions {
                    close,
                    expand,
                    feather: feather_radius,
                    smart,
                },
                under,
            ) else {
                return;
            };
            let alpha_locked = scratch.document.layers[layer].alpha_locked;
            let target = &mut scratch.document.layers[layer].surface;
            if alpha_locked {
                target.blend_mask_locked_at(&soft, 0, 0, color);
            } else if under {
                target.blend_mask_under_at(&soft, 0, 0, color);
            } else {
                target.blend_mask(&soft, color);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn solid() -> Editor {
        let mut editor = Editor::new(64, 64, 1 << 20);
        for y in 0..64 {
            for x in 0..64 {
                editor.document.layers[0]
                    .surface
                    .set(x, y, [0.0, 0.0, 0.0, 1.0]);
            }
        }
        editor
    }

    #[test]
    fn a_replayed_erase_matches_a_live_erase_at_the_same_strength() {
        let mut live = solid();
        live.set_eraser_strength(0.5);
        live.erase_at((10.0, 32.0), (54.0, 32.0), 5.0, 1.0, 0);
        assert!(
            (live.document.layers[0].surface.get(32, 32)[3] - 0.5).abs() < 0.05,
            "the live erase did not leave half the alpha"
        );
        let command = live.recording.events()[0].command;

        let mut replayed = solid();
        // The replay must use the recorded strength, not the live setting.
        replayed.set_eraser_strength(1.0);
        let mut phase = 0.0;
        apply_replay_command(&mut replayed, command, &mut phase);

        for y in 0..64 {
            for x in 0..64 {
                let expected = live.document.layers[0].surface.get(x, y);
                let actual = replayed.document.layers[0].surface.get(x, y);
                assert!(
                    (expected[3] - actual[3]).abs() < 1e-5,
                    "alpha differs at ({x}, {y}): {expected:?} vs {actual:?}"
                );
            }
        }
    }
}
