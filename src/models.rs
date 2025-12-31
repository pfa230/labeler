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
    Container {
        #[serde(rename = "box")]
        #[schema(rename = "box")]
        #[serde(skip_serializing_if = "Option::is_none")]
        bounds: Option<Box>,
        #[serde(skip_serializing_if = "Option::is_none")]
        option: Option<BTreeMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<u16>,
        #[schema(no_recursion)]
        items: Vec<LayoutItem>,
    },
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
    pub option: Option<BTreeMap<String, String>>,
}
