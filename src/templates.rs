use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::errors::TemplateError;
use crate::models::{
    resolve_coord, Dimension, Extent, FontSize, Layout, LayoutItem, Options, Point, Position,
    SizeValue, TemplateDetail, TemplateFormat, TemplateSummary,
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
    hashes: HashMap<String, String>,
    // The file each id was loaded from. A template's filename is only conventionally its id, so the
    // file-backed endpoints (source/PUT/DELETE) cannot reconstruct this from the id alone (#140).
    paths: HashMap<String, PathBuf>,
}

impl TemplateRegistry {
    pub fn load_from_dir<P: AsRef<FsPath>>(dir: P) -> Result<Self, TemplateRegistryError> {
        let dir = dir.as_ref();
        let mut templates = HashMap::new();
        let mut hashes = HashMap::new();
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
            hashes.insert(
                template.id.clone(),
                hex::encode(Sha256::digest(contents.as_bytes())),
            );
            templates.insert(template.id.clone(), template);
        }

        Ok(Self {
            templates,
            hashes,
            paths: seen_paths,
        })
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

    /// Lowercase hex SHA-256 of the template's raw YAML, used as a strong ETag.
    pub fn content_hash(&self, id: &str) -> Option<&str> {
        self.hashes.get(id).map(String::as_str)
    }

