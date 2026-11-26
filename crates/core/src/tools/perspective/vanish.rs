//! Vanishing point voting: every pair of lines proposes an intersection, and the proposal with
//! the most supporting lines wins. Lines that support a point are removed before the next round.

use std::f32::consts::PI;

use super::detect::Segment;

pub(super) struct Point {
    pub at_infinity: bool,
    pub x: f32,
    pub y: f32,
    pub direction_degrees: f32,
    pub support: usize,
}

struct Config {
    angle_tolerance: f32,
    support_tolerance: f32,
    far_limit: f32,
    centre_x: f32,
    centre_y: f32,
}

struct Candidate {
    at_infinity: bool,
    x: f32,
    y: f32,
    direction: f32,
    supports: Vec<usize>,
    score: f32,
    key: (usize, usize),
}

pub(super) fn estimate(
    segments: &[Segment],
    width: usize,
    height: usize,
    angle_tolerance_degrees: f32,
    support_tolerance: f32,
    max_points: usize,
) -> Vec<Point> {
    let diagonal = (((width * width + height * height) as f32).sqrt()).max(1.0);
    let config = Config {
        angle_tolerance: angle_tolerance_degrees.clamp(0.5, 15.0).to_radians(),
        support_tolerance: support_tolerance.clamp(1.0, 16.0),
        far_limit: diagonal * 12.0,
        centre_x: width as f32 * 0.5,
        centre_y: height as f32 * 0.5,
    };
    let considered = segments.len();
    let mut remaining: Vec<usize> = (0..considered).collect();
    let mut found = Vec::new();
    while found.len() < max_points && remaining.len() >= 2 {
        let min_support = if remaining.len() <= 4 { 2 } else { 3 };
        let Some(candidate) = best(&remaining, segments, &config, min_support) else {
            break;
        };
        found.push(Point {
            at_infinity: candidate.at_infinity,
            x: candidate.x,
            y: candidate.y,
            direction_degrees: if candidate.at_infinity {
                candidate.direction.to_degrees().rem_euclid(180.0)
            } else {
                0.0
            },
            support: candidate.supports.len(),
        });
        remaining.retain(|index| !candidate.supports.contains(index));
    }
    found
}

fn best(
    remaining: &[usize],
    segments: &[Segment],
    config: &Config,
    min_support: usize,
) -> Option<Candidate> {
    let parallel = config.angle_tolerance.sin();
    let mut best: Option<Candidate> = None;
    for first in 0..remaining.len() {
        for second in (first + 1)..remaining.len() {
            let i = remaining[first];
            let j = remaining[second];
            let a = &segments[i];
            let b = &segments[j];
            let cross = a.dx * b.dy - a.dy * b.dx;
            let candidate = if cross.abs() <= parallel {
                infinite(&[i, j], remaining, segments, config, (i, j))
            } else {
                let rx = b.x - a.x;
                let ry = b.y - a.y;
                let along = (rx * b.dy - ry * b.dx) / cross;
                let x = a.x + along * a.dx;
                let y = a.y + along * a.dy;
                if (x - config.centre_x).hypot(y - config.centre_y) > config.far_limit {
                    // A finite point this far out cannot be drawn or used; the family is parallel
                    // for every practical purpose.
                    infinite(&[i, j], remaining, segments, config, (i, j))
                } else {
                    finite(x, y, remaining, segments, config, (i, j))
                }
            };
            let Some(candidate) = candidate else {
                continue;
            };
            if candidate.supports.len() < min_support {
                continue;
            }
            if better(&best, &candidate) {
                best = Some(candidate);
            }
        }
    }
    best
}

fn better(current: &Option<Candidate>, next: &Candidate) -> bool {
    let Some(current) = current else {
        return true;
    };
    let support = next.supports.len().cmp(&current.supports.len());
    if support != std::cmp::Ordering::Equal {
        return support == std::cmp::Ordering::Greater;
    }
    if (next.score - current.score).abs() > 1e-3 {
        return next.score > current.score;
    }
    next.key < current.key
}

