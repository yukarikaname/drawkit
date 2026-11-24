//! The tools the shell needs: a brush, a fill, a clear, and the tape they are recorded on.
//!
//! Every tool returns the rectangle it touched, because that rectangle is the undo step and the
//! upload, and a tool that returns "somewhere in the canvas" forces the caller to copy the whole
//! thing. Every tool is also one event on the recording, so a stroke is replayed from numbers
//! rather than from pixels.

use crate::brush::{self, Brush, BrushKind};
use crate::color::{Rgba, over};
use crate::document::{Document, Layer, LayerKind, LayerMeta};

mod erase;
mod fill;
mod history;
mod io;
mod paint;
mod replay;
mod selection;
mod shape;
mod tape;

use self::replay::ReplayCache;
use crate::fill::{FillOptions, close_ink, edge_coverage, erode, feather, flood, grow};
use crate::history::History;
use crate::predict::Predictor;
use crate::record::{Command, Recording};
use crate::surface::Rect;

/// The paper behind the layers. A fill needs to know what the paint will be seen against, and on a
/// transparent layer that is this.
pub const PAPER: Rgba = [1.0, 1.0, 1.0, 1.0];

/// The pen data that is not position: pressure and the three angles a tablet reports.
#[derive(Clone, Copy, Debug)]
pub struct Dynamics {
    pub pressure: f32,
    pub tilt: [f32; 2],
    pub azimuth: f32,
    pub roll: f32,
}

impl Dynamics {
    pub fn pressure(pressure: f32) -> Self {
        Self {
            pressure,
            tilt: [0.0; 2],
            azimuth: 0.0,
            roll: 0.0,
        }
    }
}

/// Symmetry around the canvas centre. The axis can point anywhere; `both` adds the perpendicular
/// axis and the 180 degree rotation, which is the four-way symmetry the old H/V pair expressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mirror {
    pub enabled: bool,
    /// Axis direction in radians; 0 is horizontal, pi/2 is vertical.
    pub angle: f32,
    pub both: bool,
}

type MirroredPoints = ([((f32, f32), (f32, f32)); 4], usize);

impl Mirror {
    pub const NONE: Self = Self {
        enabled: false,
        angle: 0.0,
        both: false,
    };

    /// The legacy four modes: 0 off, 1 left/right, 2 top/bottom, 3 both.
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self {
                enabled: true,
                angle: std::f32::consts::FRAC_PI_2,
                both: false,
            },
            2 => Self {
                enabled: true,
                angle: 0.0,
                both: false,
            },
            3 => Self {
                enabled: true,
                angle: 0.0,
                both: true,
            },
            _ => Self::NONE,
        }
    }

    /// The legacy view, for shells that still offer four buttons.
    pub fn as_i32(self) -> i32 {
        if !self.enabled {
            return 0;
        }
        if self.both {
            return 3;
        }
        let turn = self.angle.rem_euclid(std::f32::consts::PI);
        if (turn - std::f32::consts::FRAC_PI_2).abs() < 0.01 {
            1
        } else {
            2
        }
    }

    pub fn variants(
        self,
        width: usize,
        height: usize,
        from: (f32, f32),
        to: (f32, f32),
    ) -> MirroredPoints {
        let mut points = [(from, to); 4];
        if !self.enabled {
            return (points, 1);
        }
        let centre = ((width as f32 - 1.0) * 0.5, (height as f32 - 1.0) * 0.5);
        let reflect = |point: (f32, f32), angle: f32| {
            let direction = (angle.cos(), angle.sin());
            let offset = (point.0 - centre.0, point.1 - centre.1);
            let along = offset.0 * direction.0 + offset.1 * direction.1;
            (
                centre.0 + 2.0 * along * direction.0 - offset.0,
                centre.1 + 2.0 * along * direction.1 - offset.1,
            )
        };
        let mut count = 1;
        points[count] = (reflect(from, self.angle), reflect(to, self.angle));
        count += 1;
        if self.both {
            let perpendicular = self.angle + std::f32::consts::FRAC_PI_2;
            points[count] = (reflect(from, perpendicular), reflect(to, perpendicular));
            count += 1;
            // A point reflected through both axes is the 180 degree rotation.
            points[count] = (
                (2.0 * centre.0 - from.0, 2.0 * centre.1 - from.1),
                (2.0 * centre.0 - to.0, 2.0 * centre.1 - to.1),
            );
            count += 1;
        }
        (points, count)
    }
}

#[derive(Clone, Copy)]
struct StrokeSession {
    group: u64,
    seed: u64,
    history_depth: usize,
    layer: usize,
    /// The last segment that actually reached the surface, for duplicate detection.
    last: Option<LastSegment>,
}

#[derive(Clone, Copy)]
struct LastSegment {
    from: (f32, f32),
    to: (f32, f32),
    painted: f32,
    ink: Rgba,
    dynamics: Dynamics,
}

#[derive(Clone)]
struct Prediction {
    layer: usize,
    saved: Vec<(Rect, Vec<Rgba>)>,
}