    /// The file this id was loaded from, or `None` if the registry does not hold the id.
    pub fn path(&self, id: &str) -> Option<&FsPath> {
        self.paths.get(id).map(PathBuf::as_path)
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
        // Require both bounds on a dynamic-width single before computing layout bounds,
        // so the caller gets the correct error rather than an out-of-bounds panic.
        if let TemplateFormat::Single {
            width: Dimension::Dynamic { min, max },
            ..
        } = &self.format
        {
            if min.is_none() || max.is_none() {
                return Err(
                    "a dynamic-width single template must specify both width.min and width.max"
                        .to_string(),
                );
            }
        }

        let bounds = layout_bounds(&self.format)?;
        let is_dynamic_width = matches!(
            &self.format,
            TemplateFormat::Single {
                width: Dimension::Dynamic { .. },
                ..
            }
        );
        validate_layout(
            &self.layout,
            self.options.as_ref(),
            bounds.as_ref(),
            is_dynamic_width,
        )?;

        if let TemplateFormat::Single {
            media_width: Some(mw),
            ..
        } = &self.format
        {
            if *mw <= 0.0 {
                return Err("media_width must be greater than 0".to_string());
            }
        }

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
            TemplateFormat::Single { width, height, .. } => {
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
    is_dynamic_width: bool,
) -> Result<(), String> {
    match layout {
        Layout::Items(items) => validate_layout_items(items, bounds, options, is_dynamic_width),
    }
}

fn validate_layout_items(
    items: &[LayoutItem],
    bounds: Option<&LayoutBounds>,
    options: Option<&Options>,
    is_dynamic_width: bool,
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
        validate_layout_item(item, bounds, options, is_dynamic_width)?;
    }
    Ok(())
}

fn layout_item_name(item: &LayoutItem) -> Option<&str> {
    match item {
        LayoutItem::Text { name, .. } => name.as_deref(),
        LayoutItem::Qr { name, .. } => name.as_deref(),
        LayoutItem::Image { name, .. } => name.as_deref(),
        LayoutItem::Line { .. } => None,
        LayoutItem::Container { .. } => None,
    }
}

fn validate_layout_item(
    item: &LayoutItem,
    layout_bounds: Option<&LayoutBounds>,
    options: Option<&Options>,
    is_dynamic_width: bool,
) -> Result<(), String> {
    match item {
        LayoutItem::Text {
            placement,
            font_size,
            font_weight,
            ..
        } => {
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_font_weight(*font_weight)?;
            validate_rotation(&placement.rotate, false)?;
            let auto_bounds = extent_auto_bounds(
                &placement.extent,
                layout_bounds,
                &placement.at,
                is_dynamic_width,
            );
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                auto_bounds.as_ref().or(layout_bounds),
                is_dynamic_width,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
            validate_font_size(font_size)?;
        }
        LayoutItem::Qr {
            placement, params, ..
        } => {
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_rotation(&placement.rotate, false)?;
            let auto_bounds = extent_auto_bounds(
                &placement.extent,
                layout_bounds,
                &placement.at,
                is_dynamic_width,
            );
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                auto_bounds.as_ref().or(layout_bounds),
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
        LayoutItem::Image { placement, .. } => {
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_rotation(&placement.rotate, false)?;
            let auto_bounds = extent_auto_bounds(
                &placement.extent,
                layout_bounds,
                &placement.at,
                is_dynamic_width,
            );
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                auto_bounds.as_ref().or(layout_bounds),
                false,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
        }
        LayoutItem::Line { at, to, thickness } => {
            const LINE_EPSILON: f32 = 1.0e-4;
            if *thickness <= 0.0 {
                return Err("line thickness must be greater than 0".to_string());
            }
            // Endpoints resolve against the frame before any comparison: `-0.0 == 0.0`, so raw
            // coordinates would reject a full-width divider as zero-length.
            let (start, end) = match layout_bounds {
                Some(bounds) => (
                    Point {
                        x: resolve_coord(at.x(), bounds.width),
                        y: resolve_coord(at.y(), bounds.height),
                    },
                    Point {
                        x: resolve_coord(to.x(), bounds.width),
                        y: resolve_coord(to.y(), bounds.height),
                    },
                ),
                None => (at.point(), to.point()),
            };
            // On a dynamic-width single an x resolved from an edge-relative component is
            // provisional: it was computed against `max`. Compare x only when both endpoints are
            // comparable, and let the render pass re-check the rest.
            let x_comparable =
                !is_dynamic_width || at.x().is_sign_negative() == to.x().is_sign_negative();
            let same_x = x_comparable && (start.x - end.x).abs() < LINE_EPSILON;
            let same_y = (start.y - end.y).abs() < LINE_EPSILON;
            if same_x && same_y {
                return Err("line start and end must differ".to_string());
            }
            if let Some(bounds) = layout_bounds {
                for point in [start, end] {
                    // A resolved x below zero means the inset exceeds `max`, which no smaller
                    // final width can rescue, so it is rejected here even on a dynamic label.
                    if point.x < -LINE_EPSILON || point.y < -LINE_EPSILON {
                        return Err("line must fit within layout bounds".to_string());
                    }
                    // The upper x bound binds only on a plain endpoint: an edge-relative one
                    // resolves to `bounds.width + x` with `x <= 0`, so it can never exceed the
                    // frame here. A plain endpoint past `max` is a constant no final width can
                    // rescue, so it is rejected at load even on a dynamic label.
                    if point.x > bounds.width + LINE_EPSILON
                        || point.y > bounds.height + LINE_EPSILON
                    {
                        return Err("line must fit within layout bounds".to_string());
                    }
                }
            }
        }
        LayoutItem::Container {
            placement,
            option,
            frame,
            padding,
            items,
        } => {
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_rotation(&placement.rotate, true)?;
            let rotation = placement
                .rotate
                .and_then(crate::models::Rotation::from_degrees)
                .unwrap_or(crate::models::Rotation::R0);
            if rotation.is_rotated() {
                // §4.2.1: the inner author canvas must be a compile-time constant. `auto` stays
                // banned outright (ADR-0036, unchanged); a `to` is only a problem when its width is
                // frame-dependent, which on a fixed frame it never is.
                let unresolvable = match &placement.extent {
                    Extent::Size(size) => size.0[0].is_auto() || size.0[1].is_auto(),
                    Extent::To(_) => is_dynamic_width && placement.width_is_frame_dependent(),
                };
                if unresolvable {
                    return Err(
                        "a rotated container must have an extent that resolves at compile time"
                            .to_string(),
                    );
                }
                if subtree_uses_auto(items) {
                    return Err("auto size is not allowed inside a rotated container".to_string());
                }
            }
            let auto_bounds = extent_auto_bounds(
                &placement.extent,
                layout_bounds,
                &placement.at,
                is_dynamic_width,
            );
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                auto_bounds.as_ref().or(layout_bounds),
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

            // Author canvas = full box, swapped for 90/270. Padding is author-space, so it is
            // subtracted from the (swapped) author dimensions. For R0 this is the existing math.
            let (canvas_w, canvas_h) = if rotation.swaps_axes() {
                (height, width)
            } else {
                (width, height)
            };
            let inner_width = canvas_w - padding.left - padding.right;
            let inner_height = canvas_h - padding.top - padding.bottom;
            const CONTENT_EPSILON: f32 = 1.0e-4;
            if rotation.is_rotated()
                && (inner_width <= CONTENT_EPSILON || inner_height <= CONTENT_EPSILON)
            {
                return Err("container padding leaves no room for content".to_string());
            }
            let container_bounds = layout_bounds_from_size(inner_width, inner_height)?;
            // Children of a frame-dependent container on a dynamic-width single may also use auto
            // width; they resolve to the container inner width at render time. A container sized
            // to the right edge via `to` is exactly as frame-dependent as an `auto` one. Rotated
            // containers reject a frame-dependent extent entirely (above), so child_dynamic is
            // false there.
            let child_dynamic =
                is_dynamic_width && !rotation.is_rotated() && placement.width_is_frame_dependent();
            validate_layout_items(items, Some(&container_bounds), options, child_dynamic)?;
        }
    }
    Ok(())
}

/// When `is_dynamic_width` is true, returns adjusted bounds where the width is reduced
/// by `at.x` so that an `auto` size resolves to the remaining width from the item's x offset.
/// Returns `None` when the adjustment is not needed (fixed-width or zero offset).
fn auto_resolve_bounds(
    layout_bounds: Option<&LayoutBounds>,
    at: &Position,
    is_dynamic_width: bool,
) -> Option<LayoutBounds> {
    if !is_dynamic_width {
        return None;
    }
    let bounds = layout_bounds?;
    let at_x = at.x();
    // An edge-relative x with a frame-dependent width is rejected in
    // `validate_placement_position`, so this never legitimately sees one. Returning `None` rather
    // than subtracting keeps a negative offset from widening the budget.
    if at_x <= 0.0 || at_x.is_sign_negative() {
        return None;
    }
    Some(LayoutBounds {
        width: (bounds.width - at_x).max(0.0),
        height: bounds.height,
    })
}

/// `auto_resolve_bounds`'s at.x-narrowed budget exists solely for `SizeValue::Auto`'s "fill to the
/// frame edge" fallback (`resolve_size_value`); a `to` box already subtracts `at.x` itself inside
/// `resolve_to_extent`, so applying the narrowed budget there too would double-subtract it.
fn extent_auto_bounds(
    extent: &Extent,
    layout_bounds: Option<&LayoutBounds>,
    at: &Position,
    is_dynamic_width: bool,
) -> Option<LayoutBounds> {
    match extent {
        Extent::Size(_) => auto_resolve_bounds(layout_bounds, at, is_dynamic_width),
        Extent::To(_) => None,
    }
}

/// Bounds-check a placement's anchor. A sign-negative component is edge-relative (#146) and
/// resolves against the frame first. On a dynamic-width single the final width is unknown but
/// bounded by `max`, so an inset larger than `max` is rejected here and a merely provisional
/// result is deferred to the render pass.
fn validate_placement_position(
    at: &Position,
    frame_dependent_width: bool,
    bounds: Option<&LayoutBounds>,
    is_dynamic_width: bool,
) -> Result<(), String> {
    const BOUNDS_EPSILON: f32 = 1.0e-4;
    if at.x().is_sign_negative() && frame_dependent_width && is_dynamic_width {
        return Err(
            "an edge-relative x cannot be combined with an auto or edge-relative width on a \
             dynamic-width template: both would depend on the label width"
                .to_string(),
        );
    }
    let Some(bounds) = bounds else {
        return Ok(());
    };
    if resolve_coord(at.x(), bounds.width) < -BOUNDS_EPSILON
        || resolve_coord(at.y(), bounds.height) < -BOUNDS_EPSILON
    {
        return Err("at resolves outside the frame".to_string());
    }
    Ok(())
}

/// True if any item in the subtree uses an `auto` width or height. Used to forbid auto sizing
/// anywhere inside a rotated container (#98), where author-horizontal maps to physical-vertical
/// and the dynamic-width measurement model does not apply.
fn subtree_uses_auto(items: &[LayoutItem]) -> bool {
    items.iter().any(|item| match item {
        LayoutItem::Text { placement, .. }
        | LayoutItem::Qr { placement, .. }
        | LayoutItem::Image { placement, .. } => placement
            .size_or_auto()
            .is_some_and(|s| s.0[0].is_auto() || s.0[1].is_auto()),
        LayoutItem::Container {
            placement, items, ..
        } => {
            placement
                .size_or_auto()
                .is_some_and(|s| s.0[0].is_auto() || s.0[1].is_auto())
                || subtree_uses_auto(items)
        }
        LayoutItem::Line { .. } => false,
    })
}

fn validate_rotation(rotate: &Option<f32>, is_container: bool) -> Result<(), String> {
    if let Some(deg) = rotate {
        // Rotation is a container-only inner transform (#98); reject it on any other item,
        // regardless of value (even 0).
        if !is_container {
            return Err("rotation is only supported on containers".to_string());
        }
        if crate::models::Rotation::from_degrees(*deg).is_none() {
            return Err("rotate must be a multiple of 90 degrees".to_string());
        }
    }
    Ok(())
}

/// `size = to - at`, both corners resolved against the frame first. Both components must be
/// strictly positive: `to` is the top-right corner of a box whose bottom-left is `at`.
fn resolve_to_extent(
    at: &Position,
    to: &Position,
    frame_w: f32,
    frame_h: f32,
) -> Result<(f32, f32), String> {
    let width = resolve_coord(to.x(), frame_w) - resolve_coord(at.x(), frame_w);
    let height = resolve_coord(to.y(), frame_h) - resolve_coord(at.y(), frame_h);
    if width <= 0.0 || height <= 0.0 {
        return Err("to must be above and to the right of at".to_string());
    }
    Ok((width, height))
}

fn resolve_size(
    at: &Position,
    extent: &Extent,
    max_w: Option<f32>,
    max_h: Option<f32>,
    layout_bounds: Option<&LayoutBounds>,
    allow_auto_fill: bool,
) -> Result<(f32, f32), String> {
    let size = match extent {
        Extent::Size(size) => size,
        Extent::To(to) => {
            // Every caller resolves against a frame (`layout_bounds` is always `Some`, and
            // container recursion passes its inner box), so this is an internal invariant, not an
            // authoring error. Erroring beats silently returning a zero extent if that changes.
            let Some(layout_bounds) = layout_bounds else {
                return Err("to cannot be resolved without a frame".to_string());
            };
            return resolve_to_extent(at, to, layout_bounds.width, layout_bounds.height);
        }
    };
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
            // `max_*` caps the resolution of `auto`; it does not replace the fallback. A cap
            // larger than the room available is simply not binding, so the smaller wins.
            let resolved = match (max, fallback) {
                (Some(max), Some(fallback)) => max.min(fallback),
                (Some(max), None) => max,
                (None, Some(fallback)) => fallback,
                (None, None) => {
                    return Err(format!("size {label} is auto but no max_{label} provided"))
                }
            };
            if resolved <= 0.0 {
                return Err(format!("max_{label} must be greater than 0"));
            }
            Ok(resolved)
        }
    }
}

