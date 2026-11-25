//! Brush presets on disk.
//!
//! The file is MyPaint's `.myb` JSON, the one brush interchange other painting apps already read.
//! MyPaint has more settings than this engine, so an imported brush maps the values that exist and
//! leaves the rest at their defaults; no second brush format is introduced.

use std::path::Path;

use serde_json::{Value, json};

use super::Brush;

/// One brush as a file. The name is display-only; the engine reads the parameters.
#[derive(Clone, Debug)]
pub struct BrushPreset {
    pub name: String,
    pub brush: Brush,
}

impl BrushPreset {
    pub fn from_current(name: impl Into<String>, brush: &Brush) -> Self {
        Self {
            name: name.into(),
            brush: *brush,
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_mypaint()).map_err(|error| error.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Self::from_mypaint(&text)
    }

    fn to_mypaint(&self) -> String {
        let brush = &self.brush;
        let dabs = if brush.spacing > 0.0 {
            2.0 / brush.spacing
        } else {
            8.0
        };
        serde_json::to_string_pretty(&json!({
            "version": 3,
            "name": self.name,
            "description": "Exported by DrawKit",
            "settings": {
                "radius_logarithmic": { "base_value": 1.0 },
                "hardness": { "base_value": brush.hardness },
                "opaque": { "base_value": brush.flow },
                "dabs_per_basic_radius": { "base_value": dabs.clamp(0.5, 50.0) },
                "elliptical_dab_ratio": { "base_value": brush.roundness },
                "angle": { "base_value": brush.angle.to_radians() },
                "random_radius": { "base_value": brush.jitter },
                "random_opaque": { "base_value": brush.grain },
                "pressure_logarithmic": { "base_value": brush.pressure_radius },
                "pressure_opaque": { "base_value": brush.pressure_alpha },
                "smudge": { "base_value": 0.0 },
            },
        }))
        .unwrap_or_default()
    }

    fn from_mypaint(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let settings = value["settings"]
            .as_object()
            .ok_or_else(|| "MyPaint brush has no settings".to_string())?;
        let read = |key: &str, fallback: f32| {
            settings
                .get(key)
                .and_then(|entry| entry["base_value"].as_f64())
                .map_or(fallback, |value| value as f32)
        };
        let mut brush = Brush::soft_round();
        brush.hardness = read("hardness", 0.5).clamp(0.0, 1.0);
        brush.flow = read("opaque", 0.8).clamp(0.01, 1.0);
        let dabs = read("dabs_per_basic_radius", 4.0).max(0.5);
        brush.spacing = (1.0 / (dabs / 2.0)).clamp(0.02, 2.0);
        brush.roundness = read("elliptical_dab_ratio", 1.0).clamp(0.05, 1.0);
        brush.angle = read("angle", 0.0).to_degrees();
        brush.jitter = read("random_radius", 0.0).clamp(0.0, 2.0);
        brush.grain = read("random_opaque", 0.0).clamp(0.0, 1.0);
        brush.pressure_radius = read("pressure_logarithmic", 0.25).clamp(0.0, 1.0);
        brush.pressure_alpha = read("pressure_opaque", 0.0).clamp(0.0, 1.0);
        Ok(Self {
            name: value["name"]
                .as_str()
                .unwrap_or("Imported brush")
                .to_string(),
            brush,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::BrushKind;
    use super::*;

    #[test]
    fn a_mypaint_round_trip_keeps_the_shared_parameters() {
        let brush = Brush::for_kind(BrushKind::Bristle);
        let preset = BrushPreset::from_current("Dry bristle", &brush);
        let loaded = BrushPreset::from_mypaint(&preset.to_mypaint()).expect("parse");
        assert_eq!(loaded.name, "Dry bristle");
        assert!((loaded.brush.hardness - brush.hardness).abs() < 1e-3);
        assert!((loaded.brush.roundness - brush.roundness).abs() < 1e-3);
        assert!((loaded.brush.spacing - brush.spacing).abs() < 1e-2);
    }

    #[test]
    fn a_mypaint_brush_maps_the_shared_settings() {
        let brush = Brush {
            hardness: 0.72,
            flow: 0.61,
            spacing: 0.25,
            roundness: 0.4,
            angle: 30.0,
            jitter: 0.12,
            ..Brush::soft_round()
        };
        let preset = BrushPreset::from_current("Ink wash", &brush);
        let loaded = BrushPreset::from_mypaint(&preset.to_mypaint()).expect("parse mypaint");
        assert_eq!(loaded.name, "Ink wash");
        assert!((loaded.brush.hardness - 0.72).abs() < 1e-3);
        assert!((loaded.brush.roundness - 0.4).abs() < 1e-3);
        assert!((loaded.brush.angle - 30.0).abs() < 0.1);
        assert!((loaded.brush.spacing - 0.25).abs() < 1e-3);
    }

    #[test]
    fn a_foreign_json_is_rejected() {
        assert!(BrushPreset::from_mypaint("{\"format\":\"something\"}").is_err());
    }
}
