use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VariableValue {
    pub value: String,
}

#[derive(Serialize, ToSchema)]
pub struct ReloadResponse {
    pub count: usize,
}

#[derive(Serialize, ToSchema)]
pub struct TemplateList {
    pub templates: Vec<TemplateSummary>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub dpi: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Options>,
    pub format: TemplateFormat,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Options>,
    pub layout: Layout,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct Options(pub BTreeMap<String, Vec<String>>);

impl Options {
    pub fn is_valid_selection(&self, selection: &BTreeMap<String, String>) -> bool {
        selection.iter().all(|(name, choice)| {
            self.0
                .get(name)
                .map(|values| values.iter().any(|entry| entry == choice))
                .unwrap_or(false)
        })
    }

    pub fn allowed(&self) -> &BTreeMap<String, Vec<String>> {
        &self.0
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Default for Point {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct Position(pub [f32; 2]);

impl Default for Position {
    fn default() -> Self {
        Self([0.0, 0.0])
    }
}

impl Position {
    pub fn point(&self) -> Point {
        Point {
            x: self.0[0],
            y: self.0[1],
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoSize {
    Auto,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum SizeValue {
    Value(f32),
    Auto(AutoSize),
}

impl SizeValue {
    pub fn value(&self) -> Option<f32> {
        match self {
            SizeValue::Value(value) => Some(*value),
            SizeValue::Auto(_) => None,
        }
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, SizeValue::Auto(_))
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct Size(pub [SizeValue; 2]);

/// Orthogonal rotation interpreted from the wire `rotate` degrees (counter-clockwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// Canonicalize wire degrees to an orthogonal rotation. `None` for non-finite or
    /// non-multiple-of-90 (within `EPS`). Handles negatives and >360 via `rem_euclid`.
    pub fn from_degrees(deg: f32) -> Option<Rotation> {
        if !deg.is_finite() {
            return None;
        }
        const EPS: f32 = 1.0e-3;
        let norm = deg.rem_euclid(360.0);
        for (target, rot) in [
            (0.0, Rotation::R0),
            (90.0, Rotation::R90),
            (180.0, Rotation::R180),
            (270.0, Rotation::R270),
            (360.0, Rotation::R0),
        ] {
            if (norm - target).abs() < EPS {
                return Some(rot);
            }
        }
        None
    }

    /// 90/270 swap width and height.
    pub fn swaps_axes(self) -> bool {
        matches!(self, Rotation::R90 | Rotation::R270)
    }

    /// Anything other than `R0` triggers the rotated render/validation path.
    pub fn is_rotated(self) -> bool {
        !matches!(self, Rotation::R0)
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Placement {
    #[serde(default)]
    pub at: Position,
    pub size: Size,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_h: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f32>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct SheetPosition(pub [f32; 2]);

impl SheetPosition {
    pub fn point(&self) -> Point {
        Point {
            x: self.0[0],
            y: self.0[1],
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum Dimension {
    Fixed(f32),
    Dynamic { min: Option<f32>, max: Option<f32> },
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum FontSize {
    Fixed(f32),
    Range { min: f32, max: f32 },
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct QrParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_correction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet_zone: Option<f32>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Serialize, ToSchema, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Serialize, ToSchema, Clone, Default, Deserialize)]
pub struct Alignment {
    #[serde(default)]
    pub horizontal: HorizontalAlign,
    #[serde(default)]
    pub vertical: VerticalAlign,
}

#[derive(Debug, Serialize, ToSchema, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

impl Fit {
    pub fn as_typst(&self) -> &'static str {
        match self {
            Fit::Contain => "contain",
            Fit::Cover => "cover",
            Fit::Stretch => "stretch",
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutItem {
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(flatten)]
        placement: Placement,
        font_size: FontSize,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        alignment: Alignment,
    },
    Qr {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(flatten)]
        placement: Placement,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<QrParams>,
    },
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        src: Option<String>,
        #[serde(flatten)]
        placement: Placement,
        #[serde(default)]
        fit: Fit,
    },
    Line {
        #[serde(default)]
        at: Position,
        to: Position,
        thickness: f32,
    },
    Container {
        #[serde(flatten)]
        placement: Placement,
        #[serde(skip_serializing_if = "Option::is_none")]
        option: Option<BTreeMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        frame: Option<Frame>,
        #[serde(default)]
        padding: Padding,
        #[schema(no_recursion)]
        items: Vec<LayoutItem>,
    },
}

#[derive(Debug, Serialize, ToSchema, Clone, Copy, PartialEq, Deserialize)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const ZERO: Padding = Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
}

impl Default for Padding {
    fn default() -> Self {
        Padding::ZERO
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Frame {
    pub thickness: f32,
    #[serde(default)]
    pub rounded: bool,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum Layout {
    Items(Vec<LayoutItem>),
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateFormat {
    Sheet {
        paper_width: f32,
        paper_height: f32,
        label_width: f32,
        label_height: f32,
        positions: Vec<SheetPosition>,
    },
    Single {
        width: Dimension,
        height: Dimension,
        #[serde(default)]
        media_width: Option<f32>,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenderLabelRequest {
    pub template: String,
    #[serde(flatten)]
    pub label: LabelInput,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchRequest {
    pub template: String,
    pub labels: Vec<LabelInput>,
    pub mode: String,
    #[serde(default)]
    pub printer: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub start_slot: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchRowError {
    pub index: usize,
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: Vec<BatchRowError>,
    pub jobs: usize,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LabelInput {
    pub data: HashMap<String, Value>,
    #[serde(default)]
    pub option: Option<BTreeMap<String, String>>,
}

fn default_copies() -> u32 {
    1
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PrintRequest {
    pub template: String,
    pub printer: String,
    #[serde(default)]
    pub fields: HashMap<String, Value>,
    #[serde(default)]
    pub option: Option<BTreeMap<String, String>>,
    #[serde(default = "default_copies")]
    pub copies: u32,
}

#[cfg(test)]
mod rotation_tests {
    use super::Rotation;

    #[test]
    fn from_degrees_maps_orthogonal_and_wraps() {
        assert_eq!(Rotation::from_degrees(0.0), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(90.0), Some(Rotation::R90));
        assert_eq!(Rotation::from_degrees(180.0), Some(Rotation::R180));
        assert_eq!(Rotation::from_degrees(270.0), Some(Rotation::R270));
        assert_eq!(Rotation::from_degrees(360.0), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(-90.0), Some(Rotation::R270));
        assert_eq!(Rotation::from_degrees(-0.0), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(359.9999), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(450.0), Some(Rotation::R90));
    }

    #[test]
    fn from_degrees_rejects_non_orthogonal_and_non_finite() {
        assert_eq!(Rotation::from_degrees(45.0), None);
        assert_eq!(Rotation::from_degrees(f32::NAN), None);
        assert_eq!(Rotation::from_degrees(f32::INFINITY), None);
        assert_eq!(Rotation::from_degrees(f32::NEG_INFINITY), None);
    }

    #[test]
    fn axis_and_rotated_predicates() {
        assert!(Rotation::R90.swaps_axes() && Rotation::R270.swaps_axes());
        assert!(!Rotation::R0.swaps_axes() && !Rotation::R180.swaps_axes());
        assert!(Rotation::R90.is_rotated() && Rotation::R180.is_rotated());
        assert!(!Rotation::R0.is_rotated());
    }
}
