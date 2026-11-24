//! Where the pen is going, from where it has been.
//!
//! A tablet reports a position every few milliseconds, and the display shows it some milliseconds
//! later; at speed the ink visibly trails the nib. The usual fix is to draw *ahead* of the last
//! sample - predict the next point, draw that, and correct when the real one arrives. The
//! prediction has to be short (one or two frames) and cheap (no model, no allocation), because it
//! runs inside the input callback.
//!
//! What this is: a straight line fitted through the last few samples, evaluated at `now + horizon`.
//! A curve is a line for a few milliseconds, and the correction the next sample brings is in the
//! same place the eye is looking - which is what makes the cheat invisible.

use std::collections::VecDeque;

/// A point and when it was reported.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub time: f64,
    pub point: [f32; 2],
}

#[derive(Clone)]
pub struct Predictor {
    samples: VecDeque<Sample>,
    capacity: usize,
}

impl Predictor {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity.max(2)),
            capacity: capacity.max(2),
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn push(&mut self, time: f64, point: [f32; 2]) {
        if let Some(last) = self.samples.back() {
            // Time that does not move forward is not a sample: two events on the same millisecond
            // would divide by zero in the fit below, and the pen did not teleport.
            if time <= last.time {
                return;
            }
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { time, point });
    }

    /// The point `horizon` seconds after `time`, by least squares through the recent samples.
    ///
    /// `None` until there are two samples: one point says nothing about direction, and guessing
    /// "stay here" is what a predictor that cannot predict does anyway.
    pub fn predict(&self, time: f64, horizon: f64) -> Option<[f32; 2]> {
        if self.samples.len() < 2 {
            return None;
        }
        let target = time + horizon;
        let count = self.samples.len() as f64;
        let (mut sum_t, mut sum_tt) = (0.0, 0.0);
        let mut sum_x = [0.0f64; 2];
        let mut sum_tx = [0.0f64; 2];
        for sample in &self.samples {
            let t = sample.time;
            sum_t += t;
            sum_tt += t * t;
            for axis in 0..2 {
                sum_x[axis] += sample.point[axis] as f64;
                sum_tx[axis] += t * sample.point[axis] as f64;
            }
        }
        let denominator = count * sum_tt - sum_t * sum_t;
        if denominator.abs() < 1e-9 {
            return self.samples.back().map(|sample| sample.point);
        }
        let mut out = [0.0f32; 2];
        for axis in 0..2 {
            let slope = (count * sum_tx[axis] - sum_t * sum_x[axis]) / denominator;
            let intercept = (sum_x[axis] - slope * sum_t) / count;
            out[axis] = (intercept + slope * target) as f32;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_line_is_predicted_exactly() {
        let mut predictor = Predictor::new(8);
        for step in 0..6 {
            let t = step as f64 * 0.01;
            predictor.push(t, [t as f32 * 100.0, t as f32 * -40.0]);
        }
        // The next event is 10 ms after the last one; a constant-velocity pen is exactly there.
        let predicted = predictor.predict(0.05, 0.01).unwrap();
        assert!((predicted[0] - 6.0).abs() < 0.1, "{predicted:?}");
        assert!((predicted[1] + 2.4).abs() < 0.1, "{predicted:?}");
    }

    #[test]
    fn one_sample_predicts_nothing() {
        let mut predictor = Predictor::new(8);
        predictor.push(0.0, [1.0, 2.0]);
        assert!(predictor.predict(0.0, 0.01).is_none());
    }

    #[test]
    fn time_that_does_not_move_is_ignored() {
        let mut predictor = Predictor::new(8);
        predictor.push(0.01, [0.0, 0.0]);
        predictor.push(0.01, [5.0, 5.0]);
        assert!(
            predictor.predict(0.01, 0.01).is_none(),
            "a stale sample became a direction"
        );
    }

    #[test]
    fn a_curve_is_followed_not_overshot() {
        // A quarter turn sampled every 10 ms; the prediction is one step ahead of several samples
        // ago, so it may lead the curve, but it must stay near it rather than fly off along the
        // first tangent.
        let mut predictor = Predictor::new(6);
        let mut last = [0.0f32, 0.0];
        for step in 0..6 {
            let angle = step as f32 * 0.15;
            last = [angle.cos() * 100.0, angle.sin() * 100.0];
            predictor.push(step as f64 * 0.01, last);
        }
        let predicted = predictor.predict(0.05, 0.01).unwrap();
        let distance = ((predicted[0] - last[0]).powi(2) + (predicted[1] - last[1]).powi(2)).sqrt();
        assert!(distance < 20.0, "prediction jumped {distance} px");
    }
}