impl LastSegment {
    fn repeats(
        &self,
        from: (f32, f32),
        to: (f32, f32),
        painted: f32,
        ink: Rgba,
        dynamics: Dynamics,
    ) -> bool {
        let near =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() <= 0.01 && (a.1 - b.1).abs() <= 0.01;
        near(self.from, from)
            && near(self.to, to)
            && (self.painted - painted).abs() <= 0.001
            && (self.ink[0] - ink[0]).abs() <= 0.001
            && (self.ink[1] - ink[1]).abs() <= 0.001
            && (self.ink[2] - ink[2]).abs() <= 0.001
            && (self.ink[3] - ink[3]).abs() <= 0.001
            && (self.dynamics.tilt[0] - dynamics.tilt[0]).abs() <= 0.001
            && (self.dynamics.tilt[1] - dynamics.tilt[1]).abs() <= 0.001
            && (self.dynamics.azimuth - dynamics.azimuth).abs() <= 0.001
            && (self.dynamics.roll - dynamics.roll).abs() <= 0.001
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerField {
    Name,
    Visible,
    Opacity,
    Blend,
    Locked,
    AlphaLocked,
    Clipping,
    ColorTag,
    Reference,
    Expanded,
}

#[derive(Clone)]
enum LayerAction {
    State {
        serial: u64,
        index: usize,
        field: LayerField,
        before: LayerMeta,
        after: LayerMeta,
    },
    Structure {
        serial: u64,
        removed: Vec<(usize, Layer)>,
        inserted: Vec<(usize, Layer)>,
        active_before: usize,
        active_after: usize,
    },
    Reorder {
        serial: u64,
        before_index: usize,
        after_index: usize,
        steps: i32,
        active_before: usize,
        active_after: usize,
    },
}

impl LayerAction {
    fn serial(&self) -> u64 {
        match self {
            Self::State { serial, .. }
            | Self::Structure { serial, .. }
            | Self::Reorder { serial, .. } => *serial,
        }
    }
}

#[derive(Clone)]
pub struct Editor {
    pub document: Document,
    pub history: History,
    pub recording: Recording,
    pub recording_enabled: bool,
    time_ms: u64,
    stroke: Option<StrokeSession>,
    next_group: u64,
    next_seed: u64,
    stabilization: f32,
    smoothed: Option<(f32, f32)>,
    pub mirror: Mirror,
    pub brush: Brush,
    /// Pull the filled colour towards the colour already in the region when they are close. The
    /// command records the snapped colour, so replay does not need the flag.
    pub fill_snap: bool,
    next_serial: u64,
    layer_undo: Vec<LayerAction>,
    layer_redo: Vec<LayerAction>,
    /// Drop a segment that repeats the previous one exactly. A stationary pen emits the same
    /// sample repeatedly; blending each one would darken a spot the painter never moved over.
    pub stroke_dedupe: bool,
    /// Draw a short extrapolated segment ahead of the pen and retract it when the real sample
    /// arrives. The prediction is never recorded and never in the undo stack.
    pub prediction_enabled: bool,
    /// Replace a finished stroke with the shape a tiny on-device model recognises.
    pub auto_shape: bool,
    predictor: Predictor,
    prediction: Option<Prediction>,
    replay_cache: Option<Box<ReplayCache>>,
    /// A transient rectangular selection. It is not part of the tape or the undo stack.
    selection: Option<Rect>,
}

/// A replay starts from the same layer tree with empty paint surfaces. Groups, fill layers,
/// visibility, opacity and blend modes are part of how the painting looks, so a scrub or timelapse
/// that ignored them would show a different picture from the canvas.
///
/// A paint layer with pixels but no events is an import or a bake; it has no history to replay, so
/// its pixels are copied as the base. A layer that was imported and painted on has events and is
/// rebuilt from them, exactly like the project cache does on open.
/// Keep a layer name usable as a file name on every platform.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .take(60)
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "layer".to_string()
    } else {
        cleaned.to_string()
    }
}

impl Editor {
    pub fn new(width: usize, height: usize, history_bytes: usize) -> Self {
        Self::new_with_color_space(
            width,
            height,
            history_bytes,
            crate::document::ColorSpace::Srgb,
        )
    }

    pub fn new_with_color_space(
        width: usize,
        height: usize,
        history_bytes: usize,
        color_space: crate::document::ColorSpace,
    ) -> Self {
        let mut document = Document::new(width, height);
        document.color_space = color_space;
        Self {
            document,
            history: History::new(history_bytes),
            recording: Recording::default(),
            // Capture is on by default: the tape is a few kilobytes and is the source a
            // resolution-independent export will need. The shell's record button still controls
            // its replay UI.
            recording_enabled: true,
            time_ms: 0,
            stroke: None,
            next_group: 1,
            next_seed: 0xD1B54A32D192ED03,
            stabilization: 0.0,
            smoothed: None,
            mirror: Mirror::NONE,
            brush: Brush::soft_round(),
            fill_snap: true,
            next_serial: 1,
            layer_undo: Vec::new(),
            layer_redo: Vec::new(),
            stroke_dedupe: true,
            prediction_enabled: false,
            auto_shape: false,
            predictor: Predictor::new(6),
            prediction: None,
            replay_cache: None,
            selection: None,
        }
    }

    /// Input smoothing, 0 (off) to 0.95 (heavy lag). The ink follows a point that moves a fraction
    /// of the way to each sample, which is the pull-string stabiliser a painter expects: the pen
    /// can shake, the line does not. The smoothed segments are what gets recorded, so replay is the
    /// line that was painted rather than the samples that were rejected.
    pub fn set_stabilization(&mut self, strength: f32) {
        self.stabilization = strength.clamp(0.0, 0.95);
    }

    pub fn set_mirror(&mut self, mirror: Mirror) {
        self.mirror = mirror;
    }

    /// Choose the preset the next segment uses. A stroke already in progress keeps its shape
    /// because each segment reads the brush when it is painted.
    pub fn set_brush(&mut self, kind: BrushKind) {
        self.brush = Brush::for_kind(kind);
    }
}

#[cfg(test)]
mod tests;
