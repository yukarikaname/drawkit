//! A tiny on-device stroke classifier. The model is a 10-16-4 tanh network trained on
//! synthetic lines, ellipses, rectangles and freehand scribbles. It runs in the core with no
//! runtime and no network; the weights are the whole model.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Line,
    Ellipse,
    Rectangle,
    Free,
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeGuess {
    pub kind: ShapeKind,
    pub confidence: f32,
}

const HIDDEN_W: [f32; 176] = [
    -0.04269, 0.60172, 0.09142, -0.02915, -0.13570, 0.34615, -0.29596, -2.09517, -0.44241,
    -0.06004, -0.98429, -0.20999, -0.39961, -0.40749, -1.20423, -0.18173, -7.19363, 0.15669,
    0.61462, 1.13238, -13.67006, 3.11183, 1.05768, -2.88317, 1.49120, 1.53541, 1.80477, -1.94273,
    -1.07502, 1.83466, 2.26225, -0.37727, 1.57396, 0.63848, 0.14982, -0.79122, 1.22210, -0.13381,
    -0.15561, -0.17942, -0.60315, -0.24936, -0.66139, 0.57427, 0.65062, 0.12828, -0.04558,
    -0.19216, -2.13722, 2.06520, 3.24201, 1.79481, -4.83888, -1.00868, -2.46409, -0.04308,
    -2.08492, 3.28046, -2.94591, -1.14566, -2.64955, 2.72592, 0.40613, -4.31483, -0.32180,
    -2.25190, -3.36901, -2.30603, 0.36124, -2.19125, 0.73229, 2.27399, -0.24254, -2.90802,
    -0.00121, 2.37322, 1.13271, -3.06482, -2.66900, 2.36607, -0.86434, 0.36806, 0.40892, 1.21922,
    -0.37479, 0.09688, 0.27498, -0.52520, 0.08319, 0.48731, 0.08347, 0.10237, 0.07454, 0.62988,
    0.43130, 0.20749, 1.03940, -1.13942, -3.06249, -1.90031, 0.58947, -2.14823, 0.19731, 3.08981,
    -0.55261, -3.57730, -1.55580, 0.73978, 1.59047, -2.36882, -1.59965, 3.06478, 1.06669, -0.77557,
    -0.10465, -1.37945, 1.58787, -3.73706, -1.18145, 2.31402, -2.79393, -2.20218, -1.52382,
    0.58742, 0.76866, -1.60291, -1.43086, -0.88801, -1.84790, 0.13488, 0.39024, 1.07014, -1.78543,
    0.09987, -1.01712, -0.21331, -0.92356, 0.52128, -0.90298, -0.37232, -0.49186, 0.28954, 0.50438,
    -0.87963, 0.38543, 0.14746, 0.07867, -0.43411, -0.00837, -0.54325, 0.20228, -0.19251, 0.39205,
    -0.64185, 0.56828, -0.67801, -0.29854, -0.30257, -0.45186, -0.12440, 0.54174, 2.42803, 1.60590,
    0.64104, -0.08971, -0.24532, 0.07273, -0.09036, -0.59882, 1.35641, 0.19768, 0.29882, -1.18676,
    0.91658, 0.35643, -1.75055,
];
const HIDDEN_B: [f32; 16] = [
    -0.24236, -0.14057, -0.02511, -0.79082, -0.25265, -1.24894, -0.27200, 1.74309, -0.29166,
    -0.59969, -0.86945, 0.20192, 0.33105, -1.10811, -1.08859, 0.03500,
];
const OUT_W: [f32; 64] = [
    2.16859, -0.80884, 0.13805, -1.63045, 0.75770, -2.60133, 5.20622, -1.11861, -1.72270, -6.24355,
    8.69325, -1.60319, -1.80004, -0.12230, 3.82395, -0.83918, 2.47675, -0.64452, -0.11736,
    -1.69613, -1.34555, 1.63606, 3.22730, -2.82097, -0.90171, 1.16076, 0.03952, -1.29841, 0.81137,
    -1.03344, -4.72896, 3.56583, -0.99759, 1.15913, 0.90131, -1.19847, -0.83041, -3.66663,
    10.54142, -2.03573, -0.90967, 1.30788, 1.24461, -1.30024, 1.80441, -0.90771, -1.47750, 0.59577,
    1.42501, -1.04886, -2.18518, 0.08759, -1.56972, -3.43171, 10.94624, -2.89069, -1.21452,
    0.63534, 5.89027, -2.52096, 1.50150, 2.51994, -5.36167, 0.53292,
];
const OUT_B: [f32; 4] = [-0.25493, 0.47774, -0.90295, 0.55332];

