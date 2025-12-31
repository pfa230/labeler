use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::models::{
    Box, Dimension, FontSize, Layout, LayoutItem, Options, TemplateDetail, TemplateFormat,
    TemplateSummary,
};

#[derive(Debug, Deserialize, Clone)]
pub struct TemplateDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    #[serde(default)]
    pub options: Option<Options>,
    pub layout: Layout,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug)]
pub struct TemplateRegistry {
    templates: HashMap<String, TemplateDefinition>,
}

impl TemplateRegistry {
    pub fn load_from_dir<P: AsRef<FsPath>>(dir: P) -> Result<Self, TemplateRegistryError> {
        let dir = dir.as_ref();
        let mut templates = HashMap::new();
        let mut seen_paths: HashMap<String, PathBuf> = HashMap::new();
        let entries = std::fs::read_dir(dir).map_err(|source| TemplateRegistryError::Io {
            path: dir.to_path_buf(),
            source,
        })?;

        for entry in entries {
            let entry = entry.map_err(|source| TemplateRegistryError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("yaml") | Some("yml")) {
                continue;
            }

            let contents =
                std::fs::read_to_string(&path).map_err(|source| TemplateRegistryError::Io {
                    path: path.clone(),
                    source,
                })?;
            let template: TemplateDefinition =
                serde_yml::from_str(&contents).map_err(|source| TemplateRegistryError::Yaml {
                    path: path.clone(),
                    source,
                })?;
            template
                .validate()
                .map_err(|message| TemplateRegistryError::Validation {
                    path: path.clone(),
                    message,
                })?;

            if let Some(existing_path) = seen_paths.get(&template.id) {
                return Err(TemplateRegistryError::DuplicateId {
                    id: template.id.clone(),
                    first: existing_path.clone(),
                    second: path,
                });
            }

            seen_paths.insert(template.id.clone(), path);
            templates.insert(template.id.clone(), template);
        }

        Ok(Self { templates })
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&TemplateDefinition> {
        self.templates.get(id)
    }

    pub fn summaries(&self) -> Vec<TemplateSummary> {
        let mut items: Vec<_> = self.templates.values().map(TemplateSummary::from).collect();
        items.sort_by(|a, b| a.id.cmp(&b.id));
        items
    }

    pub fn detail(&self, id: &str) -> Option<TemplateDetail> {
        self.templates.get(id).map(TemplateDetail::from)
    }
}

#[derive(Debug, Error)]
pub enum TemplateRegistryError {
    #[error("failed to read templates from {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse template {path}: {source}")]
    Yaml {
        path: PathBuf,
        source: serde_yml::Error,
    },
    #[error("template {path} failed validation: {message}")]
    Validation { path: PathBuf, message: String },
    #[error("duplicate template id '{id}' found in {first} and {second}")]
    DuplicateId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
}

impl TemplateDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("id must not be empty".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".to_string());
        }
        match self.unit.as_str() {
            "mm" | "in" => {}
            _ => return Err("unit must be either \"mm\" or \"in\"".to_string()),
        }
        if self.dpi == 0 {
            return Err("dpi must be greater than 0".to_string());
        }
        if let Some(options) = &self.options {
            if options.0.is_empty() {
                return Err("options must not be empty".to_string());
            }
            for (name, values) in &options.0 {
                if name.trim().is_empty() {
                    return Err("options must not contain empty names".to_string());
                }
                if values.is_empty() {
                    return Err(format!("options for '{name}' must not be empty"));
                }
                if values.iter().any(|opt| opt.trim().is_empty()) {
                    return Err("options must not contain empty values".to_string());
                }
            }
        }
        let bounds = layout_bounds(&self.format)?;
        validate_layout(&self.layout, self.options.as_ref(), bounds.as_ref())?;

