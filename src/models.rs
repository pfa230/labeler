use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
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
    pub margins: Option<Margins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    pub format: TemplateFormat,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub dpi: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margins: Option<Margins>,
    pub format: TemplateFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Options>,
    pub layout: Layout,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct Options(pub Vec<String>);

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct Box(pub [f32; 4]);

impl Box {
    pub fn corners(&self) -> (Point, Point) {
        let [x1, y1, x2, y2] = self.0;
        (Point { x: x1, y: y1 }, Point { x: x2, y: y2 })
    }
}

#[derive(Debug, Serialize, ToSchema, Clone)]
pub struct SheetPosition {
    pub origin: Point,
    pub width: f32,
    pub height: f32,
    #[schema(example = 0)]
    pub rotation_deg: u16,
}

impl SheetPosition {
    fn from_coords(coords: [f32; 4]) -> Result<Self, String> {
        let [x1, y1, x2, y2] = coords;
        let dx = x2 - x1;
        let dy = y2 - y1;
        if dx.abs() < f32::EPSILON || dy.abs() < f32::EPSILON {
            return Err("sheet position must have non-zero width and height".to_string());
        }

        let (rotation_deg, width, height) = if dx > 0.0 && dy > 0.0 {
            (0, dx.abs(), dy.abs())
        } else if dx < 0.0 && dy < 0.0 {
            (180, dx.abs(), dy.abs())
        } else if dx < 0.0 && dy > 0.0 {
            (90, dy.abs(), dx.abs())
        } else {
            (270, dy.abs(), dx.abs())
        };

        Ok(Self {
            origin: Point { x: x1, y: y1 },
            width,
            height,
            rotation_deg,
        })
    }
}

impl<'de> Deserialize<'de> for SheetPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let coords = <[f32; 4]>::deserialize(deserializer)?;
        Self::from_coords(coords).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, Default)]
#[serde(transparent)]
pub struct Margins(pub [f32; 4]);

impl Margins {
    pub fn left(&self) -> f32 {
        self.0[0]
    }

    pub fn bottom(&self) -> f32 {
        self.0[1]
    }

    pub fn right(&self) -> f32 {
        self.0[2]
    }

    pub fn top(&self) -> f32 {
        self.0[3]
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

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

impl Default for HorizontalAlign {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
}

impl Default for VerticalAlign {
    fn default() -> Self {
        Self::Top
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Alignment {
    #[serde(default)]
    pub horizontal: HorizontalAlign,
    #[serde(default)]
    pub vertical: VerticalAlign,
}

impl Default for Alignment {
    fn default() -> Self {
        Self {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutItem {
    Text {
        name: String,
        #[serde(rename = "box")]
        #[schema(rename = "box")]
        bounds: Box,
        font_size: FontSize,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        alignment: Alignment,
    },
    Qr {
        name: String,
        #[serde(rename = "box")]
        #[schema(rename = "box")]
        bounds: Box,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<QrParams>,
    },
    Line {
        start: Point,
        end: Point,
        thickness: f32,
    },
    Rectangle {
        #[serde(rename = "box")]
        #[schema(rename = "box")]
        bounds: Box,
        thickness: f32,
        rounded: bool,
    },
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum Layout {
    Items(Vec<LayoutItem>),
    OptionsLayout(HashMap<String, Vec<LayoutItem>>),
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateFormat {
    Sheet {
        paper_size: String,
        positions: Vec<SheetPosition>,
    },
    Single {
        width: Dimension,
        height: Dimension,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenderLabelRequest {
    pub template: String,
    #[serde(flatten)]
    pub label: LabelInput,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenderBatchRequest {
    pub template: String,
    pub labels: Vec<LabelInput>,
    #[serde(default)]
    pub start_slot: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LabelInput {
    pub data: HashMap<String, Value>,
    #[serde(default)]
    pub option: Option<String>,
}