pub fn classify(points: &[(f32, f32)]) -> ShapeGuess {
    let Some(features) = features(points) else {
        return ShapeGuess {
            kind: ShapeKind::Free,
            confidence: 0.0,
        };
    };
    let mut hidden = [0.0_f32; 16];
    for (j, value) in hidden.iter_mut().enumerate() {
        let mut sum = HIDDEN_B[j];
        for i in 0..11 {
            sum += features[i] * HIDDEN_W[i * 16 + j];
        }
        *value = sum.tanh();
    }
    let mut logits = [0.0_f32; 4];
    for (k, logit) in logits.iter_mut().enumerate() {
        let mut sum = OUT_B[k];
        for j in 0..16 {
            sum += hidden[j] * OUT_W[j * 4 + k];
        }
        *logit = sum;
    }
    let max = logits.iter().copied().fold(f32::MIN, f32::max);
    let mut sum = 0.0;
    for value in &mut logits {
        *value = (*value - max).exp();
        sum += *value;
    }
    let mut best = 0;
    for k in 1..4 {
        if logits[k] > logits[best] {
            best = k;
        }
    }
    let kind = match best {
        0 => ShapeKind::Line,
        1 => ShapeKind::Ellipse,
        2 => ShapeKind::Rectangle,
        _ => ShapeKind::Free,
    };
    ShapeGuess {
        kind,
        confidence: logits[best] / sum,
    }
}