        match &self.format {
            TemplateFormat::Sheet {
                paper_width,
                paper_height,
                label_width,
                label_height,
                positions,
            } => {
                if *paper_width <= 0.0 {
                    return Err("paper_width must be greater than 0".to_string());
                }
                if *paper_height <= 0.0 {
                    return Err("paper_height must be greater than 0".to_string());
                }
                if *label_width <= 0.0 {
                    return Err("label_width must be greater than 0".to_string());
                }
                if *label_height <= 0.0 {
                    return Err("label_height must be greater than 0".to_string());
                }
                if positions.is_empty() {
                    return Err("positions must not be empty".to_string());
                }
                for (idx, position) in positions.iter().enumerate() {
                    let point = position.point();
                    if point.x < 0.0 || point.y < 0.0 {
                        return Err(format!(
                            "position {} must have non-negative coordinates",
                            idx
                        ));
                    }
                }
            }
            TemplateFormat::Single { width, height } => {
                validate_dimension("width", width)?;
                validate_dimension("height", height)?;
            }
        }

        Ok(())
    }
}

fn validate_dimension(name: &str, dimension: &Dimension) -> Result<(), String> {
    match dimension {
        Dimension::Fixed(value) => {
            if *value <= 0.0 {
                return Err(format!("{name} must be greater than 0"));
            }
        }
        Dimension::Dynamic { min, max } => {
            if min.is_none() && max.is_none() {
                return Err(format!("{name} dynamic must specify min, max, or both"));
            }
            if let Some(min) = min {
                if *min <= 0.0 {
                    return Err(format!("min_{name} must be greater than 0"));
                }
            }
            if let Some(max) = max {
                if *max <= 0.0 {
                    return Err(format!("max_{name} must be greater than 0"));
                }
            }
            if let (Some(min), Some(max)) = (min, max) {
                if min > max {
                    return Err(format!("min_{name} must be <= max_{name}"));
                }
            }
        }
    }
    Ok(())
}

fn validate_layout(
    layout: &Layout,
    options: Option<&Options>,
    bounds: Option<&LayoutBounds>,
) -> Result<(), String> {
    match layout {
        Layout::Items(items) => validate_layout_items(items, bounds, options),
    }
}

fn validate_layout_items(
    items: &[LayoutItem],
    bounds: Option<&LayoutBounds>,
    options: Option<&Options>,
) -> Result<(), String> {
    let mut seen_names = HashSet::new();
    for item in items {
        if let Some(name) = layout_item_name(item) {
            if name.trim().is_empty() {
                return Err("layout item name must not be empty".to_string());
            }
            if !seen_names.insert(name.to_string()) {
                return Err(format!("duplicate layout item name '{}'", name));
            }
        }
        validate_layout_item(item, bounds, options)?;
    }
    Ok(())
}

fn layout_item_name(item: &LayoutItem) -> Option<&str> {
    match item {
        LayoutItem::Text { name, .. } => Some(name.as_str()),
        LayoutItem::Qr { name, .. } => Some(name.as_str()),
        LayoutItem::Line { .. } => None,
        LayoutItem::Rectangle { .. } => None,
        LayoutItem::Container { .. } => None,
    }
}