/// The x half of this check is frame-independent even when the frame is not: for an edge-relative
/// `at.x` it reduces to `at.x + width <= 0` once `W` cancels out of `W + at.x + width <= W`, and
/// `validate_placement_position` guarantees such an item's width is a compile-time constant. So
/// there is nothing to defer to the render pass on either axis.
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
    let x = resolve_coord(at.x(), layout_bounds.width);
    let y = resolve_coord(at.y(), layout_bounds.height);
    if x + width > layout_bounds.width + BOUNDS_EPSILON
        || y + height > layout_bounds.height + BOUNDS_EPSILON
    {
        return Err("item must fit within layout bounds".to_string());
    }
    Ok(())
}

/// The wght axis accepts any value in range; the multiple-of-100 rule is a CSS-style convention that
/// keeps templates predictable. Enforced here as well as in `convert.rs` so a `LayoutItem` built by
/// any route — including the many built directly in tests — is checked.
fn validate_font_weight(font_weight: Option<u16>) -> Result<(), String> {
    match font_weight {
        Some(weight) if !(100..=900).contains(&weight) || weight % 100 != 0 => Err(format!(
            "font_weight must be a multiple of 100 between 100 and 900, got {weight}"
        )),
        _ => Ok(()),
    }
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
        TemplateFormat::Single { width, height, .. } => {
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

/// Test-only registry covering the whole `catalog/` tree plus `tests/fixtures/templates/`.
///
/// Nothing ships with the binary any more (#137): templates live in `catalog/` and users install
/// what they want. The suite still needs all of them — sheet format, options, container rotation, QR
/// layout and interpolation are only covered by catalog entries or the engine-demo fixtures moved out
/// of `catalog/` in #135. Flattens both trees into a single temp dir because `load_from_dir` takes one
/// path and does not recurse, and returns that dir so a test's `templates_dir` matches its registry —
/// the source/save/delete endpoints read YAML off disk, so a registry that disagreed with the dir
/// would 404 on them.
#[cfg(test)]
pub(crate) fn load_all_for_tests() -> (TemplateRegistry, std::path::PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "labeler-templates-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create merged template dir");
    // The catalog is nested (tape/brother, sheet/avery, examples) but the registry — and
    // {config}/templates, where installs land — is flat, so flatten while copying. Ids are unique
    // across the tree, enforced by `template_ids_are_unique_and_match_filenames` (#135).
    fn copy_yaml_into(src: &FsPath, dest: &FsPath) {
        for entry in std::fs::read_dir(src).unwrap_or_else(|e| panic!("read {src:?}: {e}")) {
            let path = entry.expect("dir entry").path();
            // symlink_metadata, not is_dir: a symlinked directory under catalog/ could form a cycle
            // and recurse forever.
            let meta = std::fs::symlink_metadata(&path).expect("stat catalog entry");
            if meta.is_dir() {
                copy_yaml_into(&path, dest);
            } else if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
                let name = path.file_name().expect("file name");
                let target = dest.join(name);
                // Flattening means two catalog files with the same filename in different directories
                // would silently overwrite each other here, before load_from_dir could ever notice a
                // duplicate id. Fail loudly instead; the CI gate then explains which files collide.
                assert!(
                    !target.exists(),
                    "two catalog templates share the filename {name:?}; ids must be unique tree-wide"
                );
                std::fs::copy(&path, target).expect("copy template");
            }
        }
    }
    copy_yaml_into(FsPath::new("catalog"), &dir);
    // The engine demos (QR, multiline, sheet options, rotation, interpolation) are test corpus, not
    // catalog entries (#135). They flatten into the same dir: ids are unique across both roots,
    // enforced by `template_ids_are_unique_and_match_filenames`. The files must be copied, not just
    // registered — this dir becomes `templates_dir`, and GET /templates/{id}/source reads
    // {templates_dir}/{id}.yaml off disk.
    copy_yaml_into(FsPath::new("tests/fixtures/templates"), &dir);
    let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
    (registry, dir)
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

    fn parse_and_validate(yaml: &str) -> Result<(), String> {
        crate::parse::parse_template(yaml)
            .map_err(|e| e.to_string())?
            .validate()
    }

    #[test]
    fn rotation_must_be_orthogonal() {
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 45\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotation_rejected_on_non_container() {
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [40,10]\n    rotate: 90\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotation_zero_rejected_on_non_container() {
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [40,10]\n    rotate: 0\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotated_container_rejects_auto_outer_size() {
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [auto,40]\n    rotate: 90\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotated_container_rejects_auto_child() {
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    items:\n      - type: text\n        value: hi\n        at: [0,0]\n        size: [auto,10]\n        font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotated_container_child_bounds_use_swapped_canvas() {
        // physical 80x40 container, rotate 90 -> author canvas 40x80; a child 30 wide x 70 tall fits.
        let ok = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    items:\n      - type: text\n        value: hi\n        at: [0,0]\n        size: [30,70]\n        font_size: 6\n";
        assert!(parse_and_validate(ok).is_ok());
        // a child 50 wide exceeds the 40-wide author canvas -> error.
        let bad = ok.replace("size: [30,70]", "size: [50,70]");
        assert!(parse_and_validate(&bad).is_err());
    }

    #[test]
    fn validate_accepts_a_to_box_spanning_to_the_right_edge() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 0.0]\n    to: [-0.0, 12.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// `to` must be above and to the right of `at`.
    #[test]
    fn validate_rejects_an_inverted_to_box() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 40\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [20.0, 0.0]\n    to: [10.0, 12.0]\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// §4.2.1: a rotated container's inner canvas has to be known at compile time.
    #[test]
    fn validate_rejects_a_rotated_container_with_a_frame_dependent_to() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    to: [-0.0, 12.0]\n    rotate: 90\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// Both corners edge-relative is a constant 20-unit box, so the canvas is known and it is fine.
    #[test]
    fn validate_accepts_a_rotated_container_whose_corners_both_hug_the_edge() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 25, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [-20.0, 0.0]\n    to: [-0.0, 12.0]\n    rotate: 90\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// A container sized to the right edge is frame-dependent, so its children are dynamic too and an
    /// auto-width child resolves against the container's inner width rather than being rejected.
    #[test]
    fn validate_accepts_an_auto_child_inside_a_to_spanned_container() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 20, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [4.0, 0.0]\n    to: [-0.0, 12.0]\n    items:\n      - type: text\n        value: \"x\"\n        at: [2.0, 1.0]\n        size: [auto, 10.0]\n        font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn rotated_container_rejects_nonpositive_content_area() {
        // author canvas is 40 wide x 80 tall; top+bottom padding 120 > 80 -> non-positive Ch.
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    padding: [60,0,60,0]\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotation_orthogonal_on_container_ok() {
        let yaml = "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: -90\n    items: []\n";
        assert!(parse_and_validate(yaml).is_ok());
    }

    /// A right-anchored box with a known width is legal on an auto-length label: its position is
    /// deferred, but its size never was.
    #[test]
    fn validate_accepts_a_right_anchored_fixed_width_box() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [20.0, 10.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Position and width would both be chasing the same unknown.
    #[test]
    fn validate_rejects_a_right_anchored_auto_width_box_on_a_dynamic_label() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [auto, 10.0]\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// On a fixed frame everything resolves, so the same shape is fine.
    #[test]
    fn validate_accepts_a_right_anchored_auto_width_box_on_a_fixed_label() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [auto, 10.0]\n    max_w: 20.0\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Negative y is the top edge, so this box sits flush against it.
    #[test]
    fn validate_accepts_a_top_anchored_box() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, -4.0]\n    size: [20.0, 4.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// `x + width <= W` reduces to `at.x + width <= 0` for an edge-relative `at.x`: the frame width
    /// cancels, so a right-anchored box that overruns the right edge is decidable at load even on a
    /// dynamic-width label, and every render of it would fail.
    #[test]
    fn validate_rejects_a_right_anchored_box_that_overruns_the_right_edge() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 60 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-0.0, 2.0]\n    size: [10.0, 6.0]\n    font_size: 6\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("item must fit within layout bounds".to_string())
        );
    }

    /// A plain endpoint past `width.max` can never render at any final width, so it is rejected at
    /// load rather than deferred to a render that is guaranteed to fail.
    #[test]
    fn validate_rejects_a_plain_line_endpoint_past_the_max_width() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 30 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [40.0, 6.0]\n    thickness: 0.2\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("line must fit within layout bounds".to_string())
        );
    }

    /// The guard exists to protect the same-sign x_comparable branch: two edge-relative endpoints
    /// at different insets on a dynamic-width label are a real divider and must validate, not be
    /// rejected as if the label's final width were unknown to both.
    #[test]
    fn validate_accepts_an_edge_relative_line_on_a_dynamic_width_label() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [-30.0, 6.0]\n    to: [-0.0, 6.0]\n    thickness: 0.2\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// `max_*` is a cap on the resolution of `auto`, not a substitute for the fallback. Discarding the
    /// fallback is what made validation reject a container the renderer would have fitted (#152).
    #[test]
    fn resolve_size_value_caps_rather_than_substituting() {
        use super::resolve_size_value;
        let auto = SizeValue::Auto(crate::models::AutoSize::Auto);
        // Both present: the smaller wins, in both orders.
        assert_eq!(
            resolve_size_value(&auto, Some(30.0), Some(10.0), "width"),
            Ok(10.0)
        );
        assert_eq!(
            resolve_size_value(&auto, Some(10.0), Some(30.0), "width"),
            Ok(10.0)
        );
        // One present: it is used.
        assert_eq!(
            resolve_size_value(&auto, Some(30.0), None, "width"),
            Ok(30.0)
        );
        assert_eq!(
            resolve_size_value(&auto, None, Some(30.0), "width"),
            Ok(30.0)
        );
        // Neither: the unchanged error.
        assert!(resolve_size_value(&auto, None, None, "width").is_err());
        // A numeric size is never clamped by the bound.
        assert_eq!(
            resolve_size_value(&SizeValue::Value(50.0), Some(30.0), Some(30.0), "width"),
            Ok(50.0)
        );
    }

    /// The #152 disagreement, from the validation side: a container whose cap exceeds the room left
    /// must resolve to the room left and fit, not to the cap and overflow.
    #[test]
    fn validate_accepts_a_capped_container_that_fits_the_remaining_width() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [90.0, 0.0]\n    size: [auto, 12.0]\n    max_w: 30.0\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

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
                media_width: None,
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
                media_width: None,
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
    fn validate_font_weight_accepts_only_hundreds_in_range() {
        for bad in [0u16, 50, 350, 1000] {
            let err = super::validate_font_weight(Some(bad)).expect_err("must be rejected");
            assert!(err.contains("font_weight"), "unexpected message: {err}");
        }
        for good in [100u16, 400, 900] {
            super::validate_font_weight(Some(good)).expect("must be accepted");
        }
        super::validate_font_weight(None).expect("absent is valid");
    }

    /// The unit test above passes even if nothing ever calls the validator. This one fails unless
    /// `validate` actually reaches it, which is the point of centralising the rule here rather than
    /// only in `convert.rs`: a `LayoutItem` built directly — as most of this suite does — is checked.
    #[test]
    fn validate_rejects_a_text_item_with_a_bad_font_weight() {
        let template = TemplateDefinition {
            id: "w".to_string(),
            name: "w".to_string(),
            description: "w".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: Some("value".to_string()),
                value: None,
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: Some(350),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        let err = template.validate().expect_err("350 must not validate");
        assert!(err.contains("font_weight"), "unexpected message: {err}");
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
                media_width: None,
            },
            options: Some(Options(BTreeMap::from([(
                "variant".to_string(),
                vec!["default".to_string()],
            )]))),
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: Some("value".to_string()),
                    value: None,
                    placement: crate::models::Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(1.0), SizeValue::Value(1.0)]),
                    ),
                    font_size: FontSize::Fixed(10.0),
                    font_weight: None,
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Text {
                    name: Some("value".to_string()),
                    value: None,
                    placement: crate::models::Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(1.0), SizeValue::Value(1.0)]),
                    ),
                    font_size: FontSize::Fixed(10.0),
                    font_weight: None,
                    multiline: false,
                    alignment: Alignment::default(),
                },
            ]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("duplicate layout item name"));
    }

    #[test]
    fn validate_rejects_duplicate_name_across_item_types() {
        let template = TemplateDefinition {
            id: "dup2".to_string(),
            name: "dup2".to_string(),
            description: "dup2".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: Some("value".to_string()),
                    value: None,
                    placement: crate::models::Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                    ),
                    font_size: FontSize::Fixed(10.0),
                    font_weight: None,
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Image {
                    name: Some("value".to_string()),
                    src: None,
                    placement: crate::models::Placement::sized(
                        Position([0.0, 5.0]),
                        Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                    ),
                    fit: crate::models::Fit::Contain,
                },
            ]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("duplicate layout item name"));
    }

    #[test]
    fn validate_rejects_degenerate_line() {
        let template = TemplateDefinition {
            id: "ln".to_string(),
            name: "ln".to_string(),
            description: "ln".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Line {
                at: Position([1.0, 1.0]),
                to: Position([1.0, 1.0]),
                thickness: 0.2,
            }]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("line start and end must differ"));
    }

    fn single_line_template(at: Position, to: Position) -> TemplateDefinition {
        TemplateDefinition {
            id: "ln".to_string(),
            name: "ln".to_string(),
            description: "ln".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Line {
                at,
                to,
                thickness: 0.2,
            }]),
            version: None,
        }
    }

    #[test]
    fn validate_rejects_out_of_bounds_line() {
        let template = single_line_template(Position([0.0, 0.0]), Position([100.0, 0.0]));
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("line must fit within layout bounds"));
    }

    #[test]
    fn validate_rejects_a_line_endpoint_inset_beyond_the_frame() {
        let template = single_line_template(Position([0.0, 1.0]), Position([-40.0, 1.0]));
        assert!(template.validate().is_err());
    }

    /// #146's headline case. `-0.0 == 0.0`, so comparing raw endpoints rejects a full-width divider
    /// as a zero-length line. The check has to run on resolved coordinates.
    #[test]
    fn validate_accepts_a_full_width_divider_on_a_dynamic_label() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [-0.0, 6.0]\n    thickness: 0.2\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Still degenerate after resolution: both endpoints land on the right edge.
    #[test]
    fn validate_rejects_a_line_degenerate_after_resolution() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 40\n  height: 12\nlayout:\n  - type: line\n    at: [-0.0, 6.0]\n    to: [-0.0, 6.0]\n    thickness: 0.2\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// An inset larger than the widest the label can ever be never resolves to a valid coordinate.
    #[test]
    fn validate_rejects_a_line_inset_larger_than_the_max_width() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [-140.0, 6.0]\n    thickness: 0.2\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn dynamic_width_single_requires_both_bounds() {
        // Only min is set; max is None. Validate should reject this.
        let template = TemplateDefinition {
            id: "tape".to_string(),
            name: "Tape".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: None,
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("hello".to_string()),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(8.0), SizeValue::Value(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(
            err.contains("must specify both width.min and width.max"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn dynamic_width_single_auto_width_item_at_offset_validates_ok() {
        // Dynamic-width single with both bounds; a container at at.x=5 with auto width.
        // Auto width should resolve to max_width - at.x = 100 - 5 = 95, which fits.
        let template = TemplateDefinition {
            id: "tape2".to_string(),
            name: "Tape2".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: crate::models::Placement::sized(
                    Position([5.0, 0.0]),
                    Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(12.0),
                    ]),
                ),
                option: None,
                frame: None,
                padding: crate::models::Padding::ZERO,
                items: vec![],
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with auto-width container at offset should validate OK");
    }

    #[test]
    fn dynamic_width_single_allows_multiline_text() {
        let template = TemplateDefinition {
            id: "tape_multiline".to_string(),
            name: "Tape Multiline".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("hello".to_string()),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(8.0), SizeValue::Value(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: true,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with multiline: true should validate OK");
    }

    #[test]
    fn dynamic_width_single_allows_single_line_text() {
        let template = TemplateDefinition {
            id: "tape_single_line".to_string(),
            name: "Tape Single Line".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("hello".to_string()),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(8.0), SizeValue::Value(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with multiline: false should validate OK");
    }

    #[test]
    fn fixed_width_single_allows_multiline_text() {
        let template = TemplateDefinition {
            id: "fixed_multiline".to_string(),
            name: "Fixed Multiline".to_string(),
            description: "fixed".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(50.0),
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("hello".to_string()),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(40.0), SizeValue::Value(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: true,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        template
            .validate()
            .expect("fixed-width single with multiline: true should validate OK");
    }

    #[test]
    fn single_rejects_nonpositive_media_width() {
        let build = |mw: Option<f32>| TemplateDefinition {
            id: "mw_test".to_string(),
            name: "MW Test".to_string(),
            description: "test".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(50.0),
                height: Dimension::Fixed(12.0),
                media_width: mw,
            },
            options: None,
            layout: Layout::Items(vec![]),
            version: None,
        };
        for bad in [Some(0.0), Some(-1.0)] {
            let err = build(bad).validate().expect_err("expected error");
            assert!(
                err.contains("media_width must be greater than 0"),
                "unexpected error: {err}"
            );
        }
        build(Some(12.0))
            .validate()
            .expect("positive media_width should validate");
    }

    #[test]
    fn registry_exposes_per_template_content_hash() {
        let dir = std::env::temp_dir().join(format!("tmpl_hash_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        write_template(
            &dir,
            "a.yaml",
            "id: a\nname: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 10\n  height: 10\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [10,5]\n    font_size: 6\n",
        );
        let reg = TemplateRegistry::load_from_dir(&dir).expect("load");
        let hash = reg.content_hash("a").expect("hash present");
        assert_eq!(hash.len(), 64, "sha-256 hex is 64 chars");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(reg.content_hash("missing").is_none());
        fs::remove_dir_all(&dir).ok();
    }
}
