use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::errors::TemplateError;
use crate::models::{
    Dimension, FontSize, Layout, LayoutItem, Options, Position, Size, SizeValue, TemplateDetail,
    TemplateFormat, TemplateSummary,
};
use crate::parse::parse_template;

#[derive(Debug, Clone)]
pub struct TemplateDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    pub options: Option<Options>,
    pub layout: Layout,
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
            let template =
                parse_template(&contents).map_err(|source| TemplateRegistryError::Parse {
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
    Parse {
        path: PathBuf,
        source: TemplateError,
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
            placement,
            font_size,
            ..
        } => {
            validate_position(&placement.at)?;
            validate_rotation(&placement.rotate)?;
            let (width, height) = resolve_size(
                &placement.size,
                placement.max_w,
                placement.max_h,
                layout_bounds,
                false,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
            validate_font_size(font_size)?;
        }
        LayoutItem::Qr {
            placement, params, ..
        } => {
            validate_position(&placement.at)?;
            validate_rotation(&placement.rotate)?;
            let (width, height) = resolve_size(
                &placement.size,
                placement.max_w,
                placement.max_h,
                layout_bounds,
                false,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
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
            placement,
            thickness,
        } => {
            validate_position(&placement.at)?;
            validate_rotation(&placement.rotate)?;
            if *thickness <= 0.0 {
                return Err("line thickness must be greater than 0".to_string());
            }
            let (dx, dy) = resolve_line_delta(
                &placement.size,
                placement.max_w,
                placement.max_h,
                layout_bounds,
            )?;
            if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
                return Err("line start and end must differ".to_string());
            }
            validate_line_bounds(&placement.at, dx, dy, layout_bounds)?;
        }
        LayoutItem::Rectangle {
            placement,
            thickness,
            ..
        } => {
            validate_position(&placement.at)?;
            validate_rotation(&placement.rotate)?;
            let (width, height) = resolve_size(
                &placement.size,
                placement.max_w,
                placement.max_h,
                layout_bounds,
                false,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
            if *thickness <= 0.0 {
                return Err("rectangle thickness must be greater than 0".to_string());
            }
        }
        LayoutItem::Container {
            placement,
            option,
            frame,
            padding,
            items,
        } => {
            validate_position(&placement.at)?;
            validate_rotation(&placement.rotate)?;
            let (width, height) = resolve_size(
                &placement.size,
                placement.max_w,
                placement.max_h,
                layout_bounds,
                true,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;

            if let Some(frame) = frame {
                if frame.thickness <= 0.0 {
                    return Err("container frame thickness must be greater than 0".to_string());
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

            let inner_width = width - padding.left - padding.right;
            let inner_height = height - padding.top - padding.bottom;
            let container_bounds = layout_bounds_from_size(inner_width, inner_height)?;
            validate_layout_items(items, Some(&container_bounds), options)?;
        }
    }
    Ok(())
}

fn validate_position(at: &Position) -> Result<(), String> {
    const BOUNDS_EPSILON: f32 = 1.0e-4;
    let point = at.point();
    if point.x < -BOUNDS_EPSILON || point.y < -BOUNDS_EPSILON {
        return Err("at must not extend into negative coordinates".to_string());
    }
    Ok(())
}

fn validate_rotation(rotate: &Option<f32>) -> Result<(), String> {
    if let Some(rotate) = rotate {
        if !rotate.is_finite() {
            return Err("rotate must be a finite number".to_string());
        }
    }
    Ok(())
}

fn resolve_size(
    size: &Size,
    max_w: Option<f32>,
    max_h: Option<f32>,
    layout_bounds: Option<&LayoutBounds>,
    allow_auto_fill: bool,
) -> Result<(f32, f32), String> {
    if let Some(max_w) = max_w {
        if max_w <= 0.0 {
            return Err("max_w must be greater than 0".to_string());
        }
    }
    if let Some(max_h) = max_h {
        if max_h <= 0.0 {
            return Err("max_h must be greater than 0".to_string());
        }
    }
    let fallback = if allow_auto_fill {
        layout_bounds.map(|bounds| (bounds.width, bounds.height))
    } else {
        None
    };
    let width = resolve_size_value(&size.0[0], max_w, fallback.map(|value| value.0), "width")?;
    let height = resolve_size_value(&size.0[1], max_h, fallback.map(|value| value.1), "height")?;
    Ok((width, height))
}

fn resolve_size_value(
    value: &SizeValue,
    max: Option<f32>,
    fallback: Option<f32>,
    label: &str,
) -> Result<f32, String> {
    match value {
        SizeValue::Value(value) => {
            if *value <= 0.0 {
                return Err(format!("size {label} must be greater than 0"));
            }
            Ok(*value)
        }
        SizeValue::Auto(_) => {
            let resolved = max
                .or(fallback)
                .ok_or_else(|| format!("size {label} is auto but no max_{label} provided"))?;
            if resolved <= 0.0 {
                return Err(format!("max_{label} must be greater than 0"));
            }
            Ok(resolved)
        }
    }
}

fn resolve_line_delta(
    size: &Size,
    max_w: Option<f32>,
    max_h: Option<f32>,
    layout_bounds: Option<&LayoutBounds>,
) -> Result<(f32, f32), String> {
    if let Some(max_w) = max_w {
        if max_w <= 0.0 {
            return Err("max_w must be greater than 0".to_string());
        }
    }
    if let Some(max_h) = max_h {
        if max_h <= 0.0 {
            return Err("max_h must be greater than 0".to_string());
        }
    }
    let fallback = layout_bounds.map(|bounds| (bounds.width, bounds.height));
    let dx = resolve_line_value(&size.0[0], max_w, fallback.map(|value| value.0), "width")?;
    let dy = resolve_line_value(&size.0[1], max_h, fallback.map(|value| value.1), "height")?;
    Ok((dx, dy))
}

fn resolve_line_value(
    value: &SizeValue,
    max: Option<f32>,
    fallback: Option<f32>,
    label: &str,
) -> Result<f32, String> {
    match value {
        SizeValue::Value(value) => Ok(*value),
        SizeValue::Auto(_) => {
            let resolved = max
                .or(fallback)
                .ok_or_else(|| format!("size {label} is auto but no max_{label} provided"))?;
            if resolved <= 0.0 {
                return Err(format!("max_{label} must be greater than 0"));
            }
            Ok(resolved)
        }
    }
}

fn validate_bounds(
    at: &Position,
    width: f32,
    height: f32,
    layout_bounds: Option<&LayoutBounds>,
) -> Result<(), String> {
    const BOUNDS_EPSILON: f32 = 1.0e-4;
    let Some(layout_bounds) = layout_bounds else {
        return Ok(());
    };
    let point = at.point();
    let max_x = point.x + width;
    let max_y = point.y + height;
    if max_x > layout_bounds.width + BOUNDS_EPSILON || max_y > layout_bounds.height + BOUNDS_EPSILON
    {
        return Err("item must fit within layout bounds".to_string());
    }
    Ok(())
}

fn validate_line_bounds(
    at: &Position,
    dx: f32,
    dy: f32,
    layout_bounds: Option<&LayoutBounds>,
) -> Result<(), String> {
    const BOUNDS_EPSILON: f32 = 1.0e-4;
    let Some(layout_bounds) = layout_bounds else {
        return Ok(());
    };
    let point = at.point();
    let end_x = point.x + dx;
    let end_y = point.y + dy;
    let min_x = point.x.min(end_x);
    let max_x = point.x.max(end_x);
    let min_y = point.y.min(end_y);
    let max_y = point.y.max(end_y);
    if min_x < -BOUNDS_EPSILON || min_y < -BOUNDS_EPSILON {
        return Err("line must not extend into negative coordinates".to_string());
    }
    if max_x > layout_bounds.width + BOUNDS_EPSILON || max_y > layout_bounds.height + BOUNDS_EPSILON
    {
        return Err("line must fit within layout bounds".to_string());
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
        Alignment, Dimension, FontSize, Layout, LayoutItem, Options, Position, Size, SizeValue,
        TemplateFormat,
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
    at: [0.0, 0.0]
    size: [10.0, 5.0]
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
                    placement: crate::models::Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(1.0), SizeValue::Value(1.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    font_size: FontSize::Fixed(10.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Text {
                    name: "value".to_string(),
                    placement: crate::models::Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(1.0), SizeValue::Value(1.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
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