fn validate_layout_item(
    item: &LayoutItem,
    layout_bounds: Option<&LayoutBounds>,
    options: Option<&Options>,
) -> Result<(), String> {
    match item {
        LayoutItem::Text {
            bounds, font_size, ..
        } => {
            validate_box(bounds)?;
            validate_box_within(bounds, layout_bounds)?;
            validate_font_size(font_size)?;
        }
        LayoutItem::Qr { bounds, params, .. } => {
            validate_box(bounds)?;
            validate_box_within(bounds, layout_bounds)?;
            if let Some(params) = params {
                if let Some(module_size) = params.module_size {
                    if module_size <= 0.0 {
                        return Err("qr module_size must be greater than 0".to_string());
                    }
                }
                if let Some(quiet_zone) = params.quiet_zone {
                    if quiet_zone < 0.0 {
                        return Err("qr quiet_zone must be >= 0".to_string());
                    }
                }
            }
        }
        LayoutItem::Line {
            start,
            end,
            thickness,
        } => {
            if *thickness <= 0.0 {
                return Err("line thickness must be greater than 0".to_string());
            }
            if (start.x - end.x).abs() < f32::EPSILON && (start.y - end.y).abs() < f32::EPSILON {
                return Err("line start and end must differ".to_string());
            }
            if let Some(layout_bounds) = layout_bounds {
                validate_point_within(start, layout_bounds)?;
                validate_point_within(end, layout_bounds)?;
            }
        }
        LayoutItem::Rectangle {
            bounds, thickness, ..
        } => {
            validate_box(bounds)?;
            validate_box_within(bounds, layout_bounds)?;
            if *thickness <= 0.0 {
                return Err("rectangle thickness must be greater than 0".to_string());
            }
        }
        LayoutItem::Container {
            bounds,
            option,
            rotation,
            items,
        } => {
            let container_bounds = if let Some(bounds) = bounds {
                validate_box(bounds)?;
                validate_box_within(bounds, layout_bounds)?;
                let (bl, tr) = bounds.corners();
                let width = tr.x - bl.x;
                let height = tr.y - bl.y;
                Some(LayoutBounds { width, height })
            } else {
                layout_bounds.cloned()
            };

            if let Some(rotation) = rotation {
                if !matches!(*rotation, 0 | 90 | 180 | 270) {
                    return Err("container rotation must be 0, 90, 180, or 270".to_string());
                }
            }

            if let Some(option) = option {
                let Some(options) = options else {
                    return Err("container option requires template options".to_string());
                };
                if option.is_empty() {
                    return Err("container option must not be empty".to_string());
                }
                for (name, value) in option {
                    if name.trim().is_empty() || value.trim().is_empty() {
                        return Err("container option must not contain empty values".to_string());
                    }
                    let matches = options
                        .0
                        .get(name)
                        .map(|values| values.iter().any(|entry| entry == value))
                        .unwrap_or(false);
                    if !matches {
                        return Err(format!(
                            "container option '{name}' must match template options"
                        ));
                    }
                }
            }

            let container_bounds = container_bounds
                .map(|bounds| layout_bounds_from_size(bounds.width, bounds.height))
                .transpose()?;

            validate_layout_items(items, container_bounds.as_ref(), options)?;
        }
    }
    Ok(())
}

fn validate_box(bounds: &Box) -> Result<(), String> {
    let (bottom_left, top_right) = bounds.corners();
    if (bottom_left.x - top_right.x).abs() < f32::EPSILON
        || (bottom_left.y - top_right.y).abs() < f32::EPSILON
    {
        return Err("box must have non-zero width and height".to_string());
    }
    Ok(())
}

fn validate_box_within(bounds: &Box, layout_bounds: Option<&LayoutBounds>) -> Result<(), String> {
    let Some(layout_bounds) = layout_bounds else {
        return Ok(());
    };
    let (bottom_left, top_right) = bounds.corners();
    let min_x = bottom_left.x.min(top_right.x);
    let max_x = bottom_left.x.max(top_right.x);
    let min_y = bottom_left.y.min(top_right.y);
    let max_y = bottom_left.y.max(top_right.y);
    if min_x < 0.0 || min_y < 0.0 {
        return Err("box must not extend into negative coordinates".to_string());
    }
    if max_x > layout_bounds.width || max_y > layout_bounds.height {
        return Err("box must fit within layout bounds".to_string());
    }
    Ok(())
}

fn validate_point_within(
    point: &crate::models::Point,
    bounds: &LayoutBounds,
) -> Result<(), String> {
    if point.x < 0.0 || point.y < 0.0 {
        return Err("point must not extend into negative coordinates".to_string());
    }
    if point.x > bounds.width || point.y > bounds.height {
        return Err("point must fit within layout bounds".to_string());
    }
    Ok(())
}

