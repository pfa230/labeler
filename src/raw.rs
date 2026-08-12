use serde::Deserialize;
use std::collections::BTreeMap;

use crate::models::{
    Alignment, Fit, FontSize, Frame, Options, Position, QrParams, Size, TemplateFormat,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDefinitionRaw {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    #[serde(default)]
    pub options: Option<Options>,
    pub layout: Vec<LayoutItemRaw>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayoutItemRaw {
    Text(TextRaw),
    Qr(QrRaw),
    Image(ImageRaw),
    Line(LineRaw),
    Container(ContainerRaw),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRaw {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(flatten)]
    pub placement: PlacementRaw,
    pub font_size: FontSize,
    #[serde(default)]
    pub font_weight: Option<u16>,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub alignment: Alignment,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrRaw {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(flatten)]
    pub placement: PlacementRaw,
    #[serde(default)]
    pub params: Option<QrParams>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRaw {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub src: Option<String>,
    #[serde(flatten)]
    pub placement: PlacementRaw,
    #[serde(default)]
    pub fit: Fit,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineRaw {
    #[serde(default)]
    pub at: Position,
    pub to: Position,
    pub thickness: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerRaw {
    #[serde(flatten)]
    pub placement: PlacementRaw,
    #[serde(default)]
    pub option: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub frame: Option<Frame>,
    #[serde(default)]
    pub padding: Option<PaddingRaw>,
    pub items: Vec<LayoutItemRaw>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PaddingRaw {
    Uniform(f32),
    Trbl([f32; 4]),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRaw {
    #[serde(default)]
    pub at: Option<Position>,
    #[serde(default)]
    pub size: Option<Size>,
    #[serde(default)]
    pub to: Option<Position>,
    #[serde(default)]
    pub max_w: Option<f32>,
    #[serde(default)]
    pub max_h: Option<f32>,
    #[serde(default)]
    pub rotate: Option<f32>,
}
