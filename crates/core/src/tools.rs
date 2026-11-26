//! The tools the shell needs: a brush, a fill, a clear, and the tape they are recorded on.
//!
//! Every tool returns the rectangle it touched, because that rectangle is the undo step and the
//! upload, and a tool that returns "somewhere in the canvas" forces the caller to copy the whole
//! thing. Every tool is also one event on the recording, so a stroke is replayed from numbers
//! rather than from pixels.

use crate::brush::{self, Brush, BrushKind};
use crate::color::{Rgba, over};
use crate::document::{Document, Layer, LayerKind, LayerMeta};
pub use drawkit_lineart::LineArtOptions;

mod animation;
mod branches;
mod canvas;
mod erase;
mod export;
mod fill;
mod frames;
mod history;
mod io;
mod lineart;
mod overview;
mod paint;
mod replay;
mod selection;
mod shape;
mod strokes;
mod tape;
pub(crate) use tape::brush_from_tape;

use crate::fill::{FillOptions, edge_coverage, erode, feather, flood, grow};
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

#[derive(Clone)]
struct PendingStroke {
    layer: usize,
    commands: Vec<Command>,
    index: usize,
    union: Rect,
    before: Vec<Rgba>,
    phase: f32,
}

#[derive(Clone)]
struct StrokeSession {
    group: u64,
    seed: u64,
    history_depth: usize,
    layer: usize,
    /// Distance to the next stamp for each mirror variant, carried across input segments.
    phases: [f32; 4],
    /// The last segment that actually reached the surface, for duplicate detection.
    last: Option<LastSegment>,
    /// The ink fixed at the first sample. The GPU preview paints the whole stroke in one colour,
    /// so the tape has to record that same colour even if the shell changes the foreground while
    /// the pointer is down.
    color: Option<Rgba>,
    deferred: bool,
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
    Reparent {
        serial: u64,
        /// Index before the move, which is the index again after an undo.
        index: usize,
        parent: Option<usize>,
        /// old index -> new index, used to put the flat vector back on undo.
        mapping: Vec<usize>,
        old_parents: Vec<Option<usize>>,
        active_before: usize,
        active_after: usize,
    },
}

impl LayerAction {
    fn serial(&self) -> u64 {
        match self {
            Self::State { serial, .. }
            | Self::Structure { serial, .. }
            | Self::Reorder { serial, .. }
            | Self::Reparent { serial, .. } => *serial,
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
    /// Whether a recorded radius is in document units or output pixels.
    pub brush_scale: drawkit_formats::BrushScale,
    /// One editable definition per preset slot. A stroke records only the kind; the table restores
    /// customised hardness, flow, spacing and the rest.
    pub brushes: [Brush; 9],
    /// Record strokes for the GPU preview without rasterising them on the CPU; end_stroke replays
    /// the recorded group into the surface once.
    pub defer_raster: bool,
    /// A deferred stroke waiting to be rasterised in bounded steps.
    pending_stroke: Option<PendingStroke>,
    /// A GPU commit collecting its tile pixels before one history step is pushed.
    tile_commit: Option<commit::TileCommit>,
    /// One imported paper/tip texture shared by the brush slots.
    pub brush_texture: Option<std::sync::Arc<crate::brush::BrushTexture>>,
    /// Pull the filled colour towards the colour already in the region when they are close. The
    /// command records the snapped colour, so replay does not need the flag.
    pub fill_snap: bool,
    /// Add the edge model's lines to a fill's barrier. Recorded with each fill.
    pub fill_smart: bool,
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
    /// Let a held stroke snap even when the always-on Auto Shape toggle is off.
    pub quick_shape: bool,
    predictor: Predictor,
    prediction: Option<Prediction>,
    /// A transient rectangular selection. It is not part of the tape or the undo stack.
    selection: Option<Rect>,
    /// An in-progress progressive fill. It is not history or tape until it finishes.
    fill_animation: Option<fill::FillAnimation>,
    /// Inactive animation frames. The active frame lives in `document`, `history` and
    /// `recording`; its slot is `None`.
    frames: Vec<Option<frames::Frame>>,
    active_frame: usize,
    pub fps: u16,
    /// Hold of the active frame in milliseconds.
    pub frame_duration: u32,
    /// Whether playback wraps. The range is `[loop_start, loop_end)`; zero means the last frame.
    pub loop_enabled: bool,
    pub loop_start: usize,
    pub loop_end: usize,
    /// Experimental: keep the redo path an undo sequence abandoned as a dormant branch.
    branches_enabled: bool,
    branches: Vec<branches::HistoryBranch>,
    next_branch: u64,
    current_branch: Option<u64>,
    /// The snapshot taken at the first undo of a sequence, turned into a branch on the next edit.
    branch_tip: Option<branches::BranchState>,
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
            brush_scale: drawkit_formats::BrushScale::Canvas,
            brushes: std::array::from_fn(|index| {
                Brush::for_kind(BrushKind::from_i32(index as i32))
            }),
            defer_raster: false,
            pending_stroke: None,
            tile_commit: None,
            brush_texture: None,
            fill_snap: true,
            fill_smart: false,
            next_serial: 1,
            layer_undo: Vec::new(),
            layer_redo: Vec::new(),
            stroke_dedupe: true,
            prediction_enabled: false,
            auto_shape: false,
            quick_shape: false,
            predictor: Predictor::new(6),
            prediction: None,
            selection: None,
            fill_animation: None,
            frames: vec![None],
            active_frame: 0,
            fps: 12,
            frame_duration: 83,
            loop_enabled: true,
            loop_start: 0,
            loop_end: 0,
            branches_enabled: false,
            branches: Vec::new(),
            next_branch: 1,
            current_branch: None,
            branch_tip: None,
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
        let slot = kind.as_i32() as usize;
        self.brush = self
            .brushes
            .get(slot)
            .copied()
            .unwrap_or_else(|| Brush::for_kind(kind));
    }

    /// Edit the current brush and keep its preset slot in sync, so a later save or a replay uses
    /// the same parameters.
    pub fn set_brush_param(&mut self, id: u8, value: f32) {
        self.brush.set_param(id, value);
        let slot = self.brush.kind.as_i32() as usize;
        if let Some(stored) = self.brushes.get_mut(slot) {
            *stored = self.brush;
        }
    }
}

mod commit;

#[cfg(test)]
mod tests;