fn finite(
    seed_x: f32,
    seed_y: f32,
    remaining: &[usize],
    segments: &[Segment],
    config: &Config,
    key: (usize, usize),
) -> Option<Candidate> {
    let mut x = seed_x;
    let mut y = seed_y;
    let mut supports = Vec::new();
    for _ in 0..4 {
        supports.clear();
        for &index in remaining {
            if distance(x, y, &segments[index]) <= config.support_tolerance {
                supports.push(index);
            }
        }
        if supports.len() < 2 {
            return None;
        }
        let (next_x, next_y) = least_squares(&supports, segments)?;
        let moved = (next_x - x).hypot(next_y - y);
        x = next_x;
        y = next_y;
        if moved < 0.01 {
            break;
        }
    }
    supports.clear();
    for &index in remaining {
        if distance(x, y, &segments[index]) <= config.support_tolerance {
            supports.push(index);
        }
    }
    if supports.len() < 2 {
        return None;
    }
    Some(Candidate {
        at_infinity: false,
        x,
        y,
        direction: 0.0,
        score: total_length(&supports, segments),
        supports,
        key,
    })
}

fn infinite(
    seed: &[usize],
    remaining: &[usize],
    segments: &[Segment],
    config: &Config,
    key: (usize, usize),
) -> Option<Candidate> {
    let mut angle = mean_angle(seed, segments)?;
    let mut supports = Vec::new();
    for _ in 0..3 {
        supports.clear();
        for &index in remaining {
            if angle_between(angle, line_angle(&segments[index])) <= config.angle_tolerance {
                supports.push(index);
            }
        }
        if supports.len() < 2 {
            return None;
        }
        let next = mean_angle(&supports, segments)?;
        let moved = angle_between(next, angle);
        angle = next;
        if moved < 1e-4 {
            break;
        }
    }
    supports.clear();
    for &index in remaining {
        if angle_between(angle, line_angle(&segments[index])) <= config.angle_tolerance {
            supports.push(index);
        }
    }
    if supports.len() < 2 {
        return None;
    }
    Some(Candidate {
        at_infinity: true,
        x: config.centre_x,
        y: config.centre_y,
        direction: angle,
        score: total_length(&supports, segments),
        supports,
        key,
    })
}

fn total_length(indices: &[usize], segments: &[Segment]) -> f32 {
    indices
        .iter()
        .map(|&index| segments[index].length as f64)
        .sum::<f64>() as f32
}

fn distance(x: f32, y: f32, segment: &Segment) -> f32 {
    ((x - segment.x) * segment.dy - (y - segment.y) * segment.dx).abs()
}

fn least_squares(indices: &[usize], segments: &[Segment]) -> Option<(f32, f32)> {
    let mut xx = 0f64;
    let mut xy = 0f64;
    let mut yy = 0f64;
    let mut xc = 0f64;
    let mut yc = 0f64;
    for &index in indices {
        let segment = &segments[index];
        let nx = -(segment.dy as f64);
        let ny = segment.dx as f64;
        let c = nx * segment.x as f64 + ny * segment.y as f64;
        xx += nx * nx;
        xy += nx * ny;
        yy += ny * ny;
        xc += nx * c;
        yc += ny * c;
    }
    let determinant = xx * yy - xy * xy;
    if determinant.abs() < 1e-9 {
        return None;
    }
    Some((
        ((xc * yy - xy * yc) / determinant) as f32,
        ((xx * yc - xy * xc) / determinant) as f32,
    ))
}

fn line_angle(segment: &Segment) -> f32 {
    segment.dy.atan2(segment.dx).rem_euclid(PI)
}

fn angle_between(a: f32, b: f32) -> f32 {
    let difference = (a - b).rem_euclid(PI);
    difference.min(PI - difference)
}

fn mean_angle(indices: &[usize], segments: &[Segment]) -> Option<f32> {
    let mut cosine = 0f64;
    let mut sine = 0f64;
    for &index in indices {
        let segment = &segments[index];
        let angle = line_angle(segment) as f64;
        let weight = segment.length.max(1.0) as f64;
        cosine += (2.0 * angle).cos() * weight;
        sine += (2.0 * angle).sin() * weight;
    }
    if cosine == 0.0 && sine == 0.0 {
        return None;
    }
    Some(((0.5 * sine.atan2(cosine)) as f32).rem_euclid(PI))
}
