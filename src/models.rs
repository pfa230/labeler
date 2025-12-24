use serde::{Deserialize, Serialize};
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
    pub options: Vec<String>,
    pub format: TemplateFormat,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub format: TemplateFormat,
    pub options: Options,
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
        positions: Vec<Box>,
    },
    Single {
        width: Dimension,
        height: Dimension,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenderLabelRequest {
    pub template: String,
    pub data: HashMap<String, Value>,
    #[serde(default)]
    pub options: HashMap<String, Value>,
    #[serde(default)]
    pub output: OutputOptions,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenderBatchRequest {
    pub template: String,
    pub labels: Vec<BatchLabel>,
    #[serde(default)]
    pub start_slot: u32,
    #[serde(default)]
    pub output: OutputOptions,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchLabel {
    pub data: HashMap<String, Value>,
    #[serde(default)]
    pub options: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct OutputOptions {
    pub dpi: Option<u32>,
}
