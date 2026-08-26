use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::errors::TemplateError;
use crate::models::{
    resolve_coord, DynamicDimension, DynamicValue, Extent, FontSize, Layout, LayoutItem, Options,
    ParamSpec, ParamType, Point, Position, Size, SizeValue, TemplateDetail, TemplateFormat,
    TemplateSummary,
};
use crate::parse::parse_template;

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

        let bounds = layout_bounds(&instantiated.format)?;
        let is_dynamic_width = matches!(
            &instantiated.format,
            TemplateFormat::Single {
                width: DynamicDimension::Dynamic { .. },
                ..
            }
        );
        validate_layout(
            &instantiated.layout,
            instantiated.options().as_ref(),
            bounds.as_ref(),
            is_dynamic_width,
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
    if name == "datetime" || name == "vars" {
        return Err(format!("parameter name '{name}' is reserved"));
    }
    if name.starts_with("datetime.") || name.starts_with("vars.") {
        return Err(format!("parameter name '{name}' uses a reserved prefix"));
    }
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

fn validate_item_references(
    item: &LayoutItem,
    params: &std::collections::BTreeMap<String, ParamSpec>,
) -> Result<(), String> {
    match item {
        LayoutItem::Text {
            placement,
            font_weight,
            when,
            ..
        } => {
            validate_when_references(when.as_ref(), params)?;
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
            placement, when, ..
        } => {
            validate_when_references(when.as_ref(), params)?;
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
            placement, when, ..
        } => {
            validate_when_references(when.as_ref(), params)?;
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
        LayoutItem::Image { name, .. } => name.as_deref(),
        LayoutItem::Text { .. }
        | LayoutItem::Qr { .. }
        | LayoutItem::Line { .. }
        | LayoutItem::Container { .. } => None,
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
            value,
            placement,
            font_size,
            font_weight,
            ..
        } => {
            if value.trim().is_empty() {
                return Err("text value must not be empty".to_string());
            }
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_font_weight(font_weight.as_ref())?;
            validate_rotation(&placement.rotate, false)?;
            // `allow_auto_fill` is always `true` for text: this axis asks whether the item type has
            // a frame to fall back on (it does), not whether this template's width is dynamic. Keying
            // it off `is_dynamic_width` instead was #155: it let validation accept a `max_h` above the
            // frame on the strength of a fallback the render path (keyed off the item's own
            // frame-dependence) did not have.
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                layout_bounds,
                true,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
            validate_font_size(font_size)?;
        }
        LayoutItem::Qr {
            value,
            placement,
            params,
            ..
        } => {
            if value.trim().is_empty() {
                return Err("qr value must not be empty".to_string());
            }
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_rotation(&placement.rotate, false)?;
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
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
        LayoutItem::Image { placement, .. } => {
            validate_placement_position(
                &placement.at,
                placement.width_is_frame_dependent(),
                layout_bounds,
                is_dynamic_width,
            )?;
            validate_rotation(&placement.rotate, false)?;
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                layout_bounds,
                false,
            )?;
            validate_bounds(&placement.at, width, height, layout_bounds)?;
        }
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
            when,
            frame,
            padding,
            items,
        } => {
            validate_when(when.as_ref())?;
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
                    Extent::To(_) => placement.width_is_frame_dependent(),
                };
                if unresolvable {
                    return Err(
                        "rotated container must have fixed size (no auto or dynamic dimensions)"
                            .to_string(),
                    );
                }
                if subtree_uses_auto(items) {
                    return Err("auto size is not allowed inside a rotated container".to_string());
                }
            }
            let (width, height) = resolve_size(
                &placement.at,
                &placement.extent,
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
            if inner_width <= CONTENT_EPSILON || inner_height <= CONTENT_EPSILON {
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
        layout_bounds.map(|bounds| {
            (
                (bounds.width - resolve_coord(at.x(), bounds.width)).max(0.0),
                (bounds.height - resolve_coord(at.y(), bounds.height)).max(0.0),
            )
        })
    } else {
        None
    };
    let width = resolve_size_value(&size.0[0], max_w, fallback.map(|value| value.0), "width")?;
    let height = resolve_size_value(&size.0[1], max_h, fallback.map(|value| value.1), "height")?;
    Ok((width, height))
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

fn resolve_size_value(
    value: &SizeValue,
    max: Option<f32>,
    fallback: Option<f32>,
    label: &str,
) -> Result<f32, String> {
    match value {
        SizeValue::Dynamic(DynamicValue::Literal(value)) => {
            if *value <= 0.0 {
                return Err(format!("size {label} must be greater than 0"));
            }
            Ok(*value)
        }
        SizeValue::Dynamic(DynamicValue::Ref(_)) => {
            // Unresolved dynamic parameter reference in template load validation
            Ok(0.0)
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
                // Blame whichever of `max`/`fallback` is actually `resolved` (the smaller of the
                // two, mirroring the `.min()` above): a `max_*` that isn't the binding value is
                // fine even if it happens to be set, and a fallback of `0` (the anchor leaving no
                // room) is not a `max_*` authoring error.
                let max_is_binding = match (max, fallback) {
                    (Some(max), Some(fallback)) => max <= fallback,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                return Err(if max_is_binding {
                    format!("max_{label} must be greater than 0")
                } else {
                    format!("no room left for an auto {label} at this anchor")
                });
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
fn resolve_dimension(dimension: &DynamicDimension) -> f32 {
    match dimension {
        DynamicDimension::Fixed(DynamicValue::Literal(value)) => *value,
        DynamicDimension::Fixed(DynamicValue::Ref(_)) => 0.0,
        DynamicDimension::Dynamic { min, max } => match (max, min) {
            (Some(DynamicValue::Literal(v)), _) => *v,
            (_, Some(DynamicValue::Literal(v))) => *v,
            _ => 0.0,
        },
    }
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
    fn rotated_container_rejects_auto_outer_size() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [auto,40]\n    rotate: 90\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn rotated_container_rejects_auto_child() {
        let yaml = "name: A\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 80\n  height: 40\nlayout:\n  - type: container\n    at: [0,0]\n    size: [80,40]\n    rotate: 90\n    items:\n      - type: text\n        value: hi\n        at: [0,0]\n        size: [auto,10]\n        font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
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

    /// §4.2.1: a rotated container's inner canvas has to be known at compile time.
    #[test]
    fn validate_rejects_a_rotated_container_with_a_frame_dependent_to() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    to: [-0.0, 12.0]\n    rotate: 90\n    items: []\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// Both corners edge-relative is a constant 20-unit box, so the canvas is known and it is fine.
    #[test]
    fn validate_accepts_a_rotated_container_whose_corners_both_hug_the_edge() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 25, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [-20.0, 0.0]\n    to: [-0.0, 12.0]\n    rotate: 90\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// A container sized to the right edge is frame-dependent, so its children are dynamic too and an
    /// auto-width child resolves against the container's inner width rather than being rejected.
    #[test]
    fn validate_accepts_an_auto_child_inside_a_to_spanned_container() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 20, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [4.0, 0.0]\n    to: [-0.0, 12.0]\n    items:\n      - type: text\n        value: \"x\"\n        at: [2.0, 1.0]\n        size: [auto, 10.0]\n        font_size: 6\n";
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

    /// Issue #154 repro 1: an unrotated container with auto width whose padding exceeds the resolved width.
    #[test]
    fn unrotated_container_with_auto_width_and_excessive_padding_rejected() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    size: [auto, 24.0]\n    padding: [0.0, 60.0, 0.0, 60.0]\n    items: []\n";
        assert_eq!(
            parse_and_validate(yaml),
            Err("container padding leaves no room for content".to_string())
        );
    }

    /// Issue #154 repro 2: an unrotated container capped by max_w whose padding exceeds the cap.
    #[test]
    fn unrotated_capped_container_with_excessive_padding_rejected() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 24\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    size: [auto, 24.0]\n    max_w: 50.0\n    padding: [0.0, 30.0, 0.0, 30.0]\n    items: []\n";
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

    /// Position and width would both be chasing the same unknown.
    #[test]
    fn validate_rejects_a_right_anchored_auto_width_box_on_a_dynamic_label() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [auto, 10.0]\n    font_size: 6\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// On a fixed frame everything resolves, so the same shape is fine.
    #[test]
    fn validate_accepts_a_right_anchored_auto_width_box_on_a_fixed_label() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 12\nlayout:\n  - type: text\n    value: \"x\"\n    at: [-20.0, 1.0]\n    size: [auto, 10.0]\n    max_w: 20.0\n    font_size: 6\n";
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
            Err("item must fit within layout bounds".to_string())
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
            resolve_size_value(&SizeValue::fixed(50.0), Some(30.0), Some(30.0), "width"),
            Ok(50.0)
        );
    }

    /// A zero (or negative) fallback with no `max_*` set is the anchor leaving no room, not an
    /// invalid `max_*` — there isn't one. Blaming `max_width` here points the author at a field
    /// they never wrote.
    #[test]
    fn a_zero_remainder_with_no_max_blames_the_anchor_not_a_max() {
        use super::resolve_size_value;
        let auto = SizeValue::Auto(crate::models::AutoSize::Auto);
        assert_eq!(
            resolve_size_value(&auto, None, Some(0.0), "width"),
            Err("no room left for an auto width at this anchor".to_string())
        );
    }

    /// A genuinely non-positive `max_*` still gets the original message, whether or not a fallback
    /// happens to be present, as long as `max_*` is the value that actually resolved (the smaller
    /// of the two, or the only one set).
    #[test]
    fn a_non_positive_max_still_blames_the_max() {
        use super::resolve_size_value;
        let auto = SizeValue::Auto(crate::models::AutoSize::Auto);
        assert_eq!(
            resolve_size_value(&auto, Some(-5.0), None, "width"),
            Err("max_width must be greater than 0".to_string())
        );
        // `max_*` is set and positive but is not the binding value (the fallback is smaller and
        // is the actual culprit): still a room problem, not a `max_*` problem, even though a
        // `max_*` happens to be set.
        assert_eq!(
            resolve_size_value(&auto, Some(50.0), Some(0.0), "width"),
            Err("no room left for an auto width at this anchor".to_string())
        );
    }

    /// The fallback is the space remaining from the item's own anchor, not the whole frame. An
    /// `auto` height at a nonzero `at.y` used to resolve to the full frame and get rejected on
    /// bounds, blaming the author for the resolver's arithmetic.
    #[test]
    fn an_auto_axis_falls_back_to_the_space_left_from_its_anchor() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 40\nlayout:\n  - type: container\n    at: [0.0, 10.0]\n    size: [20.0, auto]\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// The anchor is resolved before subtracting: an edge-relative `at.y` is measured from the top,
    /// so `frame - raw_at` would give 45 on a 40mm frame instead of 5.
    #[test]
    fn an_edge_relative_anchor_is_resolved_before_the_subtraction() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 60\n  height: 40\nlayout:\n  - type: container\n    at: [0.0, -5.0]\n    size: [20.0, auto]\n    items: []\n";
        // Resolves to 40 - 35 = 5 and fits. A raw-anchor implementation resolves 45 and is rejected.
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// ADR-0053's double-subtraction, which `extent_auto_bounds` was written to prevent. The
    /// `Extent::To` early return in `resolve_size` is what prevents it now, so test it directly
    /// rather than trusting the control flow.
    #[test]
    fn a_to_extent_is_not_narrowed_twice_by_its_anchor() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [20.0, 0.0]\n    to: [-0.0, 12.0]\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// The fallback's actual value, not merely that something fits. `frame - resolved_at` on each
    /// axis, with the anchor resolved first so an edge-relative component measures from the far edge.
    #[test]
    fn the_auto_fallback_is_the_remaining_space() {
        use super::{resolve_size, LayoutBounds};
        use crate::models::{AutoSize, Extent, Position, Size, SizeValue};
        let bounds = LayoutBounds {
            width: 100.0,
            height: 40.0,
        };
        let auto_h = Extent::Size(Size([
            SizeValue::fixed(20.0),
            SizeValue::Auto(AutoSize::Auto),
        ]));

        // Plain anchor: 40 - 10 = 30.
        let (_, h) = resolve_size(
            &Position([0.0, 10.0]),
            &auto_h,
            None,
            None,
            Some(&bounds),
            true,
        )
        .expect("resolves");
        assert_eq!(h, 30.0);

        // Origin: the subtraction is inert, but the fallback still applies.
        let (_, h) = resolve_size(
            &Position([0.0, 0.0]),
            &auto_h,
            None,
            None,
            Some(&bounds),
            true,
        )
        .expect("resolves");
        assert_eq!(h, 40.0);

        // Edge-relative: at.y -5 resolves to 35, leaving 5. A raw-anchor implementation gives 45.
        let (_, h) = resolve_size(
            &Position([0.0, -5.0]),
            &auto_h,
            None,
            None,
            Some(&bounds),
            true,
        )
        .expect("resolves");
        assert_eq!(h, 5.0);

        // The cap still binds when it is the smaller of the two: min(6, 30).
        let (_, h) = resolve_size(
            &Position([0.0, 10.0]),
            &auto_h,
            None,
            Some(6.0),
            Some(&bounds),
            true,
        )
        .expect("resolves");
        assert_eq!(h, 6.0);

        // And #155's shape: min(200, 40) = 40, not 200.
        let (_, h) = resolve_size(
            &Position([0.0, 0.0]),
            &auto_h,
            None,
            Some(200.0),
            Some(&bounds),
            true,
        )
        .expect("resolves");
        assert_eq!(h, 40.0);

        // Spec §7, the far-edge zero: an anchor exactly on the far edge leaves no room, and the
        // helper's existing `resolved <= 0.0` rejection is the right outcome here. This is the
        // helper path only; the dynamic auto-width container's zero remainder is deliberately
        // allowed elsewhere and is pinned separately by
        // `a_zero_remainder_container_renders_an_empty_box`.
        assert!(
            resolve_size(
                &Position([0.0, 40.0]),
                &auto_h,
                None,
                None,
                Some(&bounds),
                true
            )
            .is_err(),
            "an auto axis with no room left is an authoring error at the helper"
        );

        // Spec §7, the width axis for a container on a fixed label: 100 - 10 = 90.
        let auto_w = Extent::Size(Size([
            SizeValue::Auto(AutoSize::Auto),
            SizeValue::fixed(12.0),
        ]));
        let (w, _) = resolve_size(
            &Position([10.0, 0.0]),
            &auto_w,
            None,
            None,
            Some(&bounds),
            true,
        )
        .expect("resolves");
        assert_eq!(
            w, 90.0,
            "a container at x=10 fills the remaining width, not the whole frame"
        );
    }

    /// The #152 disagreement, from the validation side: a container whose cap exceeds the room left
    /// must resolve to the room left and fit, not to the cap and overflow.
    #[test]
    fn validate_accepts_a_capped_container_that_fits_the_remaining_width() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [90.0, 0.0]\n    size: [auto, 12.0]\n    max_w: 30.0\n    items: []\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// #155's validation half, which **passes before this task** — that is the bug: validation caps
    /// `max_h: 200` against its full-frame fallback while the render path has no fallback at all.
    /// Kept as the pin that the two layers stay agreed, not as a red for this step. The red is
    /// `the_155_repro_renders`.
    #[test]
    fn the_155_repro_validates_and_its_height_is_capped() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 60 }\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 0.0]\n    size: [20.0, auto]\n    max_h: 200.0\n    font_size: 8\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// A fixed label, where text validation previously had no fallback at all.
    #[test]
    fn text_auto_height_on_a_fixed_label_falls_back_to_the_remainder() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 100\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 10.0]\n    size: [20.0, auto]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// The false rejection this closes: it would have rendered at min(35, 30) = 30.
    #[test]
    fn text_auto_height_with_an_oversized_max_h_is_not_rejected_on_a_fixed_label() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 100\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 10.0]\n    size: [20.0, auto]\n    max_h: 35.0\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Sheets render with LengthMode::Fixed and are easy to forget in a change that started on
    /// auto-length tape. An auto width with no max_w previously errored here.
    #[test]
    fn text_auto_width_on_a_sheet_falls_back_to_the_slot_remainder() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: sheet\n  paper_width: 100\n  paper_height: 100\n  label_width: 40\n  label_height: 20\n  positions: [[0.0, 0.0]]\nlayout:\n  - type: text\n    value: \"x\"\n    at: [5.0, 2.0]\n    size: [auto, 8.0]\n    font_size: 6\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// `frame - 0 == frame`, so the anchor subtraction is inert at the origin — but the fallback
    /// itself is new for text on fixed formats, and #155's repro is an origin case. This test
    /// exists because "origin items are unaffected" is the first wrong thing a reader assumes.
    #[test]
    fn the_origin_is_not_exempt() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 100\n  height: 40\nlayout:\n  - type: text\n    value: \"x\"\n    at: [0.0, 0.0]\n    size: [20.0, auto]\n    font_size: 6\n";
        // No max_h, at the origin, on a fixed label: rejected before this branch, resolves to 40 now.
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
    fn dynamic_width_single_auto_width_item_at_offset_validates_ok() {
        // Dynamic-width single with both bounds; a container at at.x=5 with auto width.
        // Auto width should resolve to max_width - at.x = 100 - 5 = 95, which fits.
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
                    Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::fixed(12.0),
                    ]),
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
            .expect("dynamic-width single with auto-width container at offset should validate OK");
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
    size: [auto, auto]
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
    fn reject_reserved_parameter_names() {
        let bad_names = [
            "datetime",
            "vars",
            "datetime.iso",
            "vars.site",
            "invalid.dot",
        ];
        for name in bad_names {
            let yaml = format!(
                "name: T\nunit: mm\ndpi: 200\nparams:\n  {name}:\n    type: string\nformat:\n  type: single\n  height: 12\n  width: 50\nlayout: []"
            );
            let res = parse_and_validate(&yaml);
            assert!(res.is_err(), "should reject reserved name '{name}'");
        }
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
}
