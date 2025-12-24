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
    pub options: HashMap<String, Vec<String>>,
    pub format: TemplateFormat,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub format: TemplateFormat,
    pub options: HashMap<String, OptionDetail>,
    pub fields: Vec<FieldSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct OptionDetail {
    pub values: Vec<String>,
    pub default: String,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiline: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct LabelPosition(pub [f32; 4]);

impl LabelPosition {
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateFormat {
    Sheet {
        paper_size: String,
        positions: Vec<LabelPosition>,
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
