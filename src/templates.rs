use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::errors::TemplateError;
use crate::models::{
    resolve_coord, DynamicDimension, DynamicValue, Extent, FontSize, Layout, LayoutItem, Options,
    ParamSpec, ParamType, Placement, Point, Size, SizeValue, TemplateDetail, TemplateFormat,
    TemplateSummary,
};
use crate::parse::parse_template;
use crate::resolver;

#[derive(Debug, Clone)]
pub struct TemplateContent {
    pub name: String,
    pub description: String,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    pub params: std::collections::BTreeMap<String, ParamSpec>,
    pub layout: Layout,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TemplateDefinition {
    pub id: String,
    pub group: Option<String>,
    pub content: TemplateContent,
}

impl std::ops::Deref for TemplateDefinition {
    type Target = TemplateContent;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

impl std::ops::DerefMut for TemplateDefinition {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.content
    }
}

impl TemplateContent {
    pub fn options(&self) -> Option<Options> {
        let mut map = std::collections::BTreeMap::new();
        for (name, spec) in &self.params {
            if let ParamType::Enum { values } = &spec.param_type {
                map.insert(name.clone(), values.clone());
            }
        }
        if map.is_empty() {
            None
        } else {
            Some(Options(map))
        }
    }
}

/// A template file that could not be parsed, failed validation, or lost an id collision.
#[derive(Debug, Clone)]
pub struct BrokenTemplate {
    /// Path of the file relative to the templates directory (e.g. `foo.yaml` or `Shipping/pallet.yaml`).
    pub path: String,
    /// Human-readable description of what went wrong.
    pub error: String,
}

#[derive(Debug)]
pub struct TemplateRegistry {
    templates: HashMap<String, TemplateDefinition>,
    hashes: HashMap<String, String>,
    paths: HashMap<String, PathBuf>,
    rel_paths: HashMap<String, PathBuf>,
    /// Files refused by a parse, validation or duplicate-id fault; excluded from the valid set
    /// but not fatal.
    broken: Vec<BrokenTemplate>,
    // Files refused *specifically* for declaring an id another file already holds, keyed by that id.
    // `broken` carries the same event as prose written for an operator; the write endpoints need it
    // as data, to answer "is this id contested, and by which file?" without parsing a message
    // (#183, #184).
    duplicates: HashMap<String, Vec<PathBuf>>,
}

pub fn validate_template_id_stem(stem: &str) -> bool {
    !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

struct DiscoveredFile {
    rel_path_bytes: Vec<u8>,
    rel_path: PathBuf,
    abs_path: PathBuf,
    is_utf8: bool,
    dir_error: Option<String>,
}

fn collect_dir_entries(
    root: &FsPath,
    current_rel: &FsPath,
    dir_error: Option<&str>,
    out: &mut Vec<DiscoveredFile>,
) -> Result<(), TemplateRegistryError> {
    let current_abs = root.join(current_rel);
    let entries = match std::fs::read_dir(&current_abs) {
        Ok(e) => e,
        Err(source) => {
            return Err(TemplateRegistryError::Io {
                path: current_abs,
                source,
            })
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(source) => {
                return Err(TemplateRegistryError::Io {
                    path: current_abs,
                    source,
                })
            }
        };

        let file_name = entry.file_name();
        let name_bytes = file_name.as_encoded_bytes();
        let abs_path = entry.path();

        let meta = match std::fs::symlink_metadata(&abs_path) {
            Ok(m) => m,
            Err(source) => {
                return Err(TemplateRegistryError::Io {
                    path: abs_path,
                    source,
                })
            }
        };

        let rel_path = if current_rel.as_os_str().is_empty() {
            PathBuf::from(&file_name)
        } else {
            current_rel.join(&file_name)
        };

        if meta.is_dir() {
            // Dot-directory skip outranks invalid-directory reporting at any depth
            if name_bytes.starts_with(b".") {
                continue;
            }

            let next_dir_error: Option<String> = if let Some(err) = dir_error {
                Some(err.to_string())
            } else if let Some(name_str) = file_name.to_str() {
                match validate_group_segment(name_str) {
                    Ok(()) => None,
                    Err(err) => Some(format!(
                        "directory '{}' is invalid: {err}",
                        rel_path.display()
                    )),
                }
            } else {
                Some(format!(
                    "directory '{}' name is not valid UTF-8",
                    rel_path.to_string_lossy()
                ))
            };

            collect_dir_entries(root, &rel_path, next_dir_error.as_deref(), out)?;
        } else {
            let is_utf8 = rel_path.to_str().is_some();
            let is_yaml = if let Some(ext) = rel_path.extension().and_then(|e| e.to_str()) {
                ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml")
            } else {
                false
            };

            let should_include = is_yaml
                || (!is_utf8
                    && (name_bytes.ends_with(b".yaml")
                        || name_bytes.ends_with(b".YAML")
                        || name_bytes.ends_with(b".yml")
                        || name_bytes.ends_with(b".YML")));

            if should_include {
                let rel_path_bytes = rel_path.as_os_str().as_encoded_bytes().to_vec();
                out.push(DiscoveredFile {
                    rel_path_bytes,
                    rel_path,
                    abs_path,
                    is_utf8,
                    dir_error: dir_error.map(str::to_string),
                });
            }
        }
    }
    Ok(())
}

fn collect_group_paths(
    root: &FsPath,
    current_rel: &FsPath,
    out: &mut Vec<String>,
) -> Result<(), TemplateRegistryError> {
    let current_abs = root.join(current_rel);
    let entries = match std::fs::read_dir(&current_abs) {
        Ok(e) => e,
        Err(source) => {
            return Err(TemplateRegistryError::Io {
                path: current_abs,
                source,
            })
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(source) => {
                return Err(TemplateRegistryError::Io {
                    path: current_abs,
                    source,
                })
            }
        };

        let file_name = entry.file_name();
        let name_bytes = file_name.as_encoded_bytes();
        let abs_path = entry.path();

        let meta = match std::fs::symlink_metadata(&abs_path) {
            Ok(m) => m,
            Err(source) => {
                return Err(TemplateRegistryError::Io {
                    path: abs_path,
                    source,
                })
            }
        };

        if meta.is_dir() {
            if name_bytes.starts_with(b".") {
                continue;
            }

            let Some(name_str) = file_name.to_str() else {
                continue;
            };

            if validate_group_segment(name_str).is_err() {
                continue;
            }

            let rel_path = if current_rel.as_os_str().is_empty() {
                PathBuf::from(name_str)
            } else {
                current_rel.join(name_str)
            };

            let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");
            if validate_group_name(&rel_path_str).is_ok() {
                out.push(rel_path_str);
                collect_group_paths(root, &rel_path, out)?;
            }
        }
    }
    Ok(())
}

pub fn list_template_groups<P: AsRef<FsPath>>(
    dir: P,
) -> Result<Vec<String>, TemplateRegistryError> {
    let mut groups = Vec::new();
    collect_group_paths(dir.as_ref(), FsPath::new(""), &mut groups)?;
    groups.sort();
    Ok(groups)
}

impl TemplateRegistry {
    pub fn load_from_dir<P: AsRef<FsPath>>(dir: P) -> Result<Self, TemplateRegistryError> {
        let dir = dir.as_ref();
        let mut files = Vec::new();
        collect_dir_entries(dir, FsPath::new(""), None, &mut files)?;
        files.sort_by(|a, b| a.rel_path_bytes.cmp(&b.rel_path_bytes));

        let mut templates = HashMap::new();
        let mut hashes = HashMap::new();
        let mut seen_paths: HashMap<String, PathBuf> = HashMap::new();
        let mut seen_rel_paths: HashMap<String, PathBuf> = HashMap::new();
        let mut broken: Vec<BrokenTemplate> = Vec::new();
        let mut duplicates: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for file in files {
            if !file.is_utf8 {
                let lossy_path = file.rel_path.to_string_lossy().into_owned();
                let error = format!("path '{lossy_path}' is not valid UTF-8");
                tracing::warn!(%error, "skipping broken template");
                broken.push(BrokenTemplate {
                    path: lossy_path,
                    error,
                });
                continue;
            }

            let rel_path_str = file.rel_path.to_str().unwrap().replace('\\', "/");

            if let Some(dir_err) = file.dir_error {
                tracing::warn!(error = %dir_err, "skipping broken template");
                broken.push(BrokenTemplate {
                    path: rel_path_str,
                    error: dir_err,
                });
                continue;
            }

            let stem = file
                .rel_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if !validate_template_id_stem(stem) {
                let error = format!(
                    "template filename stem '{stem}' is not a valid id: must match ^[a-zA-Z0-9_-]+$"
                );
                tracing::warn!(%error, "skipping broken template");
                broken.push(BrokenTemplate {
                    path: rel_path_str,
                    error,
                });
                continue;
            }

            let group = file.rel_path.parent().and_then(|p| {
                let s = p.to_string_lossy().replace('\\', "/");
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            });

            let contents = match std::fs::read_to_string(&file.abs_path) {
                Ok(c) => c,
                Err(source) => {
                    let error = TemplateRegistryError::Io {
                        path: file.rel_path.clone(),
                        source,
                    }
                    .to_string();
                    tracing::warn!(%error, "skipping broken template");
                    broken.push(BrokenTemplate {
                        path: rel_path_str,
                        error,
                    });
                    continue;
                }
            };

            let content = match parse_template(&contents) {
                Ok(c) => c,
                Err(source) => {
                    let error = TemplateRegistryError::Parse {
                        path: file.rel_path.clone(),
                        source,
                    }
                    .to_string();
                    tracing::warn!(%error, "skipping broken template");
                    broken.push(BrokenTemplate {
                        path: rel_path_str,
                        error,
                    });
                    continue;
                }
            };

            if let Err(message) = content.validate() {
                let error = TemplateRegistryError::Validation {
                    path: file.rel_path.clone(),
                    message,
                }
                .to_string();
                tracing::warn!(%error, "skipping broken template");
                broken.push(BrokenTemplate {
                    path: rel_path_str,
                    error,
                });
                continue;
            }

            let id = stem.to_string();
            if let Some(first_rel) = seen_rel_paths.get(&id) {
                let error = TemplateRegistryError::DuplicateId {
                    id: id.clone(),
                    first: first_rel.clone(),
                    second: file.rel_path.clone(),
                }
                .to_string();
                tracing::warn!(%error, "skipping broken template");
                broken.push(BrokenTemplate {
                    path: rel_path_str,
                    error,
                });
                duplicates.entry(id).or_default().push(file.rel_path);
                continue;
            }

            seen_paths.insert(id.clone(), file.abs_path);
            seen_rel_paths.insert(id.clone(), file.rel_path);
            hashes.insert(id.clone(), hex::encode(Sha256::digest(contents.as_bytes())));
            templates.insert(id.clone(), TemplateDefinition { id, group, content });
        }

        Ok(Self {
            templates,
            hashes,
            paths: seen_paths,
            rel_paths: seen_rel_paths,
            broken,
            duplicates,
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

    /// Files refused for declaring `id` while another file already held it, in load order.
    ///
    /// Empty for an uncontested id. Only files that parsed and validated can appear: one that fails
    /// either never reaches the id check, so it never claims an id (see the create guard in
    /// `api.rs`, which covers that case by filename instead).
    pub fn duplicates(&self, id: &str) -> &[PathBuf] {
        self.duplicates.get(id).map_or(&[], Vec::as_slice)
    }

    /// The file this id was loaded from, or `None` if the registry does not hold the id.
    pub fn path(&self, id: &str) -> Option<&FsPath> {
        self.paths.get(id).map(PathBuf::as_path)
    }

    /// The relative file path this id was loaded from, or `None` if the registry does not hold the id.
    pub fn rel_path(&self, id: &str) -> Option<&FsPath> {
        self.rel_paths.get(id).map(PathBuf::as_path)
    }

    /// Files refused during this load, by a parse, validation or duplicate-id fault.
    pub fn broken(&self) -> &[BrokenTemplate] {
        &self.broken
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

impl TemplateContent {
    pub fn validate_params(&self) -> Result<(), String> {
        for (name, spec) in &self.params {
            validate_param_name(name)?;
            validate_param_spec(name, spec)?;
        }
        Ok(())
    }

    pub fn validate_references(&self) -> Result<(), String> {
        match &self.format {
            TemplateFormat::Single { width, height, .. } => {
                for (dim_name, dim) in [("width", width), ("height", height)] {
                    match dim {
                        DynamicDimension::Fixed(DynamicValue::Ref(r)) => {
                            check_param_ref(
                                &self.params,
                                r,
                                &format!("format {dim_name}"),
                                &["length", "number", "integer"],
                            )?;
                        }
                        DynamicDimension::Dynamic { min, max } => {
                            if let Some(DynamicValue::Ref(r)) = min {
                                check_param_ref(
                                    &self.params,
                                    r,
                                    &format!("format {dim_name}.min"),
                                    &["length", "number", "integer"],
                                )?;
                            }
                            if let Some(DynamicValue::Ref(r)) = max {
                                check_param_ref(
                                    &self.params,
                                    r,
                                    &format!("format {dim_name}.max"),
                                    &["length", "number", "integer"],
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            TemplateFormat::Sheet { .. } => {}
        }

        match &self.layout {
            Layout::Items(items) => {
                for item in items {
                    validate_item_references(item, &self.params)?;
                }
            }
        }
        Ok(())
    }

    pub fn instantiate_with_defaults(&self) -> TemplateContent {
        TemplateContent {
            name: self.name.clone(),
            description: self.description.clone(),
            unit: self.unit.clone(),
            dpi: self.dpi,
            format: instantiate_format_defaults(&self.format, &self.params),
            params: self.params.clone(),
            layout: instantiate_layout_defaults(&self.layout, &self.params),
            version: self.version.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
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

        self.validate_params()?;
        self.validate_references()?;

        if let Some(options) = &self.options() {
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

        let instantiated = self.instantiate_with_defaults();

        // Require both bounds on a dynamic-width single before computing layout bounds,
        // so the caller gets the correct error rather than an out-of-bounds panic.
        if let TemplateFormat::Single {
            width: DynamicDimension::Dynamic { min, max },
            ..
        } = &instantiated.format
        {
            if min.is_none() || max.is_none() {
                return Err(
                    "a dynamic-width single template must specify both width.min and width.max"
                        .to_string(),
                );
            }
        }

        let geometry_values = load_geometry_values(self);
        let (val_frame, axes_resolved) = match &instantiated.format {
            TemplateFormat::Sheet {
                label_width,
                label_height,
                ..
            } => (Some((*label_width, *label_height)), [true, true]),
            TemplateFormat::Single { width, height, .. } => {
                let w = match width {
                    DynamicDimension::Fixed(DynamicValue::Literal(v)) => *v,
                    DynamicDimension::Dynamic {
                        max: Some(DynamicValue::Literal(v)),
                        ..
                    } => *v,
                    _ => 0.0,
                };
                let h = match height {
                    DynamicDimension::Fixed(DynamicValue::Literal(v)) => *v,
                    _ => 0.0,
                };
                let is_dynamic = matches!(width, DynamicDimension::Dynamic { .. });
                (Some((w, h)), [!is_dynamic, true])
            }
        };

        validate_layout(
            &instantiated.layout,
            instantiated.options().as_ref(),
            val_frame,
            axes_resolved,
            &geometry_values,
        )?;

        if let TemplateFormat::Single {
            media_width: Some(mw),
            ..
        } = &instantiated.format
        {
            if *mw <= 0.0 {
                return Err("media_width must be greater than 0".to_string());
            }
        }

        match &instantiated.format {
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

impl TemplateDefinition {
    pub fn instantiate_with_defaults(&self) -> TemplateDefinition {
        TemplateDefinition {
            id: self.id.clone(),
            group: self.group.clone(),
            content: self.content.instantiate_with_defaults(),
        }
    }
}

const RESERVED_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "COM¹", "COM²",
    "COM³", "LPT¹", "LPT²", "LPT³",
];

pub fn validate_group_segment(segment: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err("group path segment must not be empty".to_string());
    }
    if segment.chars().count() > 64 {
        return Err("group path segment must be at most 64 characters".to_string());
    }
    if segment.len() > 255 {
        return Err("group path segment must be at most 255 bytes".to_string());
    }
    if segment.chars().any(|c| c.is_control()) {
        return Err("group path segment must not contain control characters".to_string());
    }
    if segment
        .chars()
        .any(|c| matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err("group path segment contains invalid characters".to_string());
    }
    if segment == "." || segment == ".." {
        return Err("group path segment cannot be '.' or '..'".to_string());
    }
    if segment.starts_with(char::is_whitespace) || segment.ends_with(char::is_whitespace) {
        return Err("group path segment must not have leading or trailing whitespace".to_string());
    }
    if segment.starts_with('.') || segment.ends_with('.') {
        return Err("group path segment must not start or end with a period".to_string());
    }
    let base_name = segment.split('.').next().unwrap_or(segment);
    let base_upper = base_name.to_uppercase();
    if RESERVED_DEVICE_NAMES.iter().any(|&r| r == base_upper) {
        return Err(format!(
            "group path segment '{segment}' is a reserved device name"
        ));
    }
    Ok(())
}

pub fn validate_group_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("group path must not be empty".to_string());
    }
    if trimmed.chars().count() > 255 {
        return Err("group path must be at most 255 characters".to_string());
    }
    if trimmed.len() > 1024 {
        return Err("group path must be at most 1024 bytes".to_string());
    }
    for segment in trimmed.split('/') {
        validate_group_segment(segment)?;
    }
    Ok(trimmed.to_string())
}

fn validate_param_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("parameter name must not be empty".to_string());
    }
    // A parameter name must match ^[a-zA-Z0-9_-]+$. Dots separate namespaces and colons separate
    // formats in the token grammar; no words are reserved.
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "parameter name '{name}' contains invalid characters; must match ^[a-zA-Z0-9_-]+$"
        ));
    }
    Ok(())
}

fn validate_param_spec(name: &str, spec: &ParamSpec) -> Result<(), String> {
    match &spec.param_type {
        ParamType::Enum { values } => {
            if values.is_empty() {
                return Err(format!("parameter '{name}' enum values must not be empty"));
            }
            if values.iter().any(|opt| opt.trim().is_empty()) {
                return Err("options must not contain empty values".to_string());
            }
            if let Some(default) = &spec.default {
                let default_str = match default {
                    crate::models::ParamValue::String(s) => s.as_str(),
                    crate::models::ParamValue::Integer(i) => {
                        let s = i.to_string();
                        if !values.contains(&s) {
                            return Err(format!(
                                "parameter '{name}' default '{i}' is not in enum values"
                            ));
                        }
                        return Ok(());
                    }
                    _ => "",
                };
                if !values.iter().any(|v| v == default_str) {
                    return Err(format!(
                        "parameter '{name}' default '{default_str}' is not in enum values"
                    ));
                }
            }
        }
        ParamType::Length | ParamType::Number | ParamType::Integer => {
            if let (Some(min), Some(max)) = (spec.min, spec.max) {
                if min > max {
                    return Err(format!(
                        "parameter '{name}' min ({min}) must be <= max ({max})"
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn check_param_ref(
    params: &std::collections::BTreeMap<String, ParamSpec>,
    name: &str,
    context: &str,
    allowed_types: &[&str],
) -> Result<(), String> {
    let spec = params
        .get(name)
        .ok_or_else(|| format!("undeclared parameter '{name}' referenced in {context}"))?;
    let matches_type = match &spec.param_type {
        ParamType::Length => allowed_types.contains(&"length"),
        ParamType::Number => allowed_types.contains(&"number"),
        ParamType::Integer => allowed_types.contains(&"integer"),
        ParamType::String { .. } => allowed_types.contains(&"string"),
        ParamType::Boolean => allowed_types.contains(&"boolean"),
        ParamType::Enum { .. } => allowed_types.contains(&"enum"),
        ParamType::Datetime { .. } => allowed_types.contains(&"datetime"),
    };
    if !matches_type {
        return Err(format!(
            "parameter '{name}' of type {:?} cannot be used in {context}",
            spec.param_type
        ));
    }
    Ok(())
}

fn validate_when_references(
    when: Option<&std::collections::BTreeMap<String, String>>,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> Result<(), String> {
    if let Some(when) = when {
        for (name, val) in when {
            let spec = params.get(name).ok_or_else(|| {
                format!("undeclared parameter '{name}' referenced in when condition")
            })?;
            if let ParamType::Enum { values } = &spec.param_type {
                if !values.iter().any(|v| v == val) {
                    return Err(format!(
                        "when condition for '{name}' references '{val}' which is not in enum values"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_interpolated_string(
    s: &str,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> Result<(), String> {
    for scanned in crate::interpolation::scan_tokens(s) {
        let token = match crate::interpolation::parse(scanned.raw) {
            Ok(tok) => tok,
            Err(crate::interpolation::TokenError::UnknownSource {
                token: tok_str,
                source,
            }) => {
                if let Some(spec) = params.get(&source) {
                    if matches!(spec.param_type, crate::models::ParamType::Datetime { .. }) {
                        let inner = scanned
                            .raw
                            .strip_prefix('{')
                            .unwrap_or(scanned.raw)
                            .strip_suffix('}')
                            .unwrap_or(scanned.raw);
                        if let Some(key) = inner.strip_prefix(&format!("{source}.")) {
                            if !key.is_empty()
                                && !key.contains('.')
                                && !key.contains(':')
                                && crate::interpolation::is_valid_ident(key)
                            {
                                return Err(format!(
                                    "template contains '{tok_str}': unknown source '{source}'; use '{{{source}:{key}}}' instead"
                                ));
                            }
                        }
                    }
                }
                return Err(crate::interpolation::TokenError::UnknownSource {
                    token: tok_str,
                    source,
                }
                .to_string());
            }
            Err(e) => return Err(e.to_string()),
        };
        if let Some(fmt) = token.format {
            let is_instant = match token.source {
                crate::interpolation::Source::Sys(crate::interpolation::SysValue::Now) => true,
                crate::interpolation::Source::Bare(name) => params.get(name).is_some_and(|spec| {
                    matches!(spec.param_type, crate::models::ParamType::Datetime { .. })
                }),
                _ => false,
            };
            if !is_instant {
                return Err(format!(
                    "template contains '{}': format '{}' can only be applied to an instant (sys.now or type: datetime parameter)",
                    scanned.raw, fmt
                ));
            }
        }
    }
    Ok(())
}

fn validate_item_references(
    item: &LayoutItem,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> Result<(), String> {
    match item {
        LayoutItem::Text {
            value,
            placement,
            font_weight,
            when,
            ..
        } => {
            validate_when_references(when.as_ref(), params)?;
            validate_interpolated_string(value, params)?;
            if let Some(DynamicValue::Ref(ref_name)) = font_weight {
                check_param_ref(params, ref_name, "font_weight", &["integer"])?;
            }
            if let Extent::Size(size) = &placement.extent {
                for (axis, sv) in [("width", &size.0[0]), ("height", &size.0[1])] {
                    if let SizeValue::Dynamic(DynamicValue::Ref(ref_name)) = sv {
                        check_param_ref(
                            params,
                            ref_name,
                            &format!("text {axis}"),
                            &["length", "number", "integer"],
                        )?;
                    }
                }
            }
        }
        LayoutItem::Qr {
            value,
            placement,
            when,
            ..
        } => {
            validate_when_references(when.as_ref(), params)?;
            validate_interpolated_string(value, params)?;
            if let Extent::Size(size) = &placement.extent {
                for (axis, sv) in [("width", &size.0[0]), ("height", &size.0[1])] {
                    if let SizeValue::Dynamic(DynamicValue::Ref(ref_name)) = sv {
                        check_param_ref(
                            params,
                            ref_name,
                            &format!("qr {axis}"),
                            &["length", "number", "integer"],
                        )?;
                    }
                }
            }
        }
        LayoutItem::Image {
            name,
            src,
            placement,
            when,
            ..
        } => {
            validate_when_references(when.as_ref(), params)?;
            if let Some(n) = name {
                if n.is_empty()
                    || !n
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Err(format!(
                        "image name '{n}' contains invalid characters; must match ^[a-zA-Z0-9_-]+$"
                    ));
                }
            }
            if let Some(s) = src {
                validate_interpolated_string(s, params)?;
            }
            if let Extent::Size(size) = &placement.extent {
                for (axis, sv) in [("width", &size.0[0]), ("height", &size.0[1])] {
                    if let SizeValue::Dynamic(DynamicValue::Ref(ref_name)) = sv {
                        check_param_ref(
                            params,
                            ref_name,
                            &format!("image {axis}"),
                            &["length", "number", "integer"],
                        )?;
                    }
                }
            }
        }
        LayoutItem::Line { when, .. } => {
            validate_when_references(when.as_ref(), params)?;
        }
        LayoutItem::Container {
            placement,
            when,
            items,
            ..
        } => {
            validate_when_references(when.as_ref(), params)?;
            if let Extent::Size(size) = &placement.extent {
                for (axis, sv) in [("width", &size.0[0]), ("height", &size.0[1])] {
                    if let SizeValue::Dynamic(DynamicValue::Ref(ref_name)) = sv {
                        check_param_ref(
                            params,
                            ref_name,
                            &format!("container {axis}"),
                            &["length", "number", "integer"],
                        )?;
                    }
                }
            }
            for child in items {
                validate_item_references(child, params)?;
            }
        }
    }
    Ok(())
}

fn load_geometry_values(template: &TemplateContent) -> HashMap<String, f32> {
    let mut map = HashMap::new();
    for (name, spec) in &template.params {
        let val = match &spec.default {
            Some(crate::models::ParamValue::Float(f)) => *f,
            Some(crate::models::ParamValue::Integer(i)) => *i as f32,
            _ => match (spec.min, spec.max) {
                (Some(min), _) => min,
                (_, Some(max)) => max,
                _ => 0.0,
            },
        };
        map.insert(name.clone(), val);
    }
    map
}

fn resolve_f32_default(params: &std::collections::BTreeMap<String, ParamSpec>, name: &str) -> f32 {
    if let Some(spec) = params.get(name) {
        match &spec.default {
            Some(crate::models::ParamValue::Float(f)) => *f,
            Some(crate::models::ParamValue::Integer(i)) => *i as f32,
            Some(crate::models::ParamValue::String(s)) => {
                s.parse::<f32>().unwrap_or_else(|_| spec.min.unwrap_or(0.0))
            }
            _ => spec.min.unwrap_or(0.0),
        }
    } else {
        0.0
    }
}

fn resolve_u16_default(params: &std::collections::BTreeMap<String, ParamSpec>, name: &str) -> u16 {
    if let Some(spec) = params.get(name) {
        match &spec.default {
            Some(crate::models::ParamValue::Integer(i)) => *i as u16,
            Some(crate::models::ParamValue::Float(f)) => *f as u16,
            _ => 400,
        }
    } else {
        400
    }
}

fn instantiate_format_defaults(
    format: &TemplateFormat,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> TemplateFormat {
    match format {
        TemplateFormat::Single {
            width,
            height,
            media_width,
        } => {
            let inst_dim = |dim: &DynamicDimension| -> DynamicDimension {
                match dim {
                    DynamicDimension::Fixed(dv) => {
                        let val = match dv {
                            DynamicValue::Literal(v) => *v,
                            DynamicValue::Ref(r) => resolve_f32_default(params, r),
                        };
                        DynamicDimension::Fixed(DynamicValue::Literal(val))
                    }
                    DynamicDimension::Dynamic { min, max } => {
                        let inst_dv =
                            |dv: &Option<DynamicValue<f32>>| -> Option<DynamicValue<f32>> {
                                dv.as_ref().map(|v| match v {
                                    DynamicValue::Literal(val) => DynamicValue::Literal(*val),
                                    DynamicValue::Ref(r) => {
                                        DynamicValue::Literal(resolve_f32_default(params, r))
                                    }
                                })
                            };
                        DynamicDimension::Dynamic {
                            min: inst_dv(min),
                            max: inst_dv(max),
                        }
                    }
                }
            };
            TemplateFormat::Single {
                width: inst_dim(width),
                height: inst_dim(height),
                media_width: *media_width,
            }
        }
        TemplateFormat::Sheet { .. } => format.clone(),
    }
}

fn instantiate_layout_defaults(
    layout: &Layout,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> Layout {
    match layout {
        Layout::Items(items) => Layout::Items(
            items
                .iter()
                .map(|item| instantiate_item_defaults(item, params))
                .collect(),
        ),
    }
}

fn instantiate_item_defaults(
    item: &LayoutItem,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> LayoutItem {
    let inst_placement = |placement: &crate::models::Placement| -> crate::models::Placement {
        let extent = match &placement.extent {
            Extent::Size(Size([w, h])) => {
                let inst_sv = |sv: &SizeValue| -> SizeValue {
                    match sv {
                        SizeValue::Dynamic(DynamicValue::Ref(r)) => SizeValue::Dynamic(
                            DynamicValue::Literal(resolve_f32_default(params, r)),
                        ),
                        _ => sv.clone(),
                    }
                };
                Extent::Size(Size([inst_sv(w), inst_sv(h)]))
            }
            Extent::To(pos) => Extent::To(pos.clone()),
        };
        crate::models::Placement {
            at: placement.at.clone(),
            extent,
            max_w: placement.max_w,
            max_h: placement.max_h,
            rotate: placement.rotate,
        }
    };

    match item {
        LayoutItem::Text {
            value,
            placement,
            font_size,
            font_weight,
            multiline,
            alignment,
            overflow,
            when,
        } => {
            let fw = font_weight.as_ref().map(|w| match w {
                DynamicValue::Literal(v) => DynamicValue::Literal(*v),
                DynamicValue::Ref(r) => DynamicValue::Literal(resolve_u16_default(params, r)),
            });
            LayoutItem::Text {
                value: value.clone(),
                placement: inst_placement(placement),
                font_size: font_size.clone(),
                font_weight: fw,
                multiline: *multiline,
                alignment: alignment.clone(),
                overflow: *overflow,
                when: when.clone(),
            }
        }
        LayoutItem::Qr {
            value,
            placement,
            params: qr_params,
            when,
        } => LayoutItem::Qr {
            value: value.clone(),
            placement: inst_placement(placement),
            params: qr_params.clone(),
            when: when.clone(),
        },
        LayoutItem::Image {
            name,
            src,
            placement,
            fit,
            when,
        } => LayoutItem::Image {
            name: name.clone(),
            src: src.clone(),
            placement: inst_placement(placement),
            fit: *fit,
            when: when.clone(),
        },
        LayoutItem::Line {
            at,
            to,
            thickness,
            when,
        } => LayoutItem::Line {
            at: at.clone(),
            to: to.clone(),
            thickness: *thickness,
            when: when.clone(),
        },
        LayoutItem::Container {
            placement,
            when,
            frame,
            padding,
            items,
        } => LayoutItem::Container {
            placement: inst_placement(placement),
            when: when.clone(),
            frame: frame.clone(),
            padding: *padding,
            items: items
                .iter()
                .map(|child| instantiate_item_defaults(child, params))
                .collect(),
        },
    }
}

fn validate_dimension(name: &str, dimension: &DynamicDimension) -> Result<(), String> {
    match dimension {
        DynamicDimension::Fixed(DynamicValue::Literal(value)) => {
            if *value <= 0.0 {
                return Err(format!("{name} must be greater than 0"));
            }
        }
        DynamicDimension::Fixed(DynamicValue::Ref(_)) => {}
        DynamicDimension::Dynamic { min, max } => {
            if min.is_none() && max.is_none() {
                return Err(format!("{name} dynamic must specify min, max, or both"));
            }
            if let Some(DynamicValue::Literal(min)) = min {
                if *min <= 0.0 {
                    return Err(format!("min_{name} must be greater than 0"));
                }
            }
            if let Some(DynamicValue::Literal(max)) = max {
                if *max <= 0.0 {
                    return Err(format!("max_{name} must be greater than 0"));
                }
            }
            if let (Some(DynamicValue::Literal(min)), Some(DynamicValue::Literal(max))) = (min, max)
            {
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
    frame: Option<(f32, f32)>,
    axes_resolved: [bool; 2],
    geometry_values: &HashMap<String, f32>,
) -> Result<(), String> {
    match layout {
        Layout::Items(items) => {
            validate_layout_items(items, frame, axes_resolved, options, geometry_values)
        }
    }
}

fn validate_layout_items(
    items: &[LayoutItem],
    frame: Option<(f32, f32)>,
    axes_resolved: [bool; 2],
    options: Option<&Options>,
    geometry_values: &HashMap<String, f32>,
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
        validate_layout_item(item, frame, axes_resolved, options, geometry_values)?;
    }
    Ok(())
}

fn layout_item_name(item: &LayoutItem) -> Option<&str> {
    match item {
        LayoutItem::Image { name, .. } => name.as_deref(),
        LayoutItem::Text { .. }
        | LayoutItem::Qr { .. }
        | LayoutItem::Line { .. }
        | LayoutItem::Container { .. } => None,
    }
}

fn validate_layout_item(
    item: &LayoutItem,
    frame: Option<(f32, f32)>,
    axes_resolved: [bool; 2],
    options: Option<&Options>,
    geometry_values: &HashMap<String, f32>,
) -> Result<(), String> {
    match item {
        LayoutItem::Line {
            at,
            to,
            thickness,
            when,
        } => {
            validate_when(when.as_ref())?;
            const LINE_EPSILON: f32 = 1.0e-4;
            if *thickness <= 0.0 {
                return Err("line thickness must be greater than 0".to_string());
            }
            let (start, end) = match frame {
                Some((fw, fh)) => (
                    Point {
                        x: resolve_coord(at.x(), fw),
                        y: resolve_coord(at.y(), fh),
                    },
                    Point {
                        x: resolve_coord(to.x(), fw),
                        y: resolve_coord(to.y(), fh),
                    },
                ),
                None => (at.point(), to.point()),
            };
            let x_comparable =
                axes_resolved[0] || at.x().is_sign_negative() == to.x().is_sign_negative();
            let y_comparable =
                axes_resolved[1] || at.y().is_sign_negative() == to.y().is_sign_negative();
            let same_x = x_comparable && (start.x - end.x).abs() < LINE_EPSILON;
            let same_y = y_comparable && (start.y - end.y).abs() < LINE_EPSILON;
            if same_x && same_y {
                return Err("line start and end must differ".to_string());
            }
            if let Some((fw, fh)) = frame {
                for point in [start, end] {
                    if point.x < -LINE_EPSILON || point.y < -LINE_EPSILON {
                        return Err("line must fit within layout bounds".to_string());
                    }
                    if point.x > fw + LINE_EPSILON || point.y > fh + LINE_EPSILON {
                        return Err("line must fit within layout bounds".to_string());
                    }
                }
            }
        }
        LayoutItem::Text {
            value,
            placement,
            font_size,
            font_weight,
            when,
            ..
        } => {
            if value.trim().is_empty() {
                return Err("text value must not be empty".to_string());
            }
            validate_when(when.as_ref())?;
            validate_font_weight(font_weight.as_ref())?;
            validate_font_size(font_size)?;
            validate_placement(placement, false, frame, axes_resolved, geometry_values)?;
        }
        LayoutItem::Qr {
            value,
            placement,
            params,
            when,
            ..
        } => {
            if value.trim().is_empty() {
                return Err("qr value must not be empty".to_string());
            }
            validate_when(when.as_ref())?;
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
            let spec_0 = resolver::source_of(placement, 0, geometry_values);
            let spec_1 = resolver::source_of(placement, 1, geometry_values);
            if (spec_0.demands_intrinsic() || spec_1.demands_intrinsic())
                && params.as_ref().and_then(|p| p.module_size).is_none()
            {
                return Err("qr content or fill extent requires module_size".to_string());
            }
            validate_placement(placement, false, frame, axes_resolved, geometry_values)?;
        }
        LayoutItem::Image {
            placement, when, ..
        } => {
            validate_when(when.as_ref())?;
            validate_placement(placement, false, frame, axes_resolved, geometry_values)?;
        }
        LayoutItem::Container {
            placement,
            when,
            frame: cont_frame,
            padding,
            items,
        } => {
            validate_when(when.as_ref())?;
            if let Some(cf) = cont_frame {
                if cf.thickness <= 0.0 {
                    return Err("container frame thickness must be greater than 0".to_string());
                }
            }
            validate_placement(placement, true, frame, axes_resolved, geometry_values)?;

            // The frame validation runs against is the declared-default one, so the geometry the
            // children see is the resolver's, unmeasured, exactly as it is at render.
            let child_axes_resolved = resolver::container_inner_axes_resolved(
                placement,
                axes_resolved,
                resolver::rotation_of(placement),
                geometry_values,
            );

            let child_frame = match frame {
                Some(outer_frame) => {
                    let geometry = resolver::container_geometry(
                        placement,
                        padding,
                        outer_frame,
                        axes_resolved,
                        geometry_values,
                    );
                    if geometry.inner.0 <= resolver::BOUNDS_EPSILON
                        || geometry.inner.1 <= resolver::BOUNDS_EPSILON
                    {
                        return Err("container padding leaves no room for content".to_string());
                    }
                    Some(geometry.inner)
                }
                None => None,
            };

            validate_layout_items(
                items,
                child_frame,
                child_axes_resolved,
                options,
                geometry_values,
            )?;
        }
    }
    Ok(())
}

fn validate_placement(
    placement: &Placement,
    is_container: bool,
    frame: Option<(f32, f32)>,
    axes_resolved: [bool; 2],
    geometry_values: &HashMap<String, f32>,
) -> Result<(), String> {
    validate_rotation(&placement.rotate, is_container)?;

    if let Some(max_w) = placement.max_w {
        if max_w <= 0.0 {
            return Err("max_w must be greater than 0".to_string());
        }
    }
    if let Some(max_h) = placement.max_h {
        if max_h <= 0.0 {
            return Err("max_h must be greater than 0".to_string());
        }
    }

    for (axis, &is_resolved) in axes_resolved.iter().enumerate() {
        let spec = resolver::source_of(placement, axis, geometry_values);
        if spec.is_shrinking_to() && !is_resolved {
            return Err("extent shrinks as the label grows".to_string());
        }
    }

    // Every remaining rule about where a box lands is the resolver's, so load reports the same
    // refusals render does; only the words differ.
    match frame {
        Some(frame) => resolver::place(placement, frame, geometry_values, [None, None])
            .map(|_| ())
            .map_err(violation_message),
        None => resolver::precheck(placement, None, geometry_values).map_err(violation_message),
    }
}

/// How load words a resolver [`Violation`]. Load has no reason slugs, so the mapping is a table of
/// messages and nothing else; the rule that produced the violation lives in the resolver.
fn violation_message(violation: resolver::Violation) -> String {
    let label = if violation.axis() == 0 {
        "width"
    } else {
        "height"
    };
    let inverted = || "to must be above and to the right of at".to_string();
    match violation {
        resolver::Violation::AnchorBeforeFrame { .. }
        | resolver::Violation::AnchorBeyondFrame { .. } => {
            "at resolves outside the frame".to_string()
        }
        resolver::Violation::AuthoredExtentNotPositive { written_as_to, .. } => {
            if written_as_to {
                inverted()
            } else {
                format!("size {label} must be greater than 0")
            }
        }
        resolver::Violation::ExtentInverted { .. } => inverted(),
        resolver::Violation::ExtentNegative { .. } => {
            format!("size {label} must be greater than 0")
        }
        resolver::Violation::ExtentBeyondFrame { .. } => {
            "item does not fit within layout bounds".to_string()
        }
    }
}

fn validate_rotation(rotate: &Option<f32>, is_container: bool) -> Result<(), String> {
    if let Some(deg) = rotate {
        if !is_container {
            return Err("rotation is only supported on containers".to_string());
        }
        if crate::models::Rotation::from_degrees(*deg).is_none() {
            return Err("rotate must be a multiple of 90 degrees".to_string());
        }
    }
    Ok(())
}

fn validate_when(when: Option<&std::collections::BTreeMap<String, String>>) -> Result<(), String> {
    if let Some(when) = when {
        if when.is_empty() {
            return Err("when must not be empty".to_string());
        }
        for (name, value) in when {
            if name.trim().is_empty() || value.trim().is_empty() {
                return Err("when must not contain empty values".to_string());
            }
        }
    }
    Ok(())
}

/// The wght axis accepts any value in range; the multiple-of-100 rule is a CSS-style convention that
/// keeps templates predictable. Enforced here as well as in `convert.rs` so a `LayoutItem` built by
/// any route — including the many built directly in tests — is checked.
fn validate_font_weight(font_weight: Option<&DynamicValue<u16>>) -> Result<(), String> {
    match font_weight {
        Some(DynamicValue::Literal(weight))
            if !(100..=900).contains(weight) || weight % 100 != 0 =>
        {
            Err(format!(
                "font_weight must be a multiple of 100 between 100 and 900, got {weight}"
            ))
        }
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

impl From<&TemplateDefinition> for TemplateSummary {
    fn from(template: &TemplateDefinition) -> Self {
        Self {
            id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            group: template.group.clone(),
            unit: template.unit.clone(),
            dpi: template.dpi,
            params: template.params.clone(),
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
            group: template.group.clone(),
            unit: template.unit.clone(),
            dpi: template.dpi,
            format: template.format.clone(),
            params: template.params.clone(),
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
    fn copy_tree_into(src_root: &FsPath, current: &FsPath, dest_root: &FsPath) {
        for entry in std::fs::read_dir(current).unwrap_or_else(|e| panic!("read {current:?}: {e}"))
        {
            let path = entry.expect("dir entry").path();
            let meta = std::fs::symlink_metadata(&path).expect("stat entry");
            let rel = path.strip_prefix(src_root).expect("rel path");
            let target = dest_root.join(rel);
            if meta.is_dir() {
                std::fs::create_dir_all(&target).expect("create dir");
                copy_tree_into(src_root, &path, dest_root);
            } else if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).expect("create parent dir");
                }
                std::fs::copy(&path, target).expect("copy template");
            }
        }
    }
    copy_tree_into(FsPath::new("catalog"), FsPath::new("catalog"), &dir);
    copy_tree_into(
        FsPath::new("tests/fixtures/templates"),
        FsPath::new("tests/fixtures/templates"),
        &dir,
    );
    let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
    (registry, dir)
}

#[cfg(test)]
mod tests {
    use super::{
        list_template_groups, validate_group_name, validate_group_segment,
        validate_template_id_stem, TemplateContent, TemplateRegistry,
    };
    use crate::models::{
        Alignment, Dimension, DynamicDimension, DynamicValue, FontSize, Layout, LayoutItem,
        ParamSpec, ParamType, Position, Size, SizeValue, TemplateFormat,
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
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 45\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotation_rejected_on_non_container() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [40,10]\n    rotate: 90\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotation_zero_rejected_on_non_container() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [40,10]\n    rotate: 0\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    /// The ban on a rotated container sizing itself from its content is gone: rotation composes
    /// through the resolver, so the outer axes classify and resolve like any other container's.
    fn rotated_container_accepts_a_content_outer_size() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [content,40]\n    rotate: 90\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// So is the ban on content-sized descendants: the child resolves against the swapped canvas.
    #[test]
    fn rotated_container_accepts_a_content_child() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    items:\n      - type: text\n        value: hi\n        at: [0,0]\n        size: [content,10]\n        font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn rotated_container_child_bounds_use_swapped_canvas() {
        // physical 80x40 container, rotate 90 -> author canvas 40x80; a child 30 wide x 70 tall fits.
        let ok = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    items:\n      - type: text\n        value: hi\n        at: [0,0]\n        size: [30,70]\n        font_size: 6\n";
        assert!(parse_and_validate(ok).is_ok());
        // a child 50 wide exceeds the 40-wide author canvas -> error.
        let bad = ok.replace("size: [30,70]", "size: [50,70]");
        assert!(parse_and_validate(&bad).is_err());
    }

    #[test]
    fn validate_accepts_a_to_box_spanning_to_the_right_edge() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 0.0]\n    to: [-0.0, 12.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// `to` must be above and to the right of `at`.
    #[test]
    fn validate_rejects_an_inverted_to_box() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 40\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [20.0, 0.0]\n    to: [10.0, 12.0]\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// Rotated container with frame-dependent `to` is accepted under unified resolution.
    #[test]
    fn validate_accepts_a_rotated_container_with_a_frame_dependent_to() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    to: [-0.0, 12.0]\n    rotate: 90\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Both corners edge-relative is a constant 20-unit box, so the canvas is known and it is fine.
    #[test]
    fn validate_accepts_a_rotated_container_whose_corners_both_hug_the_edge() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 25, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [-20.0, 0.0]\n    to: [-0.0, 12.0]\n    rotate: 90\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// A container sized to the right edge is frame-dependent, and a content child resolves inside.
    #[test]
    fn validate_accepts_an_auto_child_inside_a_to_spanned_container() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 20, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [4.0, 0.0]\n    to: [-0.0, 12.0]\n    items:\n      - type: text\n        value: \"x\"\n        at: [2.0, 1.0]\n        size: [content, 10.0]\n        font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn rotated_container_rejects_nonpositive_content_area() {
        // author canvas is 40 wide x 80 tall; top+bottom padding 120 > 80 -> non-positive Ch.
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    padding: [60,0,60,0]\n    items: []\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("container padding leaves no room for content".to_string())
        );
    }

    #[test]
    fn unrotated_container_rejects_excessive_padding() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    padding: [0,50,0,50]\n    items: []\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("container padding leaves no room for content".to_string())
        );
    }

    /// Issue #154 repro 1: an unrotated container with fill width whose padding exceeds the resolved width.
    #[test]
    fn unrotated_container_with_auto_width_and_excessive_padding_rejected() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    size: [fill, 24.0]\n    padding: [0.0, 60.0, 0.0, 60.0]\n    items: []\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("container padding leaves no room for content".to_string())
        );
    }

    /// Issue #154 repro 2: an unrotated container capped by max_w whose padding exceeds the cap.
    #[test]
    fn unrotated_capped_container_with_excessive_padding_rejected() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    size: [fill, 24.0]\n    max_w: 50.0\n    padding: [0.0, 30.0, 0.0, 30.0]\n    items: []\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("container padding leaves no room for content".to_string())
        );
    }

    #[test]
    fn nested_container_padding_overflow_rejected() {
        let yaml = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0, 0]\n    size: [80, 40]\n    padding: [5, 5, 5, 5]\n    items:\n      - type: container\n        at: [0, 0]\n        size: [40, 20]\n        padding: [15, 0, 15, 0]\n        items: []\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("container padding leaves no room for content".to_string())
        );
    }

    #[test]
    fn rotation_orthogonal_on_container_ok() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: -90\n    items: []\n";
        assert!(parse_and_validate(yaml).is_ok());
    }

    /// A right-anchored box with a known width is legal on an auto-length label: its position is
    /// deferred, but its size never was.
    #[test]
    fn validate_accepts_a_right_anchored_fixed_width_box() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [20.0, 10.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// A shrinking `to` on an unresolved axis is rejected.
    #[test]
    fn validate_rejects_a_shrinking_to_on_an_unresolved_axis() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    to: [10.0, 10.0]\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// On a fixed frame everything resolves, so the same shape is fine.
    #[test]
    fn validate_accepts_a_right_anchored_auto_width_box_on_a_fixed_label() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [fill, 10.0]\n    max_w: 20.0\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Negative y is the top edge, so this box sits flush against it.
    #[test]
    fn validate_accepts_a_top_anchored_box() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, -4.0]\n    size: [20.0, 4.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// `x + width <= W` reduces to `at.x + width <= 0` for an edge-relative `at.x`: the frame width
    /// cancels, so a right-anchored box that overruns the right edge is decidable at load even on a
    /// dynamic-width label, and every render of it would fail.
    #[test]
    fn validate_rejects_a_right_anchored_box_that_overruns_the_right_edge() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 60 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-0.0, 2.0]\n    size: [10.0, 6.0]\n    font_size: 6\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("item does not fit within layout bounds".to_string())
        );
    }

    /// A plain endpoint past `width.max` can never render at any final width, so it is rejected at
    /// load rather than deferred to a render that is guaranteed to fail.
    #[test]
    fn validate_rejects_a_plain_line_endpoint_past_the_max_width() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 30 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [40.0, 6.0]\n    thickness: 0.2\n";
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
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [-30.0, 6.0]\n    to: [-0.0, 6.0]\n    thickness: 0.2\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn auto_spelling_is_rejected_at_parse_with_helpful_migration_message() {
        let yaml_container = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 40\nlayout:\n  - type: container\n    at: [0.0, 10.0]\n    size: [20.0, auto]\n    items: []\n";
        let err = parse_and_validate(yaml_container)
            .expect_err("auto on container height must be rejected");
        assert!(
            err.contains("`auto` was renamed"),
            "unexpected message: {err}"
        );

        let yaml_text = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 40\nlayout:\n  - type: text\n    value: \"hello\"\n    at: [0.0, 0.0]\n    size: [auto, 10.0]\n    font_size: 10\n";
        let err = parse_and_validate(yaml_text).expect_err("auto on text width must be rejected");
        assert!(
            err.contains("`auto` was renamed"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn resolver_source_of_and_available_and_claim_behave_consistently() {
        use crate::models::{Placement, Position, Size, SizeValue};
        use crate::resolver::{available, claim, requirement, source_of, Anchor, ExtentSource};
        use std::collections::HashMap;

        let geo = HashMap::new();
        let p_content = Placement::sized(
            Position([10.0, 0.0]),
            Size([SizeValue::content(), SizeValue::fixed(20.0)]),
        );
        let spec_w = source_of(&p_content, 0, &geo);
        assert_eq!(spec_w.source, ExtentSource::Content);
        assert_eq!(spec_w.anchor, Anchor::Plain(10.0));
        assert_eq!(available(100.0, &spec_w), 90.0);

        let p_fill = Placement::sized(
            Position([10.0, 0.0]),
            Size([SizeValue::fill(), SizeValue::fixed(20.0)]),
        );
        let spec_fill_w = source_of(&p_fill, 0, &geo);
        assert_eq!(spec_fill_w.source, ExtentSource::Frame);
        assert_eq!(available(100.0, &spec_fill_w), 90.0);

        // Claim on content respects intrinsic and cap
        let cl = claim(&spec_w, 100.0, 90.0, Some(50.0), Some(30.0));
        assert_eq!(cl, 30.0);
        let req = requirement(&spec_w, cl);
        assert_eq!(req, 40.0); // at (10) + claim (30)
    }

    #[test]
    fn a_fill_axis_falls_back_to_the_space_left_from_its_anchor() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 40\nlayout:\n  - type: container\n    at: [0.0, 10.0]\n    size: [20.0, fill]\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn an_edge_relative_anchor_is_resolved_before_the_subtraction() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 40\nlayout:\n  - type: container\n    at: [0.0, -5.0]\n    size: [20.0, fill]\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn a_to_extent_is_not_narrowed_twice_by_its_anchor() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [20.0, 0.0]\n    to: [-0.0, 12.0]\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn validate_accepts_a_capped_container_that_fits_the_remaining_width() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [90.0, 0.0]\n    size: [fill, 12.0]\n    max_w: 30.0\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn the_155_repro_validates_and_its_height_is_capped() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 60 }\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 0.0]\n    size: [20.0, fill]\n    max_h: 200.0\n    font_size: 8\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn text_fill_height_on_a_fixed_label_falls_back_to_the_remainder() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 100\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 10.0]\n    size: [20.0, fill]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn text_fill_height_with_an_oversized_max_h_is_not_rejected_on_a_fixed_label() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 100\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 10.0]\n    size: [20.0, fill]\n    max_h: 35.0\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn text_content_width_on_a_sheet_validates_ok() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: sheet\n  paper_width: 100\n  paper_height: 100\n  label_width: 40\n  label_height: 20\n  positions: [[0.0, 0.0]]\nlayout:\n  - type: text\n    value: \"x\"\n    at: [5.0, 2.0]\n    size: [content, 8.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    #[test]
    fn the_origin_is_not_exempt() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 100\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 0.0]\n    size: [20.0, fill]\n    font_size: 6\n";
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
    fn validate_rejects_empty_option_value() {
        let template = TemplateContent {
            name: "Label".to_string(),
            description: "desc".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(12.0).into(),
                height: Dimension::Fixed(25.0).into(),
                media_width: None,
            },
            params: BTreeMap::from([(
                "variant".to_string(),
                ParamSpec {
                    param_type: ParamType::Enum {
                        values: vec!["".to_string()],
                    },
                    default: None,
                    min: None,
                    max: None,
                    description: None,
                },
            )]),
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
    value: "{message}"
    at: [0.0, 0.0]
    size: [10.0, 5.0]
    font_size: 10.0
    multiline: true
"#,
        );

        // A non-YAML file in the same dir is ignored: neither served nor reported broken. An
        // uppercase extension is not, the filter lowercases before matching.
        write_template(&dir, "notes.txt", "id: sample\n");
        write_template(
            &dir,
            "SHOUTED.YAML",
            r#"
name: Shouted
description: Uppercase extension
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
        assert_eq!(registry.len(), 2);
        assert!(registry.get("sample").is_some());
        assert!(registry.get("SHOUTED").is_some());
        assert!(registry.broken().is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    fn sample_yaml(name: &str) -> String {
        format!(
            r#"
name: {name}
description: d
unit: mm
dpi: 300
format:
  type: single
  width: 12.0
  height: 25.0
layout: []
"#
        )
    }

    #[test]
    fn duplicate_id_serves_first_filename_and_quarantines_the_collider() {
        for (label, first_written, second_written) in [
            ("dup_az", "a.yaml", "sub/a.yaml"),
            ("dup_za", "sub/a.yaml", "a.yaml"),
        ] {
            let dir = temp_dir(label);
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            write_template(&dir, first_written, &sample_yaml("dup"));
            write_template(&dir, second_written, &sample_yaml("dup"));

            let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");

            assert_eq!(registry.len(), 1, "{label}: only the winner is served");
            assert!(registry.get("a").is_some(), "{label}: id is still served");
            assert_eq!(
                registry.path("a").and_then(|p| p.file_name()),
                Some(std::ffi::OsStr::new("a.yaml")),
                "{label}: a.yaml wins the id"
            );

            let broken = registry.broken();
            assert_eq!(broken.len(), 1, "{label}: one file refused");
            assert_eq!(
                broken[0].path, "sub/a.yaml",
                "{label}: sub/a.yaml is refused"
            );
            assert!(
                broken[0].error.contains("a") && broken[0].error.contains("a.yaml"),
                "{label}: message names the id and the file it collides with: {}",
                broken[0].error
            );

            fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn duplicates_records_the_refused_file_per_id() {
        let dir = temp_dir("dup_map");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        write_template(&dir, "a.yaml", &sample_yaml("dup"));
        write_template(&dir, "sub/a.yaml", &sample_yaml("dup"));
        write_template(&dir, "solo.yaml", &sample_yaml("solo"));

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");

        let refused: Vec<_> = registry
            .duplicates("a")
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(refused, vec!["sub/a.yaml"], "the loser is recorded for 'a'");
        assert!(
            registry.duplicates("solo").is_empty(),
            "an uncontested id records no duplicate"
        );
        assert!(
            registry.duplicates("absent").is_empty(),
            "an id the registry does not hold records no duplicate"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_id_leaves_unrelated_templates_served() {
        let dir = temp_dir("dup_sibling");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        write_template(&dir, "a.yaml", &sample_yaml("dup"));
        write_template(&dir, "sub/a.yaml", &sample_yaml("dup"));
        write_template(&dir, "other.yaml", &sample_yaml("other"));

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");

        assert_eq!(registry.len(), 2);
        assert!(registry.get("other").is_some());
        assert_eq!(
            registry.path("other").and_then(|p| p.file_name()),
            Some(std::ffi::OsStr::new("other.yaml"))
        );
        assert_eq!(registry.broken().len(), 1);
        assert_eq!(registry.broken()[0].path, "sub/a.yaml");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_token_quarantines_template_and_serves_valid() {
        let dir = temp_dir("bad_token_quarantine");
        let valid_yaml = sample_yaml("valid");
        let bad_yaml = r#"
name: Bad
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{datetime.long_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        write_template(&dir, "valid.yaml", &valid_yaml);
        write_template(&dir, "bad.yaml", bad_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        assert_eq!(registry.len(), 1);
        assert!(registry.get("valid").is_some());
        assert!(registry.get("bad").is_none());

        let broken = registry.broken();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].path, "bad.yaml");
        assert!(broken[0].error.contains("unknown source 'datetime'"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn summaries_are_sorted_by_id() {
        let dir = temp_dir("sorted");
        write_template(
            &dir,
            "b.yaml",
            r#"
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
            let val = DynamicValue::Literal(bad);
            let err = super::validate_font_weight(Some(&val)).expect_err("must be rejected");
            assert!(err.contains("font_weight"), "unexpected message: {err}");
        }
        for good in [100u16, 400, 900] {
            let val = DynamicValue::Literal(good);
            super::validate_font_weight(Some(&val)).expect("must be accepted");
        }
        super::validate_font_weight(None).expect("absent is valid");
    }

    /// The unit test above passes even if nothing ever calls the validator. This one fails unless
    /// `validate` actually reaches it, which is the point of centralising the rule here rather than
    /// only in `convert.rs`: a `LayoutItem` built directly — as most of this suite does — is checked.
    #[test]
    fn validate_rejects_a_text_item_with_a_bad_font_weight() {
        let template = TemplateContent {
            name: "w".to_string(),
            description: "w".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "value".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(10.0), SizeValue::fixed(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: Some(DynamicValue::Literal(350)),
                multiline: false,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        let err = template.validate().expect_err("350 must not validate");
        assert!(err.contains("font_weight"), "unexpected message: {err}");
    }

    #[test]
    fn validate_rejects_duplicate_field_names() {
        let template = TemplateContent {
            name: "dup".to_string(),
            description: "dup".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(12.0).into(),
                height: Dimension::Fixed(25.0).into(),
                media_width: None,
            },
            params: BTreeMap::from([(
                "variant".to_string(),
                ParamSpec {
                    param_type: ParamType::Enum {
                        values: vec!["default".to_string()],
                    },
                    default: None,
                    min: None,
                    max: None,
                    description: None,
                },
            )]),
            layout: Layout::Items(vec![
                LayoutItem::Image {
                    name: Some("logo".to_string()),
                    src: None,
                    placement: crate::models::Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(1.0), SizeValue::fixed(1.0)]),
                    ),
                    fit: crate::models::Fit::Contain,
                    when: None,
                },
                LayoutItem::Image {
                    name: Some("logo".to_string()),
                    src: None,
                    placement: crate::models::Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(1.0), SizeValue::fixed(1.0)]),
                    ),
                    fit: crate::models::Fit::Contain,
                    when: None,
                },
            ]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("duplicate layout item name"));
    }

    #[test]
    fn parse_rejects_text_with_name() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  width: 10
  height: 10
layout:
  - type: text
    name: bad_name
    at: [0, 0]
    size: [10, 5]
    font_size: 8
"#;
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn validate_rejects_empty_text_value() {
        let template = TemplateContent {
            name: "Empty Text".to_string(),
            description: "test".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(10.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(10.0), SizeValue::fixed(5.0)]),
                ),
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
                multiline: false,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert_eq!(err, "text value must not be empty");
    }

    #[test]
    fn validate_rejects_empty_qr_value() {
        let template = TemplateContent {
            name: "Empty Qr".to_string(),
            description: "test".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(10.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Qr {
                value: "".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(10.0), SizeValue::fixed(10.0)]),
                ),
                params: None,
                when: None,
            }]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert_eq!(err, "qr value must not be empty");
    }

    #[test]
    fn validate_rejects_degenerate_line() {
        let template = TemplateContent {
            name: "ln".to_string(),
            description: "ln".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Line {
                at: Position([1.0, 1.0]),
                to: Position([1.0, 1.0]),
                thickness: 0.2,
                when: None,
            }]),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("line start and end must differ"));
    }

    fn single_line_template(at: Position, to: Position) -> TemplateContent {
        TemplateContent {
            name: "ln".to_string(),
            description: "ln".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Line {
                at,
                to,
                thickness: 0.2,
                when: None,
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
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [-0.0, 6.0]\n    thickness: 0.2\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Still degenerate after resolution: both endpoints land on the right edge.
    #[test]
    fn validate_rejects_a_line_degenerate_after_resolution() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 40\n  height: 12\nlayout:\n  - type: line\n    at: [-0.0, 6.0]\n    to: [-0.0, 6.0]\n    thickness: 0.2\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// An inset larger than the widest the label can ever be never resolves to a valid coordinate.
    #[test]
    fn validate_rejects_a_line_inset_larger_than_the_max_width() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [-140.0, 6.0]\n    thickness: 0.2\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn dynamic_width_single_requires_both_bounds() {
        // Only min is set; max is None. Validate should reject this.
        let template = TemplateContent {
            name: "Tape".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: None,
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "hello".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(8.0), SizeValue::fixed(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
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
    fn dynamic_width_single_fill_width_item_at_offset_validates_ok() {
        let template = TemplateContent {
            name: "Tape2".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: crate::models::Placement::sized(
                    Position([5.0, 0.0]),
                    Size([SizeValue::fill(), SizeValue::fixed(12.0)]),
                ),
                when: None,
                frame: None,
                padding: crate::models::Padding::ZERO,
                items: vec![],
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with fill-width container at offset should validate OK");
    }

    #[test]
    fn dynamic_width_single_allows_multiline_text() {
        let template = TemplateContent {
            name: "Tape Multiline".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "hello".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(8.0), SizeValue::fixed(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: true,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with multiline: true should validate OK");
    }

    #[test]
    fn dynamic_width_single_allows_single_line_text() {
        let template = TemplateContent {
            name: "Tape Single Line".to_string(),
            description: "tape".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "hello".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(8.0), SizeValue::fixed(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with multiline: false should validate OK");
    }

    #[test]
    fn fixed_width_single_allows_multiline_text() {
        let template = TemplateContent {
            name: "Fixed Multiline".to_string(),
            description: "fixed".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(50.0).into(),
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "hello".to_string(),
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(40.0), SizeValue::fixed(6.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: true,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        template
            .validate()
            .expect("fixed-width single with multiline: true should validate OK");
    }

    #[test]
    fn single_rejects_nonpositive_media_width() {
        let build = |mw: Option<f32>| TemplateContent {
            name: "MW Test".to_string(),
            description: "test".to_string(),
            unit: "mm".to_string(),
            dpi: 300,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(50.0).into(),
                height: Dimension::Fixed(12.0).into(),
                media_width: mw,
            },
            params: BTreeMap::new(),
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
            "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 10\n  height: 10\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [10,5]\n    font_size: 6\n",
        );
        let reg = TemplateRegistry::load_from_dir(&dir).expect("load");
        let hash = reg.content_hash("a").expect("hash present");
        assert_eq!(hash.len(), 64, "sha-256 hex is 64 chars");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(reg.content_hash("missing").is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn raw_template_deserializes_params_dynamic_values_and_when() {
        let yaml = r#"
name: Test Params
unit: mm
dpi: 200
params:
  message:
    type: string
    description: "Main label text"
  target_width:
    type: length
    default: 80
    min: 25
    max: 300
  weight:
    type: integer
    default: 400
    enum: [400, 700]
  show_border:
    type: boolean
    default: false
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: horizontal
format:
  type: single
  height: 18
  width:
    min: 25
    max: "{target_width}"
layout:
  - type: text
    value: "{message}"
    size: [content, fill]
    font_size: 10
    font_weight: "{weight}"
    when:
      show_border: "true"
      orientation: "vertical"
"#;
        let raw: crate::raw::RawTemplate =
            serde_yaml_ng::from_str(yaml).expect("parse raw template");
        assert!(raw.params.is_some());
        let template = TemplateContent::try_from(raw).expect("convert template");
        assert_eq!(template.params.len(), 5);

        // Check format has dynamic max width ref
        match &template.format {
            TemplateFormat::Single { width, .. } => match width {
                crate::models::DynamicDimension::Dynamic { max, .. } => {
                    assert!(
                        matches!(max, Some(crate::models::DynamicValue::Ref(r)) if r == "target_width")
                    );
                }
                _ => panic!("expected dynamic dimension"),
            },
            _ => panic!("expected single format"),
        }
    }

    #[test]
    fn parameter_names_character_class_and_unreserved() {
        let good_names = ["datetime", "vars", "sys", "field_1", "my-param"];
        for name in good_names {
            let yaml = format!(
                "name: T\nunit: mm\ndpi: 200\nparams:\n  {name}:\n    type: string\nformat:\n  type: single\n  height: 12\n  width: 50\nlayout: []"
            );
            let res = parse_and_validate(&yaml);
            assert!(res.is_ok(), "should accept valid parameter name '{name}'");
        }

        let bad_names = [
            "datetime.iso",
            "vars.site",
            "invalid.dot",
            "printed_on:long_date",
            "has space",
        ];
        for name in bad_names {
            let yaml = format!(
                "name: T\nunit: mm\ndpi: 200\nparams:\n  {name}:\n    type: string\nformat:\n  type: single\n  height: 12\n  width: 50\nlayout: []"
            );
            let res = parse_and_validate(&yaml);
            assert!(
                res.is_err(),
                "should reject invalid parameter name '{name}'"
            );
        }
    }

    #[test]
    fn load_time_token_validation_refusals() {
        // Unknown source
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{datetime.long_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("unknown source 'datetime'"));
        assert!(err.contains("{sys.now:long_date}"));

        // Unknown system value
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{sys.nwo}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("unknown system value 'nwo'"));

        // Dotted rewrite of sys.now
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{sys.now.long_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("unknown system value 'now.long_date'"));
        assert!(err.contains("{sys.now:long_date}"));

        // Format on string parameter
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  title:
    type: string
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{title:long_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("can only be applied to an instant"));

        // Format on vars key
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{vars.qr_base_url:long_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("can only be applied to an instant"));

        // Empty format name
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{printed_on:}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("invalid format"));

        // Image name invalid
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: image
    name: "bad image name"
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("image name 'bad image name' contains invalid characters"));

        // Dotted datetime parameter unknown source with suggested replacement
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{printed_on.short_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("{printed_on.short_date}"));
        assert!(err.contains("unknown source 'printed_on'"));
        assert!(err.contains("{printed_on:short_date}"));

        // Image src with unknown source
        let yaml = r#"
name: T
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: image
    src: "logos/{datetime.brand}.png"
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.contains("unknown source 'datetime'"));

        // Valid image src and datetime format
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format:
  type: single
  height: 12
  width: 50
layout:
  - type: image
    src: "logos/{vars.brand}.png"
    at: [0, 0]
    size: [10, 10]
  - type: text
    value: "{printed_on:short_date} {sys.now:iso_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        assert!(parse_and_validate(yaml).is_ok());
    }

    #[test]
    fn reject_referencing_undeclared_parameter() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  declared_param:
    type: length
    default: 50
format:
  type: single
  height: 18
  width:
    min: 25
    max: "{undeclared_param}"
layout: []
"#;
        let res = parse_and_validate(yaml);
        assert!(res.is_err(), "should reject undeclared parameter reference");
    }

    #[test]
    fn reject_type_mismatch_in_layout_parameter_reference() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  string_param:
    type: string
format:
  type: single
  height: 18
  width: 50
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
    font_weight: "{string_param}"
"#;
        let res = parse_and_validate(yaml);
        assert!(
            res.is_err(),
            "should reject string parameter in font_weight"
        );
    }

    /// A `datetime` parameter names an instant, so it can never stand in for a number. Every
    /// numeric context refuses it at load rather than at render.
    #[test]
    fn reject_datetime_parameter_in_numeric_contexts() {
        let font_weight = r#"
name: T
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format:
  type: single
  height: 18
  width: 50
layout:
  - type: text
    value: "{printed_on}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
    font_weight: "{printed_on}"
"#;
        assert!(
            parse_and_validate(font_weight).is_err(),
            "should reject datetime parameter in font_weight"
        );

        let width = r#"
name: T
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format:
  type: single
  height: 18
  width: "{printed_on}"
layout:
  - type: text
    value: "{printed_on}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        assert!(
            parse_and_validate(width).is_err(),
            "should reject datetime parameter as a format width"
        );
    }

    #[test]
    fn validate_bounds_instantiating_parameter_defaults() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  box_w:
    type: length
    default: 10
format:
  type: single
  height: 18
  width: 50
layout:
  - type: container
    at: [0, 0]
    size: ["{box_w}", 10]
    items: []
"#;
        let res = parse_and_validate(yaml);
        assert!(res.is_ok(), "validates cleanly using default box_w = 10");
    }

    #[test]
    fn reject_enum_default_not_in_values() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: diagonal
format:
  type: single
  height: 18
  width: 50
layout: []
"#;
        let res = parse_and_validate(yaml);
        assert!(res.is_err(), "should reject enum default not in values");
    }

    #[test]
    fn reject_parameter_min_greater_than_max() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  custom_w:
    type: length
    min: 100
    max: 50
format:
  type: single
  height: 18
  width: 50
layout: []
"#;
        let res = parse_and_validate(yaml);
        assert!(res.is_err(), "should reject min > max");
    }

    #[test]
    fn reject_default_bounds_overflow() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  box_w:
    type: length
    default: 60
format:
  type: single
  height: 18
  width: 50
layout:
  - type: container
    at: [0, 0]
    size: ["{box_w}", 10]
    items: []
"#;
        let res = parse_and_validate(yaml);
        assert!(
            res.is_err(),
            "should reject default box_w = 60 exceeding width 50"
        );
    }

    #[test]
    fn template_id_stem_validation() {
        assert!(validate_template_id_stem("valid-id_123"));
        assert!(!validate_template_id_stem(""));
        assert!(!validate_template_id_stem("has space"));
        assert!(!validate_template_id_stem("has.dot"));
        assert!(!validate_template_id_stem("has/slash"));
    }

    #[test]
    fn group_segment_validation_rules() {
        assert!(validate_group_segment("Warehouse").is_ok());
        assert!(validate_group_segment("").is_err());
        assert!(validate_group_segment("   ").is_err());
        assert!(validate_group_segment("trailing-dot.").is_err());
        assert!(validate_group_segment("trailing-space ").is_err());
        assert!(validate_group_segment(" leading-space").is_err());
        assert!(validate_group_segment(".leading-dot").is_err());
        assert!(validate_group_segment(".").is_err());
        assert!(validate_group_segment("..").is_err());
        assert!(validate_group_segment("CON").is_err());
        assert!(validate_group_segment("con").is_err());
        assert!(validate_group_segment("CON.txt").is_err());
        assert!(validate_group_segment("LPT1").is_err());
        assert!(validate_group_segment("COM¹").is_err());
        assert!(validate_group_segment("contains/slash").is_err());
        assert!(validate_group_segment("has\nnewline").is_err());
        assert!(validate_group_segment("has\ttab").is_err());
        let long_65 = "a".repeat(65);
        assert!(validate_group_segment(&long_65).is_err());
    }

    #[test]
    fn group_name_path_validation() {
        assert_eq!(
            validate_group_name("Shipping/Pallets").unwrap(),
            "Shipping/Pallets"
        );
        assert_eq!(
            validate_group_name("  Shipping/Pallets  ").unwrap(),
            "Shipping/Pallets"
        );
        assert!(validate_group_name("").is_err());
        assert!(validate_group_name("   ").is_err());
        assert!(validate_group_name("Shipping//Pallets").is_err());
        assert!(validate_group_name("/Shipping").is_err());
        assert!(validate_group_name("Shipping/").is_err());
        assert!(validate_group_name("CON/Pallets").is_err());
        assert!(validate_group_name("Shipping/CON").is_err());
        assert!(validate_group_name("Shipping/./Pallets").is_err());
        assert!(validate_group_name("Shipping/../Pallets").is_err());
        let long_256 = format!("{}/{}", "a".repeat(64), "b".repeat(64)).repeat(3);
        assert!(validate_group_name(&long_256).is_err());
    }

    #[test]
    fn list_template_groups_orders_and_filters() {
        let temp = std::env::temp_dir().join(format!("test-groups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(temp.join("Shipping/Pallets")).unwrap();
        std::fs::create_dir_all(temp.join("Shipping/Boxes")).unwrap();
        std::fs::create_dir_all(temp.join("Archive")).unwrap();
        std::fs::create_dir_all(temp.join(".hidden/Sub")).unwrap();
        std::fs::create_dir_all(temp.join("Invalid:Name/Sub")).unwrap();

        let groups = list_template_groups(&temp).unwrap();
        assert_eq!(
            groups,
            vec!["Archive", "Shipping", "Shipping/Boxes", "Shipping/Pallets"]
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn load_from_dir_handles_nesting_dot_precedence_invalid_dirs_and_stems() {
        let dir = temp_dir("load_nesting");
        std::fs::create_dir_all(dir.join("nested/sub")).unwrap();
        std::fs::create_dir_all(dir.join(".dot_dir/invalid:name")).unwrap();
        std::fs::create_dir_all(dir.join("invalid:group")).unwrap();

        write_template(&dir, "nested/sub/t1.yaml", &sample_yaml("Nested 1"));
        write_template(
            &dir,
            ".dot_dir/invalid:name/t2.yaml",
            &sample_yaml("Dot Ignored"),
        );
        write_template(&dir, "invalid:group/t3.yaml", &sample_yaml("Invalid Dir"));
        write_template(&dir, "bad stem.yaml", &sample_yaml("Bad Stem"));
        write_template(&dir, "good_stem.yaml", &sample_yaml("Good Stem"));

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");

        assert_eq!(registry.len(), 2);
        let t1 = registry.get("t1").expect("t1 loaded");
        assert_eq!(t1.group.as_deref(), Some("nested/sub"));
        let good = registry.get("good_stem").expect("good_stem loaded");
        assert_eq!(good.group, None);

        // Dot dir is completely skipped (no broken entry for t2)
        assert!(registry.get("t2").is_none());

        let broken = registry.broken();
        assert_eq!(broken.len(), 2);

        let invalid_dir_broken = broken
            .iter()
            .find(|b| b.path == "invalid:group/t3.yaml")
            .expect("invalid dir reported broken");
        assert!(invalid_dir_broken.error.contains("invalid:group"));

        let bad_stem_broken = broken
            .iter()
            .find(|b| b.path == "bad stem.yaml")
            .expect("bad stem reported broken");
        assert!(bad_stem_broken.error.contains("bad stem"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn id_contest_won_by_location_valid_file() {
        let dir = temp_dir("id_contest_validity");
        std::fs::create_dir_all(dir.join("invalid:dir")).unwrap();
        std::fs::create_dir_all(dir.join("valid")).unwrap();

        // invalid:dir/contest.yaml sorts before valid/contest.yaml lexically,
        // but invalid:dir fails group name validation and cannot contest the ID.
        write_template(
            &dir,
            "invalid:dir/contest.yaml",
            &sample_yaml("Invalid Loc"),
        );
        write_template(&dir, "valid/contest.yaml", &sample_yaml("Valid Loc"));

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");

        assert_eq!(registry.len(), 1);
        let contest = registry.get("contest").expect("contest loaded");
        assert_eq!(contest.group.as_deref(), Some("valid"));
        assert!(
            registry.duplicates("contest").is_empty(),
            "invalid location file does not register as an id duplicate"
        );

        let broken = registry.broken();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].path, "invalid:dir/contest.yaml");
        assert!(broken[0].error.contains("invalid:dir"));

        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn load_from_dir_skips_symlink_directories() {
        let dir = temp_dir("symlink_dir_skip");
        let target = dir.join("real_dir");
        std::fs::create_dir_all(&target).unwrap();
        write_template(&dir, "real_dir/real.yaml", &sample_yaml("Real"));

        let symlink = dir.join("sym_dir");
        std::os::unix::fs::symlink(&target, &symlink).unwrap();

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.get("real").unwrap().group.as_deref(),
            Some("real_dir")
        );
        assert!(registry.broken().is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn load_from_dir_handles_non_utf8_paths() {
        use std::os::unix::ffi::OsStrExt;

        let dir = temp_dir("non_utf8");
        let non_utf8_filename = std::ffi::OsStr::from_bytes(b"non_utf8_\xff.yaml");
        let path = dir.join(non_utf8_filename);
        std::fs::write(&path, sample_yaml("Non UTF8")).unwrap();

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        assert_eq!(registry.len(), 0);
        let broken = registry.broken();
        assert_eq!(broken.len(), 1);
        assert!(broken[0].error.contains("not valid UTF-8"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn line_validation_with_unresolved_vertical_axis_is_not_rejected() {
        let yaml = r#"
name: Unresolved Vertical Line
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, content]
    items:
      - type: line
        at: [10, 10]
        to: [20, -10]
        thickness: 0.5
      - type: text
        value: "Hello"
        at: [0, 0]
        size: [100, 30]
        font_size: 12
"#;
        let template = parse_and_validate(yaml);
        assert!(
            template.is_ok(),
            "unresolved vertical line must validate: {template:?}"
        );
    }
}