fn validate_font_size(font_size: &FontSize) -> Result<(), String> {
    match font_size {
        FontSize::Fixed(value) => {
            if *value <= 0.0 {
                return Err("font_size must be greater than 0".to_string());
            }
        }
        FontSize::Range { min, max } => {
            if *min <= 0.0 || *max <= 0.0 {
                return Err("font_size min/max must be greater than 0".to_string());
            }
            if min > max {
                return Err("font_size min must be <= max".to_string());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct LayoutBounds {
    width: f32,
    height: f32,
}

fn layout_bounds(format: &TemplateFormat) -> Result<Option<LayoutBounds>, String> {
    let (width, height) = match format {
        TemplateFormat::Single { width, height } => {
            (resolve_dimension(width), resolve_dimension(height))
        }
        TemplateFormat::Sheet {
            label_width,
            label_height,
            ..
        } => (*label_width, *label_height),
    };

    layout_bounds_from_size(width, height).map(Some)
}

fn layout_bounds_from_size(width: f32, height: f32) -> Result<LayoutBounds, String> {
    Ok(LayoutBounds { width, height })
}
fn resolve_dimension(dimension: &Dimension) -> f32 {
    match dimension {
        Dimension::Fixed(value) => *value,
        Dimension::Dynamic { min, max } => max.or(*min).unwrap_or(0.0),
    }
}

impl From<&TemplateDefinition> for TemplateSummary {
    fn from(template: &TemplateDefinition) -> Self {
        Self {
            id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            unit: template.unit.clone(),
            dpi: template.dpi,
            options: template.options.clone(),
            format: template.format.clone(),
        }
    }
}

impl From<&TemplateDefinition> for TemplateDetail {
    fn from(template: &TemplateDefinition) -> Self {
        Self {
            id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            unit: template.unit.clone(),
            dpi: template.dpi,
            format: template.format.clone(),
            options: template.options.clone(),
            layout: template.layout.clone(),
            version: template.version.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TemplateDefinition, TemplateRegistry};
    use crate::models::{
        Alignment, Box, Dimension, FontSize, Layout, LayoutItem, Options, TemplateFormat,
    };
    use std::collections::BTreeMap;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("labeler_test_{label}_{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_template(dir: &Path, name: &str, contents: &str) {
        let path = dir.join(name);
        fs::write(&path, contents).expect("write template");
    }

    #[test]
    fn validate_rejects_empty_id() {
        let template = TemplateDefinition {
            id: " ".to_string(),
            name: "Label".to_string(),
            description: "desc".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(12.0),
                height: Dimension::Fixed(25.0),
            },
            options: Some(Options(BTreeMap::from([(
                "variant".to_string(),
                vec!["default".to_string()],
            )]))),
            layout: Layout::Items(Vec::new()),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("id must not be empty"));
    }

    #[test]
    fn validate_rejects_empty_option_value() {
        let template = TemplateDefinition {
            id: "test".to_string(),
            name: "Label".to_string(),
            description: "desc".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(12.0),
                height: Dimension::Fixed(25.0),
            },
            options: Some(Options(BTreeMap::from([(
                "variant".to_string(),
                vec!["".to_string()],
            )]))),
            layout: Layout::Items(Vec::new()),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("options must not contain empty values"));
    }

    #[test]
    fn load_from_dir_reads_templates() {
        let dir = temp_dir("load");
        write_template(
            &dir,
            "sample.yaml",
            r#"
id: sample
name: Sample
description: Sample template
unit: mm
dpi: 300
format:
  type: single
  width: 12.0
  height: 25.0
layout:
  - type: text
    name: message
    box: [0.0, 0.0, 10.0, 5.0]
    font_size: 10.0
    multiline: true
"#,
        );

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        assert_eq!(registry.len(), 1);
        assert!(registry.get("sample").is_some());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn summaries_are_sorted_by_id() {
        let dir = temp_dir("sorted");
        write_template(
            &dir,
            "b.yaml",
            r#"
id: b
name: B
description: B
unit: mm
dpi: 300
format:
  type: single
  width: 12.0
  height: 25.0
layout: []
"#,
        );
        write_template(
            &dir,
            "a.yaml",
            r#"
id: a
name: A
description: A
unit: mm
dpi: 300
format:
  type: single
  width: 12.0
  height: 25.0
layout: []
"#,
        );

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        let summaries = registry.summaries();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, "a");
        assert_eq!(summaries[1].id, "b");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_duplicate_field_names() {
        let template = TemplateDefinition {
            id: "dup".to_string(),
            name: "dup".to_string(),
            description: "dup".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(12.0),
                height: Dimension::Fixed(25.0),
            },
            options: Some(Options(BTreeMap::from([(
                "variant".to_string(),
                vec!["default".to_string()],
            )]))),
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: "value".to_string(),
                    bounds: Box([0.0, 0.0, 1.0, 1.0]),
                    font_size: FontSize::Fixed(10.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Text {
                    name: "value".to_string(),
                    bounds: Box([0.0, 0.0, 1.0, 1.0]),
                    font_size: FontSize::Fixed(10.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
            ]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("duplicate layout item name"));
    }
}
