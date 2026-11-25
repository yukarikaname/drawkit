//! Model-assisted gap closing for the fill. The drawn line stays the primary barrier; the edge
//! model supplies the faint continuation of a line that the raster has a hole in.

use super::*;

/// The image a fill's flood runs on: the composite with the requested gap closing and, when the
/// caller asked for it, the edge model's lines added as a barrier. A one-pixel break in the
/// drawn line is exactly what the model can infer and the morphology cannot.
pub fn sealed_for_fill(
    image: &[Rgba],
    width: usize,
    height: usize,
    paper: Rgba,
    options: FillOptions,
) -> Vec<Rgba> {
    let mut sealed = close_ink(image, width, height, paper, options.close);
    if options.smart
        && let Some(edges) = model_edges(image, width, height)
    {
        seal_with_edges(&mut sealed, &edges, 0.4);
    }
    sealed
}

/// Pixels the model sees as an edge become ink. The threshold is deliberately lower than the
/// line-art default: a faint continuation of a line should still stop the fill.
pub fn seal_with_edges(image: &mut [Rgba], edges: &[f32], threshold: f32) {
    for (pixel, edge) in image.iter_mut().zip(edges) {
        if *edge >= threshold {
            *pixel = [0.0, 0.0, 0.0, 1.0];
        }
    }
}

fn model_edges(image: &[Rgba], width: usize, height: usize) -> Option<Vec<f32>> {
    if width == 0 || height == 0 || image.len() < width * height {
        return None;
    }
    let mut bytes = Vec::with_capacity(width * height * 4);
    for pixel in image.iter().take(width * height) {
        bytes.push((pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push(255);
    }
    drawkit_lineart::edges(&bytes, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edge_map_closes_a_gap_the_plain_fill_leaks_through() {
        let (width, height) = (9usize, 9usize);
        let paper = [1.0, 1.0, 1.0, 1.0];
        let mut image = vec![paper; width * height];
        for x in 0..width {
            if x != 4 {
                image[4 * width + x] = [0.0, 0.0, 0.0, 1.0];
            }
        }
        let sealed = close_ink(&image, width, height, paper, 0);
        let leak = flood(&sealed, width, height, (4, 1), 0.1);
        assert!(
            leak.get(4, 7) > 0.5,
            "the plain fill should leak through the gap"
        );

        let mut edges = vec![0.0; width * height];
        edges[4 * width + 4] = 1.0;
        let mut closed = sealed;
        seal_with_edges(&mut closed, &edges, 0.4);
        let mask = flood(&closed, width, height, (4, 1), 0.1);
        assert_eq!(mask.get(4, 7), 0.0, "the model edge did not close the gap");
    }
}