/// Resample by arc length and smooth twice, so a shaky hand reads as the shape it meant.
fn features(points: &[(f32, f32)]) -> Option<[f32; 11]> {
    let q = resample(points, 32);
    if q.len() < 4 {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in &q {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    let width = (max_x - min_x).max(1e-6);
    let height = (max_y - min_y).max(1e-6);
    let diagonal = (width * width + height * height).sqrt().max(1e-6);
    let mut length = 0.0;
    for i in 1..q.len() {
        length += distance(q[i - 1], q[i]);
    }
    length = length.max(1e-6);
    let closed = 1.0 - distance(q[0], *q.last().unwrap()) / length;
    let (ax, ay) = q[0];
    let (bx, by) = *q.last().unwrap();
    let line = ((bx - ax) * (bx - ax) + (by - ay) * (by - ay))
        .sqrt()
        .max(1e-6);
    let mut max_deviation = 0.0_f32;
    for (x, y) in &q {
        max_deviation =
            max_deviation.max(((x - ax) * (by - ay) - (y - ay) * (bx - ax)).abs() / line);
    }
    let mut total_turn = 0.0;
    let mut reversals = 0;
    let mut previous = 0.0;
    let mut corners = 0;
    let mut turns = 0;
    let mut turns_abs: Vec<f32> = Vec::new();
    for i in 1..q.len() - 1 {
        let a1 = (q[i].1 - q[i - 1].1).atan2(q[i].0 - q[i - 1].0);
        let a2 = (q[i + 1].1 - q[i].1).atan2(q[i + 1].0 - q[i].0);
        let turn = (a2 - a1 + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI)
            - std::f32::consts::PI;
        total_turn += turn.abs() / (2.0 * std::f32::consts::PI);
        turns_abs.push(turn.abs());
        if turn.abs() > 45.0_f32.to_radians() {
            corners += 1;
        }
        if previous != 0.0 && turn * previous < 0.0 {
            reversals += 1;
        }
        previous = turn;
        turns += 1;
    }
    let mut area = 0.0;
    for i in 1..q.len() {
        area += q[i - 1].0 * q[i].1 - q[i].0 * q[i - 1].1;
    }
    let fill = (area.abs() * 0.5 / (width * height)).min(1.0);
    let center = (
        q.iter().map(|p| p.0).sum::<f32>() / q.len() as f32,
        q.iter().map(|p| p.1).sum::<f32>() / q.len() as f32,
    );
    let mean_radius = q.iter().map(|p| distance(*p, center)).sum::<f32>() / q.len() as f32;
    let variance = q
        .iter()
        .map(|p| {
            let d = distance(*p, center) - mean_radius;
            d * d
        })
        .sum::<f32>()
        / q.len() as f32;
    let radius_variance = (variance.sqrt() / mean_radius.max(1e-6)).min(2.0);
    let mut aligned = 0;
    for i in 1..q.len() {
        let angle = (q[i].1 - q[i - 1].1).atan2(q[i].0 - q[i - 1].0);
        let quarter = angle.rem_euclid(std::f32::consts::FRAC_PI_2);
        let off = quarter.min(std::f32::consts::FRAC_PI_2 - quarter);
        if off < 15.0_f32.to_radians() {
            aligned += 1;
        }
    }
    let aspect = if height <= 1e-5 {
        1.0
    } else {
        ((width / height).max(0.1).ln() / 2.3).clamp(-1.0, 1.0)
    };
    turns_abs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let turn_sum: f32 = turns_abs.iter().sum();
    let concentration = turns_abs.iter().take(4).sum::<f32>() / turn_sum.max(1e-9);
    Some([
        aspect,
        closed.clamp(0.0, 1.0),
        1.0 - (max_deviation / length).min(1.0),
        corners as f32 / turns.max(1) as f32,
        (total_turn / 2.0).min(1.0),
        fill,
        radius_variance / 2.0,
        reversals as f32 / turns.max(1) as f32,
        (length / diagonal).min(5.0) / 5.0,
        aligned as f32 / (q.len().max(2) - 1) as f32,
        concentration,
    ])
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn resample(points: &[(f32, f32)], count: usize) -> Vec<(f32, f32)> {
    let mut clean: Vec<(f32, f32)> = Vec::new();
    for point in points {
        if clean
            .last()
            .is_none_or(|last| distance(*last, *point) > 1e-3)
        {
            clean.push(*point);
        }
    }
    if clean.len() < 2 {
        return clean;
    }
    let mut lengths = vec![0.0_f32];
    for i in 1..clean.len() {
        lengths.push(lengths[i - 1] + distance(clean[i - 1], clean[i]));
    }
    let total = *lengths.last().unwrap();
    if total < 1e-6 {
        return clean;
    }
    let mut out = Vec::with_capacity(count);
    let mut segment = 0;
    for step in 0..count {
        let target = total * step as f32 / (count - 1) as f32;
        while segment + 2 < lengths.len() && lengths[segment + 1] < target {
            segment += 1;
        }
        let span = lengths[segment + 1] - lengths[segment];
        let t = if span < 1e-9 {
            0.0
        } else {
            (target - lengths[segment]) / span
        };
        out.push((
            clean[segment].0 + (clean[segment + 1].0 - clean[segment].0) * t,
            clean[segment].1 + (clean[segment + 1].1 - clean[segment].1) * t,
        ));
    }
    for _ in 0..2 {
        let mut smooth = vec![out[0]];
        for i in 1..out.len() - 1 {
            smooth.push((
                (out[i - 1].0 + 2.0 * out[i].0 + out[i + 1].0) / 4.0,
                (out[i - 1].1 + 2.0 * out[i].1 + out[i + 1].1) / 4.0,
            ));
        }
        smooth.push(*out.last().unwrap());
        out = smooth;
    }
    out
}

/// The ideal path for the guessed shape, in document coordinates. `Line` keeps the real start
/// and end; the closed shapes use the stroke's bounding box.
pub fn ideal_points(kind: ShapeKind, points: &[(f32, f32)], snap: bool) -> Vec<(f32, f32)> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    match kind {
        ShapeKind::Line => {
            let start = points[0];
            let end = *points.last().unwrap();
            if !snap {
                return vec![start, end];
            }
            // Shift pins the direction to the nearest 45 degrees; the drawn length survives.
            let (dx, dy) = (end.0 - start.0, end.1 - start.1);
            let length = (dx * dx + dy * dy).sqrt();
            let step = std::f32::consts::FRAC_PI_4;
            let angle = (dy.atan2(dx) / step).round() * step;
            vec![
                start,
                (
                    start.0 + length * angle.cos(),
                    start.1 + length * angle.sin(),
                ),
            ]
        }
        ShapeKind::Ellipse => {
            let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
            let (mut rx, mut ry) = ((max_x - min_x) / 2.0, (max_y - min_y) / 2.0);
            if snap {
                rx = rx.max(ry);
                ry = rx;
            }
            (0..=48)
                .map(|i| {
                    let t = i as f32 / 48.0 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect()
        }
        ShapeKind::Rectangle => {
            if snap {
                let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
                let half = ((max_x - min_x) / 2.0).max((max_y - min_y) / 2.0);
                min_x = cx - half;
                max_x = cx + half;
                min_y = cy - half;
                max_y = cy + half;
            }
            let corners = [
                (min_x, min_y),
                (max_x, min_y),
                (max_x, max_y),
                (min_x, max_y),
                (min_x, min_y),
            ];
            let mut out = Vec::new();
            for pair in corners.windows(2) {
                for step in 0..12 {
                    let t = step as f32 / 12.0;
                    out.push((
                        pair[0].0 + (pair[1].0 - pair[0].0) * t,
                        pair[0].1 + (pair[1].1 - pair[0].1) * t,
                    ));
                }
            }
            out.push(corners[0]);
            out
        }
        ShapeKind::Free => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Vec<(f32, f32)> {
        (0..32)
            .map(|i| (i as f32 * 5.0, i as f32 * 1.2 + (i % 3) as f32 * 0.3))
            .collect()
    }

    fn circle() -> Vec<(f32, f32)> {
        (0..64)
            .map(|i| {
                let t = i as f32 / 63.0 * std::f32::consts::TAU;
                (100.0 + 70.0 * t.cos(), 80.0 + 45.0 * t.sin())
            })
            .collect()
    }

    fn rectangle() -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for (a, b) in [
            ((0.0, 0.0), (120.0, 0.0)),
            ((120.0, 0.0), (120.0, 60.0)),
            ((120.0, 60.0), (0.0, 60.0)),
            ((0.0, 60.0), (0.0, 0.0)),
        ] {
            for i in 0..16 {
                let t = i as f32 / 16.0;
                out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
        out
    }

    #[test]
    fn shift_snaps_the_ideal_shapes() {
        let line = ideal_points(ShapeKind::Line, &[(0.0, 0.0), (100.0, 12.0)], true);
        assert!(
            line[1].1.abs() < 0.01,
            "the line did not snap horizontal: {line:?}"
        );
        let points = vec![(0.0, 0.0), (100.0, 50.0)];
        let oval = ideal_points(ShapeKind::Ellipse, &points, true);
        let (cx, cy) = (50.0, 25.0);
        let first = distance(oval[0], (cx, cy));
        for point in &oval {
            assert!(
                (distance(*point, (cx, cy)) - first).abs() < 0.05,
                "not a circle"
            );
        }
        let square = ideal_points(ShapeKind::Rectangle, &points, true);
        let xs: Vec<f32> = square.iter().map(|p| p.0).collect();
        let ys: Vec<f32> = square.iter().map(|p| p.1).collect();
        let width = xs.iter().copied().fold(f32::MIN, f32::max)
            - xs.iter().copied().fold(f32::MAX, f32::min);
        let height = ys.iter().copied().fold(f32::MIN, f32::max)
            - ys.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            (width - height).abs() < 0.01,
            "not a square: {width} x {height}"
        );
    }

    #[test]
    fn the_tiny_model_separates_the_four_gestures() {
        assert_eq!(classify(&line()).kind, ShapeKind::Line);
        assert_eq!(classify(&circle()).kind, ShapeKind::Ellipse);
        assert_eq!(classify(&rectangle()).kind, ShapeKind::Rectangle);
        let free: Vec<(f32, f32)> = (0..48)
            .map(|i| {
                (
                    i as f32 * 4.0 + (i as f32 * 0.7).sin() * 30.0,
                    (i as f32 * 0.9).cos() * 30.0,
                )
            })
            .collect();
        assert_eq!(classify(&free).kind, ShapeKind::Free);
    }

    #[test]
    fn ideal_points_are_closed_where_they_should_be() {
        let points = rectangle();
        let ellipse = ideal_points(ShapeKind::Ellipse, &points, false);
        assert!((ellipse.first().unwrap().0 - ellipse.last().unwrap().0).abs() < 1e-4);
        assert!((ellipse.first().unwrap().1 - ellipse.last().unwrap().1).abs() < 1e-4);
        let rect = ideal_points(ShapeKind::Rectangle, &points, false);
        assert!((rect.first().unwrap().0 - rect.last().unwrap().0).abs() < 1e-4);
        assert!((rect.first().unwrap().1 - rect.last().unwrap().1).abs() < 1e-4);
        assert_eq!(ideal_points(ShapeKind::Line, &points, false).len(), 2);
    }
}
