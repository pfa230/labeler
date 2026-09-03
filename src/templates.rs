use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::errors::TemplateError;
use crate::models::{
    resolve_coord, DynamicDimension, DynamicValue, Extent, FlowDirection, FlowOverflow, FontSize,
    InputControl, InputSpec, Layout, LayoutItem, Options, ParamDefaultReport, ParamSpec, ParamType,
    Placement, Point, ResolvedDefaults, Shape, Size, SizeValue, Stroke, TemplateDetail,
    TemplateFormat, TemplateInputs, TemplateSummary,
};
use crate::parse::parse_template;
use crate::resolver;

/// The bare `{token}` names an interpolated string reads: its request fields and parameters.
/// `{vars.*}` and `{sys.*}` resolve without caller input, and a token whose grammar is invalid is
/// left to validation, so neither is an input (ADR-0079).
fn bare_token_names(s: &str) -> Vec<&str> {
    crate::interpolation::scan_tokens(s)
        .into_iter()
        .filter_map(|scanned| match crate::interpolation::parse(scanned.raw) {
            Ok(token) => match token.source {
                crate::interpolation::Source::Bare(name) => Some(name),
                _ => None,
            },
            Err(_) => None,
        })
        .collect()
}

/// The `{vars.<key>}` keys an interpolated string reads.
fn vars_token_keys(s: &str) -> Vec<&str> {
    crate::interpolation::scan_tokens(s)
        .into_iter()
        .filter_map(|scanned| match crate::interpolation::parse(scanned.raw) {
            Ok(token) => match token.source {
                crate::interpolation::Source::Vars(key) => Some(key),
                _ => None,
            },
            Err(_) => None,
        })
        .collect()
}

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

    pub fn variables(&self) -> Vec<String> {
        let mut vars = HashSet::new();
        let Layout::Items(items) = &self.layout;
        fn walk(items: &[LayoutItem], vars: &mut HashSet<String>) {
            for item in items {
                match item {
                    LayoutItem::Text { value, .. } | LayoutItem::Qr { value, .. } => {
                        for key in vars_token_keys(value) {
                            vars.insert(key.to_string());
                        }
                    }
                    LayoutItem::Image { src: Some(src), .. } => {
                        for key in vars_token_keys(src) {
                            vars.insert(key.to_string());
                        }
                    }
                    LayoutItem::Container { items, .. } => {
                        walk(items, vars);
                    }
                    _ => {}
                }
            }
        }
        walk(items, &mut vars);
        let mut res: Vec<String> = vars.into_iter().collect();
        res.sort();
        res
    }

    pub fn inputs_all(&self, resolved_defaults: &ResolvedDefaults) -> Vec<InputSpec> {
        self.derive_inputs_internal(resolved_defaults, None)
    }

    pub fn inputs_default(
        &self,
        resolved_defaults: &ResolvedDefaults,
        variables: &BTreeMap<String, String>,
        datetime: &crate::datetime_fmt::DateTimeResolver,
    ) -> Vec<InputSpec> {
        self.derive_inputs_for_label(resolved_defaults, &HashMap::new(), variables, datetime)
    }

    pub fn derive_inputs_for_label(
        &self,
        resolved_defaults: &ResolvedDefaults,
        data: &HashMap<String, serde_json::Value>,
        variables: &BTreeMap<String, String>,
        datetime: &crate::datetime_fmt::DateTimeResolver,
    ) -> Vec<InputSpec> {
        let resolved = crate::render::resolve_parameters_mode(
            self,
            data,
            Some(variables),
            Some(datetime),
            crate::render::ResolveMode::Lenient,
        )
        .expect("lenient resolution never fails");
        self.derive_inputs_internal(resolved_defaults, Some(&resolved.data))
    }

    pub fn placeholder_data(
        &self,
        resolved_defaults: &ResolvedDefaults,
        now: chrono::DateTime<chrono::Local>,
    ) -> HashMap<String, serde_json::Value> {
        let mut data = HashMap::new();
        for input in self.inputs_all(resolved_defaults) {
            if input.interpolated && input.required {
                let val = match input.control {
                    InputControl::Image => {
                        serde_json::Value::String(crate::render::SAMPLE_PNG_DATA_URI.to_string())
                    }
                    InputControl::Text | InputControl::Textarea => {
                        serde_json::Value::String(input.name.clone())
                    }
                    InputControl::Integer => {
                        let n = input.min.map(|m| m as i64).unwrap_or(1);
                        serde_json::json!(n)
                    }
                    InputControl::Number => {
                        let n = input.min.unwrap_or(1.0);
                        serde_json::json!(n)
                    }
                    InputControl::Checkbox => serde_json::Value::Bool(false),
                    InputControl::Date | InputControl::Datetime => {
                        serde_json::Value::String(now.format("%Y-%m-%dT%H:%M:%S").to_string())
                    }
                    InputControl::List => {
                        serde_json::Value::Array(vec![serde_json::Value::String(
                            input.name.clone(),
                        )])
                    }
                    InputControl::Select => {
                        if input.default_error.is_some() {
                            continue;
                        }
                        let values = input.values.as_ref().unwrap_or_else(|| {
                            panic!("select placeholder for '{}' has no values", input.name)
                        });
                        let first = values.first().unwrap_or_else(|| {
                            panic!("select placeholder for '{}' has empty values", input.name)
                        });
                        serde_json::Value::String(first.clone())
                    }
                };
                data.insert(input.name, val);
            }
        }
        data
    }

    fn derive_inputs_internal(
        &self,
        resolved_defaults: &ResolvedDefaults,
        resolved_data: Option<&HashMap<String, serde_json::Value>>,
    ) -> Vec<InputSpec> {
        let single_line_names = collect_single_line_names(&self.layout);
        let mut collected: HashMap<String, NameInfo> = HashMap::new();

        let mut record_ref = |name: &str, interpolated: bool, image_bound: bool| {
            let entry = collected.entry(name.to_string()).or_default();
            if interpolated {
                entry.interpolated = true;
            }
            if image_bound {
                entry.image_bound = true;
            }
        };

        // 1. format dynamic dimensions
        if let TemplateFormat::Single { width, height, .. } = &self.format {
            for dim in [width, height] {
                match dim {
                    DynamicDimension::Fixed(DynamicValue::Ref(r)) => {
                        record_ref(r, false, false);
                    }
                    DynamicDimension::Dynamic { min, max } => {
                        if let Some(DynamicValue::Ref(r)) = min {
                            record_ref(r, false, false);
                        }
                        if let Some(DynamicValue::Ref(r)) = max {
                            record_ref(r, false, false);
                        }
                    }
                    _ => {}
                }
            }
        }

        // 2. layout items walk
        let Layout::Items(items) = &self.layout;
        fn walk_items<F>(
            items: &[LayoutItem],
            resolved_data: Option<&HashMap<String, serde_json::Value>>,
            repeated_names: &std::collections::BTreeSet<String>,
            record_ref: &mut F,
        ) where
            F: FnMut(&str, bool, bool),
        {
            for item in items {
                // Record when: keys unconditionally for any item encountered in this active scope
                if let Some(when) = item.when() {
                    for key in when.keys() {
                        if !repeated_names.contains(key) {
                            record_ref(key, false, false);
                        }
                    }
                }

                // Check active state
                let is_active = if let Some(data) = resolved_data {
                    if let Some(when) = item.when() {
                        when.iter().all(|(param_name, expected_val)| {
                            data.get(param_name)
                                .map(|v| &crate::render::value_to_string(v) == expected_val)
                                .unwrap_or(false)
                        })
                    } else {
                        true
                    }
                } else {
                    true
                };

                if !is_active {
                    continue;
                }

                // Process active item
                match item {
                    LayoutItem::Text {
                        placement,
                        font_weight,
                        color,
                        value,
                        ..
                    } => {
                        if let Extent::Size(size) = &placement.extent {
                            for sv in &size.0 {
                                if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                    record_ref(r, false, false);
                                }
                            }
                        }
                        if let Some(DynamicValue::Ref(r)) = font_weight {
                            record_ref(r, false, false);
                        }
                        if let Some(DynamicValue::Ref(r)) = color {
                            record_ref(r, false, false);
                        }
                        for name in bare_token_names(value) {
                            if !repeated_names.contains(name) {
                                record_ref(name, true, false);
                            }
                        }
                    }
                    LayoutItem::Qr {
                        placement, value, ..
                    } => {
                        if let Extent::Size(size) = &placement.extent {
                            for sv in &size.0 {
                                if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                    record_ref(r, false, false);
                                }
                            }
                        }
                        for name in bare_token_names(value) {
                            if !repeated_names.contains(name) {
                                record_ref(name, true, false);
                            }
                        }
                    }
                    LayoutItem::Image {
                        placement,
                        name,
                        src,
                        ..
                    } => {
                        if let Extent::Size(size) = &placement.extent {
                            for sv in &size.0 {
                                if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                    record_ref(r, false, false);
                                }
                            }
                        }
                        if let Some(n) = name {
                            record_ref(n, true, true);
                        }
                        if let Some(s) = src {
                            for name in bare_token_names(s) {
                                if !repeated_names.contains(name) {
                                    record_ref(name, true, false);
                                }
                            }
                        }
                    }
                    LayoutItem::Line { stroke, .. } => {
                        if let Some(Stroke {
                            color: DynamicValue::Ref(r),
                            ..
                        }) = stroke
                        {
                            record_ref(r, false, false);
                        }
                    }
                    LayoutItem::Container {
                        placement,
                        stroke,
                        background,
                        repeat,
                        items,
                        ..
                    } => {
                        if let Extent::Size(size) = &placement.extent {
                            for sv in &size.0 {
                                if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                    record_ref(r, false, false);
                                }
                            }
                        }
                        if let Some(Stroke {
                            color: DynamicValue::Ref(r),
                            ..
                        }) = stroke
                        {
                            record_ref(r, false, false);
                        }
                        if let Some(DynamicValue::Ref(r)) = background {
                            record_ref(r, false, false);
                        }

                        if let Some(rep_name) = repeat {
                            record_ref(rep_name, true, false);
                            let mut child_repeated = repeated_names.clone();
                            child_repeated.insert(rep_name.clone());

                            if let Some(data) = resolved_data {
                                if let Some(serde_json::Value::Array(elements)) = data.get(rep_name)
                                {
                                    for elem in elements {
                                        let mut child_data = data.clone();
                                        child_data.insert(
                                            rep_name.clone(),
                                            serde_json::Value::String(
                                                crate::render::value_to_string(elem),
                                            ),
                                        );
                                        walk_items(
                                            items,
                                            Some(&child_data),
                                            &child_repeated,
                                            record_ref,
                                        );
                                    }
                                }
                            } else {
                                walk_items(items, None, &child_repeated, record_ref);
                            }
                        } else {
                            walk_items(items, resolved_data, repeated_names, record_ref);
                        }
                    }
                }
            }
        }

        let initial_repeated = std::collections::BTreeSet::new();
        walk_items(items, resolved_data, &initial_repeated, &mut record_ref);

        let mut specs = Vec::new();

        for (name, info) in collected {
            let truncated_elsewhere = single_line_names.contains(&name);
            let spec = self.params.get(&name).unwrap_or_else(|| {
                panic!("undeclared parameter '{name}' encountered in derive_inputs_internal");
            });

            let control = if info.image_bound {
                InputControl::Image
            } else {
                match &spec.param_type {
                    ParamType::Enum { .. } => InputControl::Select,
                    ParamType::Boolean => InputControl::Checkbox,
                    ParamType::Datetime { time } => {
                        if *time {
                            InputControl::Datetime
                        } else {
                            InputControl::Date
                        }
                    }
                    ParamType::Integer => InputControl::Integer,
                    ParamType::Number | ParamType::Length => InputControl::Number,
                    ParamType::String { multiline } => {
                        if *multiline {
                            InputControl::Textarea
                        } else {
                            InputControl::Text
                        }
                    }
                    ParamType::List => InputControl::List,
                }
            };
            let slider = matches!(
                spec.param_type,
                ParamType::Integer | ParamType::Number | ParamType::Length
            ) && spec.min.is_some()
                && spec.max.is_some();
            let (default, default_error, required) = match resolved_defaults.get(&name) {
                Some(ParamDefaultReport::Resolved { resolved }) => {
                    (Some(resolved.clone()), None, false)
                }
                Some(ParamDefaultReport::Error { error }) => (None, Some(error.clone()), true),
                None => (None, None, true),
            };
            let values = if let ParamType::Enum { values } = &spec.param_type {
                Some(values.clone())
            } else {
                None
            };
            let min = if matches!(
                spec.param_type,
                ParamType::Integer | ParamType::Number | ParamType::Length
            ) {
                spec.min
            } else {
                None
            };
            let max = if matches!(
                spec.param_type,
                ParamType::Integer | ParamType::Number | ParamType::Length
            ) {
                spec.max
            } else {
                None
            };
            let unit = if matches!(spec.param_type, ParamType::Length) {
                Some(self.unit.clone())
            } else {
                None
            };

            specs.push(InputSpec {
                name,
                control,
                slider,
                required,
                default,
                default_error,
                values,
                min,
                max,
                unit,
                description: spec.description.clone(),
                interpolated: info.interpolated,
                truncated_elsewhere,
            });
        }

        specs.sort_by(|a, b| a.name.cmp(&b.name));
        specs
    }
}

#[derive(Default, Debug)]
struct NameInfo {
    interpolated: bool,
    image_bound: bool,
}

fn collect_single_line_names(layout: &Layout) -> HashSet<String> {
    let mut names = HashSet::new();
    let Layout::Items(items) = layout;
    fn walk(items: &[LayoutItem], names: &mut HashSet<String>) {
        for item in items {
            match item {
                LayoutItem::Text { value, wrap, .. } => {
                    if !*wrap {
                        for name in bare_token_names(value) {
                            names.insert(name.to_string());
                        }
                    }
                }
                LayoutItem::Container { items, .. } => {
                    walk(items, names);
                }
                _ => {}
            }
        }
    }
    walk(items, &mut names);
    names
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

    #[cfg(test)]
    pub fn insert_for_tests(
        &mut self,
        id: String,
        group: Option<String>,
        content: TemplateContent,
    ) {
        self.templates
            .insert(id.clone(), TemplateDefinition { id, group, content });
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

    pub fn detail(
        &self,
        id: &str,
        variables: &BTreeMap<String, String>,
        datetime: &crate::datetime_fmt::DateTimeResolver,
    ) -> Option<TemplateDetail> {
        self.templates
            .get(id)
            .map(|t| t.build_detail(variables, datetime))
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
            if let Some(crate::models::ParamValue::String(s)) = &spec.default {
                crate::interpolation::validate_default_syntax(s).map_err(|e| {
                    format!("invalid interpolation syntax in default of parameter '{name}': {e}")
                })?;
                for scanned in crate::interpolation::scan_tokens(s) {
                    if let Ok(tok) = crate::interpolation::parse(scanned.raw) {
                        if matches!(tok.source, crate::interpolation::Source::Bare(_)) {
                            return Err(format!(
                                "bare token '{}' is not allowed in a default; only namespaced tokens ({{vars.…}}, {{sys.…}}) are supported",
                                scanned.raw
                            ));
                        }
                    }
                }
                let empty_repeated = std::collections::BTreeSet::new();
                validate_interpolated_string(s, &self.params, &empty_repeated)?;
            }
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
                let repeated_names = std::collections::BTreeSet::new();
                for (idx, item) in items.iter().enumerate() {
                    validate_item_references(
                        item,
                        &self.params,
                        &format!("layout[{idx}]"),
                        &repeated_names,
                    )?;
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
        validate_circle_containers(&self.layout, &geometry_values)?;

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
        ParamType::Datetime { .. } => {
            if let Some(default) = &spec.default {
                if !matches!(default, crate::models::ParamValue::String(_)) {
                    return Err(format!(
                        "default on a datetime parameter '{name}' must be a string"
                    ));
                }
            }
        }
        ParamType::Enum { values } => {
            if values.is_empty() {
                return Err(format!("parameter '{name}' enum values must not be empty"));
            }
            if values.iter().any(|opt| opt.trim().is_empty()) {
                return Err("options must not contain empty values".to_string());
            }
            if let Some(default) = &spec.default {
                match default {
                    crate::models::ParamValue::String(s) => {
                        if !s.contains('{') && !s.contains('}') && !values.iter().any(|v| v == s) {
                            return Err(format!(
                                "parameter '{name}' default '{s}' is not in enum values"
                            ));
                        }
                    }
                    crate::models::ParamValue::Integer(i) => {
                        let s = i.to_string();
                        if !values.contains(&s) {
                            return Err(format!(
                                "parameter '{name}' default '{i}' is not in enum values"
                            ));
                        }
                    }
                    _ => {
                        return Err(format!(
                            "parameter '{name}' default must be one of the declared enum values"
                        ));
                    }
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
        ParamType::List => allowed_types.contains(&"list"),
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
    path: &str,
    repeated_names: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if let Some(when) = when {
        for (name, val) in when {
            let spec = params.get(name).ok_or_else(|| {
                format!("undeclared parameter '{name}' referenced in when condition")
            })?;
            if matches!(spec.param_type, ParamType::List) && !repeated_names.contains(name) {
                return Err(format!(
                    "when condition at {path} references list parameter '{name}'; list parameters cannot be used in when conditions"
                ));
            }
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
    repeated_names: &std::collections::BTreeSet<String>,
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
        if let crate::interpolation::Source::Bare(name) = &token.source {
            if !params.contains_key(*name) {
                return Err(format!(
                    "template contains '{}': undeclared parameter '{name}'",
                    scanned.raw
                ));
            }
        }

        match token.reader {
            Some(crate::interpolation::Reader::Join(_)) => {
                let is_declared_list = match token.source {
                    crate::interpolation::Source::Bare(name) => {
                        params.get(name).is_some_and(|spec| {
                            matches!(spec.param_type, crate::models::ParamType::List)
                        })
                    }
                    _ => false,
                };
                if !is_declared_list {
                    return Err(format!(
                        "template contains '{}': join can only be applied to a parameter declared as type: list",
                        scanned.raw
                    ));
                }
            }
            Some(crate::interpolation::Reader::Format(fmt)) => {
                let is_declared_list = match token.source {
                    crate::interpolation::Source::Bare(name) => {
                        params.get(name).is_some_and(|spec| {
                            matches!(spec.param_type, crate::models::ParamType::List)
                        })
                    }
                    _ => false,
                };
                if is_declared_list {
                    return Err(format!(
                        "template contains '{}': a list parameter is read through join('<separator>')",
                        scanned.raw
                    ));
                }
                let is_instant = match token.source {
                    crate::interpolation::Source::Sys(crate::interpolation::SysValue::Now) => true,
                    crate::interpolation::Source::Bare(name) => {
                        params.get(name).is_some_and(|spec| {
                            matches!(spec.param_type, crate::models::ParamType::Datetime { .. })
                        })
                    }
                    _ => false,
                };
                if !is_instant {
                    return Err(format!(
                        "template contains '{}': format '{}' can only be applied to an instant (sys.now or type: datetime parameter)",
                        scanned.raw, fmt
                    ));
                }
            }
            None => {
                let is_declared_list = match token.source {
                    crate::interpolation::Source::Bare(name) => {
                        if repeated_names.contains(name) {
                            false
                        } else {
                            params.get(name).is_some_and(|spec| {
                                matches!(spec.param_type, crate::models::ParamType::List)
                            })
                        }
                    }
                    _ => false,
                };
                if is_declared_list {
                    return Err(format!(
                        "template contains '{}': list parameter cannot be used as a bare token; a list is read through join('<separator>')",
                        scanned.raw
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
    path: &str,
    repeated_names: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    match item {
        LayoutItem::Text {
            value,
            placement,
            font_weight,
            color,
            when,
            ..
        } => {
            validate_when_references(when.as_ref(), params, path, repeated_names)?;
            validate_interpolated_string(value, params, repeated_names)?;
            if let Some(DynamicValue::Ref(ref_name)) = font_weight {
                check_param_ref(params, ref_name, "font_weight", &["integer"])?;
            }
            if let Some(DynamicValue::Ref(ref_name)) = color {
                check_param_ref(params, ref_name, "color", &["string", "enum"])?;
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
            validate_when_references(when.as_ref(), params, path, repeated_names)?;
            validate_interpolated_string(value, params, repeated_names)?;
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
            validate_when_references(when.as_ref(), params, path, repeated_names)?;
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
                check_param_ref(params, n, "image name", &["string"])?;
            }
            if let Some(s) = src {
                validate_interpolated_string(s, params, repeated_names)?;
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
        LayoutItem::Line { stroke, when, .. } => {
            validate_when_references(when.as_ref(), params, path, repeated_names)?;
            if let Some(Stroke {
                color: DynamicValue::Ref(ref_name),
                ..
            }) = stroke
            {
                check_param_ref(params, ref_name, "stroke.color", &["string", "enum"])?;
            }
        }
        LayoutItem::Container {
            placement,
            when,
            stroke,
            background,
            repeat,
            items,
            ..
        } => {
            validate_when_references(when.as_ref(), params, path, repeated_names)?;
            if let Some(Stroke {
                color: DynamicValue::Ref(ref_name),
                ..
            }) = stroke
            {
                check_param_ref(params, ref_name, "stroke.color", &["string", "enum"])?;
            }
            if let Some(DynamicValue::Ref(ref_name)) = background {
                check_param_ref(params, ref_name, "background", &["string", "enum"])?;
            }
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
            let mut child_repeated = repeated_names.clone();
            if let Some(r) = repeat {
                child_repeated.insert(r.clone());
            }
            for (child_idx, child) in items.iter().enumerate() {
                validate_item_references(
                    child,
                    params,
                    &format!("{path}.items[{child_idx}]"),
                    &child_repeated,
                )?;
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
            color,
            wrap,
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
                color: color.clone(),
                wrap: *wrap,
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
            stroke,
            when,
        } => LayoutItem::Line {
            at: at.clone(),
            to: to.clone(),
            stroke: stroke.clone(),
            when: when.clone(),
        },
        LayoutItem::Container {
            placement,
            when,
            shape,
            stroke,
            background,
            rounded,
            padding,
            flow,
            repeat,
            items,
        } => LayoutItem::Container {
            placement: inst_placement(placement),
            when: when.clone(),
            shape: *shape,
            stroke: stroke.clone(),
            background: background.clone(),
            rounded: *rounded,
            padding: *padding,
            flow: flow.clone(),
            repeat: repeat.clone(),
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

fn validate_circle_containers(
    layout: &Layout,
    geometry_values: &HashMap<String, f32>,
) -> Result<(), String> {
    let Layout::Items(items) = layout;
    for item in items {
        validate_circle_item(item, geometry_values)?;
    }
    Ok(())
}

fn validate_circle_item(
    item: &LayoutItem,
    geometry_values: &HashMap<String, f32>,
) -> Result<(), String> {
    if let LayoutItem::Container {
        placement,
        shape,
        items,
        ..
    } = item
    {
        let spec_0 = resolver::source_of(placement, 0, geometry_values);
        let spec_1 = resolver::source_of(placement, 1, geometry_values);
        if matches!(shape, Shape::Circle) && spec_0.fixed_by_template && spec_1.fixed_by_template {
            let w = resolver::resolve(&spec_0, 0.0, 0.0, placement.max_w, None);
            let h = resolver::resolve(&spec_1, 0.0, 0.0, placement.max_h, None);
            if (w - h).abs() > resolver::BOUNDS_EPSILON {
                return Err("circle container size must be square".to_string());
            }
        }
        for child in items {
            validate_circle_item(child, geometry_values)?;
        }
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
            stroke,
            when,
        } => {
            validate_when(when.as_ref())?;
            const LINE_EPSILON: f32 = 1.0e-4;
            if let Some(stroke) = stroke {
                if !stroke.thickness.is_finite() || stroke.thickness < 0.0001 {
                    return Err("stroke thickness must be finite and >= 0.0001".to_string());
                }
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
            shape: _,
            stroke,
            background: _,
            rounded,
            padding,
            flow,
            repeat: _,
            items,
        } => {
            validate_when(when.as_ref())?;
            if let Some(s) = stroke {
                if !s.thickness.is_finite() || s.thickness < 0.0001 {
                    return Err("stroke thickness must be finite and >= 0.0001".to_string());
                }
            }
            if let Some(r) = rounded {
                if !r.is_finite() || *r < 0.0001 {
                    return Err("rounded radius must be finite and >= 0.0001".to_string());
                }
            }
            if let Some(fl) = flow {
                if !fl.gap.is_finite() || fl.gap < 0.0 {
                    return Err("flow gap must be >= 0".to_string());
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
            if let Some(flow) = flow {
                let primary_axis = match flow.direction {
                    FlowDirection::Row => 0,
                    FlowDirection::Column => 1,
                };
                if flow.wrap && !child_axes_resolved[primary_axis] {
                    return Err("flow wrap requires a resolved primary axis".to_string());
                }
                if matches!(flow.overflow, FlowOverflow::Trim)
                    && child_axes_resolved.iter().any(|resolved| !resolved)
                {
                    return Err("flow overflow trim requires both axes to be resolved".to_string());
                }
            }

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
        Some(frame) => {
            if placement.at.is_none() {
                resolver::resolve_packed(placement, frame, geometry_values, [None, None])
                    .map(|_| ())
                    .map_err(violation_message)
            } else {
                resolver::place(placement, frame, geometry_values, [None, None])
                    .map(|_| ())
                    .map_err(violation_message)
            }
        }
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

impl TemplateDefinition {
    pub fn build_detail(
        &self,
        variables: &BTreeMap<String, String>,
        datetime: &crate::datetime_fmt::DateTimeResolver,
    ) -> TemplateDetail {
        let param_defaults = crate::render::resolve_declared_defaults(self, variables, datetime);
        let default_inputs = self.inputs_default(&param_defaults, variables, datetime);
        let all_inputs = self.inputs_all(&param_defaults);
        TemplateDetail {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            group: self.group.clone(),
            unit: self.unit.clone(),
            dpi: self.dpi,
            format: self.format.clone(),
            params: self.params.clone(),
            layout: self.layout.clone(),
            version: self.version.clone(),
            inputs: TemplateInputs {
                default: default_inputs,
                all: all_inputs,
            },
            variables: self.variables(),
            param_defaults,
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
        bare_token_names, list_template_groups, load_all_for_tests, validate_group_name,
        validate_group_segment, validate_template_id_stem, TemplateContent, TemplateRegistry,
    };
    use crate::models::{
        Alignment, Color, Dimension, DynamicDimension, DynamicValue, Extent, FontSize,
        InputControl, InputSpec, Layout, LayoutItem, ParamSpec, ParamType, ParamValue, Position,
        Shape, Size, SizeValue, Stroke, TemplateFormat,
    };
    use crate::reason::Reason;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap, HashSet};
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

    fn parse_template_ok(yaml: &str) -> TemplateContent {
        let t = crate::parse::parse_template(yaml).expect("parse template");
        t.validate().expect("validate template");
        t
    }

    fn test_defaults(template: &TemplateContent) -> super::ResolvedDefaults {
        let variables = BTreeMap::new();
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let now = chrono::Local::now();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        crate::render::resolve_declared_defaults(template, &variables, &dt)
    }

    fn test_inputs_all(template: &TemplateContent) -> Vec<InputSpec> {
        let defaults = test_defaults(template);
        template.inputs_all(&defaults)
    }

    fn test_inputs_default(template: &TemplateContent) -> Vec<InputSpec> {
        let variables = BTreeMap::new();
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let now = chrono::Local::now();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let defaults = crate::render::resolve_declared_defaults(template, &variables, &dt);
        template.inputs_default(&defaults, &variables, &dt)
    }

    fn test_derive_inputs_for_label(
        template: &TemplateContent,
        data: &HashMap<String, serde_json::Value>,
    ) -> Vec<InputSpec> {
        let variables = BTreeMap::new();
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let now = chrono::Local::now();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let defaults = crate::render::resolve_declared_defaults(template, &variables, &dt);
        template.derive_inputs_for_label(&defaults, data, &variables, &dt)
    }

    fn test_placeholder_data(
        template: &TemplateContent,
        now: chrono::DateTime<chrono::Local>,
    ) -> HashMap<String, serde_json::Value> {
        let variables = BTreeMap::new();
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let defaults = crate::render::resolve_declared_defaults(template, &variables, &dt);
        template.placeholder_data(&defaults, now)
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
    fn shape_paint_validation_boundaries() {
        // Line thickness 0.0001 accepted
        let yaml_ok = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0.0001\n";
        assert!(parse_and_validate(yaml_ok).is_ok());

        // Line thickness 0.00001 rejected
        let yaml_err = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0.00001\n";
        assert!(parse_and_validate(yaml_err).is_err());

        // Line thickness 0 rejected
        let yaml_zero = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0\n";
        assert!(parse_and_validate(yaml_zero).is_err());

        // Line thickness negative rejected
        let yaml_neg = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: -1\n";
        assert!(parse_and_validate(yaml_neg).is_err());

        // Line thickness nan rejected
        let yaml_line_nan = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: .nan\n";
        assert!(parse_and_validate(yaml_line_nan).is_err());

        // Line thickness inf rejected
        let yaml_line_inf = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: .inf\n";
        assert!(parse_and_validate(yaml_line_inf).is_err());

        // Line background rejected
        let yaml_line_bg = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0.2\n    background: red\n";
        assert!(parse_and_validate(yaml_line_bg).is_err());

        // Line rounded rejected
        let yaml_line_rnd = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0.2\n    rounded: 1.0\n";
        assert!(parse_and_validate(yaml_line_rnd).is_err());

        // Container stroke thickness 0.0001 accepted
        let yaml_cont_ok = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 0.0001\n    items: []\n";
        assert!(parse_and_validate(yaml_cont_ok).is_ok());

        // Container stroke thickness 0.00001 rejected
        let yaml_cont_err = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 0.00001\n    items: []\n";
        assert!(parse_and_validate(yaml_cont_err).is_err());

        // Container stroke thickness nan rejected
        let yaml_cont_nan = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: .nan\n    items: []\n";
        assert!(parse_and_validate(yaml_cont_nan).is_err());

        // Container stroke thickness inf rejected
        let yaml_cont_inf = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: .inf\n    items: []\n";
        assert!(parse_and_validate(yaml_cont_inf).is_err());

        // Container rounded 0.0001 accepted
        let yaml_rnd_ok = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    rounded: 0.0001\n    items: []\n";
        assert!(parse_and_validate(yaml_rnd_ok).is_ok());

        // Container rounded 0.00001 rejected
        let yaml_rnd_err = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    rounded: 0.00001\n    items: []\n";
        assert!(parse_and_validate(yaml_rnd_err).is_err());

        // Container rounded nan rejected
        let yaml_rnd_nan = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    rounded: .nan\n    items: []\n";
        assert!(parse_and_validate(yaml_rnd_nan).is_err());

        // Container rounded inf rejected
        let yaml_rnd_inf = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    rounded: .inf\n    items: []\n";
        assert!(parse_and_validate(yaml_rnd_inf).is_err());

        // Unknown keys inside stroke rejected
        let yaml_stroke_unknown = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 1.0\n      width: 2.0\n    items: []\n";
        assert!(parse_and_validate(yaml_stroke_unknown).is_err());

        // Shape attributes rejected on text
        let yaml_text_stroke = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n    stroke:\n      thickness: 1.0\n";
        assert!(parse_and_validate(yaml_text_stroke).is_err());

        let yaml_text_bg = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n    background: red\n";
        assert!(parse_and_validate(yaml_text_bg).is_err());

        let yaml_text_rnd = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: text\n    value: hi\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n    rounded: 1.0\n";
        assert!(parse_and_validate(yaml_text_rnd).is_err());

        // Shape attributes rejected on qr
        let yaml_qr_stroke = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: qr\n    value: hi\n    at: [0,0]\n    size: [10,10]\n    stroke:\n      thickness: 1.0\n";
        assert!(parse_and_validate(yaml_qr_stroke).is_err());

        let yaml_qr_bg = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: qr\n    value: hi\n    at: [0,0]\n    size: [10,10]\n    background: red\n";
        assert!(parse_and_validate(yaml_qr_bg).is_err());

        let yaml_qr_rnd = "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: qr\n    value: hi\n    at: [0,0]\n    size: [10,10]\n    rounded: 1.0\n";
        assert!(parse_and_validate(yaml_qr_rnd).is_err());

        // Shape attributes rejected on image
        let yaml_img_stroke = "name: T\nunit: mm\ndpi: 200\nparams:\n  logo:\n    type: string\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: image\n    name: logo\n    at: [0,0]\n    size: [10,10]\n    stroke:\n      thickness: 1.0\n";
        assert!(parse_and_validate(yaml_img_stroke).is_err());

        let yaml_img_bg = "name: T\nunit: mm\ndpi: 200\nparams:\n  logo:\n    type: string\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: image\n    name: logo\n    at: [0,0]\n    size: [10,10]\n    background: red\n";
        assert!(parse_and_validate(yaml_img_bg).is_err());

        let yaml_img_rnd = "name: T\nunit: mm\ndpi: 200\nparams:\n  logo:\n    type: string\nformat:\n  type: single\n  width: 20\n  height: 20\nlayout:\n  - type: image\n    name: logo\n    at: [0,0]\n    size: [10,10]\n    rounded: 1.0\n";
        assert!(parse_and_validate(yaml_img_rnd).is_err());
    }

    #[test]
    fn shape_paint_direct_model_validation_boundaries() {
        let base_template = |layout: Vec<LayoutItem>| TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(50.0).into(),
                height: Dimension::Fixed(30.0).into(),
                media_width: None,
            },
            params: BTreeMap::new(),
            layout: Layout::Items(layout),
            version: None,
        };

        // 1. Line stroke validation on model directly
        let line_with_stroke = |thickness: f32| {
            base_template(vec![LayoutItem::Line {
                at: Position([0.0, 0.0]),
                to: Position([10.0, 10.0]),
                stroke: Some(Stroke {
                    thickness,
                    color: DynamicValue::Literal(Color::black()),
                }),
                when: None,
            }])
        };

        assert!(line_with_stroke(0.0001).validate().is_ok());
        assert!(line_with_stroke(0.00001).validate().is_err());
        assert!(line_with_stroke(0.0).validate().is_err());
        assert!(line_with_stroke(-1.0).validate().is_err());
        assert!(line_with_stroke(f32::NAN).validate().is_err());
        assert!(line_with_stroke(f32::INFINITY).validate().is_err());

        // Line with no stroke is valid
        let line_no_stroke = base_template(vec![LayoutItem::Line {
            at: Position([0.0, 0.0]),
            to: Position([10.0, 10.0]),
            stroke: None,
            when: None,
        }]);
        assert!(line_no_stroke.validate().is_ok());

        // 2. Container stroke validation on model directly
        let container_with_stroke = |thickness: f32| {
            base_template(vec![LayoutItem::Container {
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                ),
                when: None,
                shape: Shape::Rect,
                stroke: Some(Stroke {
                    thickness,
                    color: DynamicValue::Literal(Color::black()),
                }),
                background: None,
                rounded: None,
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
                items: vec![],
            }])
        };

        assert!(container_with_stroke(0.0001).validate().is_ok());
        assert!(container_with_stroke(0.00001).validate().is_err());
        assert!(container_with_stroke(0.0).validate().is_err());
        assert!(container_with_stroke(-1.0).validate().is_err());
        assert!(container_with_stroke(f32::NAN).validate().is_err());
        assert!(container_with_stroke(f32::INFINITY).validate().is_err());

        // 3. Container rounded validation on model directly
        let container_with_rounded = |radius: f32| {
            base_template(vec![LayoutItem::Container {
                placement: crate::models::Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                ),
                when: None,
                shape: Shape::Rect,
                stroke: None,
                background: None,
                rounded: Some(radius),
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
                items: vec![],
            }])
        };

        assert!(container_with_rounded(0.0001).validate().is_ok());
        assert!(container_with_rounded(0.00001).validate().is_err());
        assert!(container_with_rounded(0.0).validate().is_err());
        assert!(container_with_rounded(-1.0).validate().is_err());
        assert!(container_with_rounded(f32::NAN).validate().is_err());
        assert!(container_with_rounded(f32::INFINITY).validate().is_err());
    }

    #[test]
    fn superseded_shape_spellings_are_quarantined_at_registry_load() {
        let dir = temp_dir("superseded_shape_spellings");

        // 1. Valid template that must be loaded and served
        let valid_yaml = r#"
name: Valid Label
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    stroke:
      thickness: 0.2
    items: []
"#;
        write_template(&dir, "valid_label.yaml", valid_yaml);

        // 2. Container with legacy frame block
        let frame_yaml = r#"
name: Legacy Frame
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    frame:
      thickness: 0.02
      rounded: false
    items: []
"#;
        write_template(&dir, "legacy_frame.yaml", frame_yaml);

        // 3. Line with bare thickness
        let line_thickness_yaml = r#"
name: Bare Line Thickness
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: line
    at: [0, 0]
    to: [10, 10]
    thickness: 0.2
"#;
        write_template(&dir, "bare_line_thickness.yaml", line_thickness_yaml);

        // 4. Container with boolean rounded: true
        let rounded_true_yaml = r#"
name: Boolean Rounded True
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    rounded: true
    items: []
"#;
        write_template(&dir, "rounded_true.yaml", rounded_true_yaml);

        // 5. Container with boolean rounded: false
        let rounded_false_yaml = r#"
name: Boolean Rounded False
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    rounded: false
    items: []
"#;
        write_template(&dir, "rounded_false.yaml", rounded_false_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).expect("registry load must not fail");

        // The valid template must be served
        assert_eq!(registry.len(), 1, "valid template must be served");
        assert!(registry.get("valid_label").is_some());

        // The four broken templates must be quarantined
        let broken = registry.broken();
        assert_eq!(
            broken.len(),
            4,
            "four superseded templates must be quarantined"
        );

        let find_broken = |filename: &str| {
            broken
                .iter()
                .find(|b| b.path == filename)
                .unwrap_or_else(|| panic!("missing broken template {filename}"))
        };

        let frame_broken = find_broken("legacy_frame.yaml");
        assert!(
            frame_broken.error.contains("frame"),
            "expected 'frame' in error: {}",
            frame_broken.error
        );

        let line_broken = find_broken("bare_line_thickness.yaml");
        assert!(
            line_broken.error.contains("thickness"),
            "expected 'thickness' in error: {}",
            line_broken.error
        );

        let rnd_true_broken = find_broken("rounded_true.yaml");
        assert!(
            rnd_true_broken.error.contains("rounded"),
            "expected 'rounded' in error: {}",
            rnd_true_broken.error
        );

        let rnd_false_broken = find_broken("rounded_false.yaml");
        assert!(
            rnd_false_broken.error.contains("rounded"),
            "expected 'rounded' in error: {}",
            rnd_false_broken.error
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn superseded_options_spelling_is_quarantined_at_registry_load() {
        let dir = temp_dir("superseded_options_spelling");

        let valid_yaml = r#"
name: Valid Label
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    stroke:
      thickness: 0.2
    items: []
"#;
        write_template(&dir, "valid_label.yaml", valid_yaml);

        let options_yaml = r#"
name: Has Options
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
options:
  orientation: [vertical, horizontal]
layout:
  - type: text
    value: "hello"
    at: [0, 0]
    size: [10, 5]
    font_size: 8
"#;
        write_template(&dir, "has_options.yaml", options_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).expect("registry load must not fail");

        assert_eq!(registry.len(), 1, "valid template must be served");
        assert!(registry.get("valid_label").is_some());

        let broken = registry.broken();
        assert_eq!(broken.len(), 1, "options template must be quarantined");
        let broken_entry = broken
            .iter()
            .find(|b| b.path == "has_options.yaml")
            .expect("missing broken template has_options.yaml");
        assert!(
            broken_entry.error.contains("unknown field `options`"),
            "expected 'unknown field `options`' in error: {}",
            broken_entry.error
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn superseded_container_option_spelling_is_quarantined_at_registry_load() {
        let dir = temp_dir("superseded_container_option_spelling");

        let valid_yaml = r#"
name: Valid Label
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    stroke:
      thickness: 0.2
    items: []
"#;
        write_template(&dir, "valid_label.yaml", valid_yaml);

        let option_yaml = r#"
name: Has Container Option
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [vertical, horizontal]
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    option:
      orientation: vertical
    items: []
"#;
        write_template(&dir, "has_container_option.yaml", option_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).expect("registry load must not fail");

        assert_eq!(registry.len(), 1, "valid template must be served");
        assert!(registry.get("valid_label").is_some());

        let broken = registry.broken();
        assert_eq!(
            broken.len(),
            1,
            "container option template must be quarantined"
        );
        let broken_entry = broken
            .iter()
            .find(|b| b.path == "has_container_option.yaml")
            .expect("missing broken template has_container_option.yaml");
        assert!(
            broken_entry.error.contains("layout[0]")
                && broken_entry.error.contains("unknown field `option`"),
            "expected layout[0] and 'unknown field `option`' in error: {}",
            broken_entry.error
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn circle_container_load_time_squareness_and_quarantine() {
        let dir = temp_dir("circle_container_load");

        // 1. Non-square fixed size [14, 12] quarantines naming size
        let non_square_yaml = r#"
name: NonSquareCircle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: [14, 12]
    items: []
"#;
        write_template(&dir, "non_square_circle.yaml", non_square_yaml);

        // 2. size: [content, content] loads without quarantine
        let content_circle_yaml = r#"
name: ContentCircle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: [content, content]
    items: []
"#;
        write_template(&dir, "content_circle.yaml", content_circle_yaml);

        // 3. size: ["{w}", 12] loads without quarantine
        let param_circle_yaml = r#"
name: ParamCircle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
params:
  w:
    type: length
    default: 14
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: ["{w}", 12]
    items: []
"#;
        write_template(&dir, "param_circle.yaml", param_circle_yaml);

        // 4. Shrinking to loads without quarantine
        let shrinking_circle_yaml = r#"
name: ShrinkingCircle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
layout:
  - type: container
    at: [-20, 0]
    shape: circle
    to: [40, 10]
    items: []
"#;
        write_template(&dir, "shrinking_circle.yaml", shrinking_circle_yaml);

        // 5. Circle fixed by template with non-square size is refused even with false when
        let when_circle_yaml = r#"
name: WhenCircle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
params:
  badge:
    type: enum
    values: [yes, no]
    default: no
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: [14, 12]
    when: { badge: yes }
    items: []
"#;
        write_template(&dir, "when_circle.yaml", when_circle_yaml);

        // 6. at: [0.2, 0.0] with to: [0.3, 0.1] loads (diff < 0.0001)
        let tolerance_ok_yaml = r#"
name: ToleranceOkCircle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
layout:
  - type: container
    at: [0.2, 0.0]
    shape: circle
    to: [0.3, 0.1]
    items: []
"#;
        write_template(&dir, "tolerance_ok_circle.yaml", tolerance_ok_yaml);

        // 7. Difference of 0.001 is refused at load
        let diff_001_yaml = r#"
name: Diff001Circle
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
layout:
  - type: container
    at: [0.2, 0.0]
    shape: circle
    to: [0.301, 0.1]
    items: []
"#;
        write_template(&dir, "diff_001_circle.yaml", diff_001_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).unwrap();

        // 4 templates must load successfully
        assert!(registry.get("content_circle").is_some());
        assert!(registry.get("param_circle").is_some());
        assert!(registry.get("shrinking_circle").is_some());
        assert!(registry.get("tolerance_ok_circle").is_some());

        // 3 templates must be quarantined
        let broken = registry.broken();
        assert_eq!(broken.len(), 3);

        let find_broken = |filename: &str| {
            broken
                .iter()
                .find(|b| b.path == filename)
                .unwrap_or_else(|| panic!("missing broken template {filename}"))
        };

        let non_square_broken = find_broken("non_square_circle.yaml");
        assert!(
            non_square_broken.error.contains("must be square"),
            "expected 'must be square' in error: {}",
            non_square_broken.error
        );

        let when_broken = find_broken("when_circle.yaml");
        assert!(
            when_broken.error.contains("must be square"),
            "expected 'must be square' in error: {}",
            when_broken.error
        );

        let diff_broken = find_broken("diff_001_circle.yaml");
        assert!(
            diff_broken.error.contains("must be square"),
            "expected 'must be square' in error: {}",
            diff_broken.error
        );

        fs::remove_dir_all(&dir).ok();
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
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 30 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [40.0, 6.0]\n    stroke:\n      thickness: 0.2\n";
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
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [-30.0, 6.0]\n    to: [-0.0, 6.0]\n    stroke:\n      thickness: 0.2\n";
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
params:
  message:
    type: string
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
    wrap: true
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
                color: None,
                wrap: false,
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
            params: BTreeMap::from([
                (
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
                ),
                (
                    "logo".to_string(),
                    ParamSpec {
                        param_type: ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                ),
            ]),
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
                color: None,
                wrap: false,
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
                stroke: Some(Stroke {
                    thickness: 0.2,
                    color: DynamicValue::Literal(Color::black()),
                }),
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
                stroke: Some(Stroke {
                    thickness: 0.2,
                    color: DynamicValue::Literal(Color::black()),
                }),
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
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [-0.0, 6.0]\n    stroke:\n      thickness: 0.2\n";
        assert_eq!(parse_and_validate(yaml), Ok(()));
    }

    /// Still degenerate after resolution: both endpoints land on the right edge.
    #[test]
    fn validate_rejects_a_line_degenerate_after_resolution() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: 40\n  height: 12\nlayout:\n  - type: line\n    at: [-0.0, 6.0]\n    to: [-0.0, 6.0]\n    stroke:\n      thickness: 0.2\n";
        assert!(parse_and_validate(yaml).is_err());
    }

    /// An inset larger than the widest the label can ever be never resolves to a valid coordinate.
    #[test]
    fn validate_rejects_a_line_inset_larger_than_the_max_width() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: line\n    at: [0.0, 6.0]\n    to: [-140.0, 6.0]\n    stroke:\n      thickness: 0.2\n";
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
                color: None,
                wrap: false,
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
                shape: Shape::Rect,
                stroke: None,
                background: None,
                rounded: None,
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
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
                color: None,
                wrap: true,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with wrap: true should validate OK");
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
                color: None,
                wrap: false,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        template
            .validate()
            .expect("dynamic-width single with wrap: false should validate OK");
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
                color: None,
                wrap: true,
                alignment: Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        template
            .validate()
            .expect("fixed-width single with wrap: true should validate OK");
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

        let yaml_float = r#"
name: T
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: 1.5
format:
  type: single
  height: 18
  width: 50
layout: []
"#;
        let res_float = parse_and_validate(yaml_float);
        assert!(res_float.is_err(), "should reject float default on enum");
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
        if let Err(err) = std::fs::write(&path, sample_yaml("Non UTF8")) {
            // APFS refuses the name with EILSEQ on every volume it offers, case-sensitive
            // included, so the state under test cannot be constructed and there is no
            // macOS behaviour to assert.
            if err.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error()) {
                eprintln!(
                    "skipping load_from_dir_handles_non_utf8_paths: filesystem does not support non-UTF-8 filename (capability: non_utf8_paths, errno: {} EILSEQ)",
                    err.raw_os_error().unwrap()
                );
                fs::remove_dir_all(&dir).ok();
                return;
            }
            panic!("unexpected error creating non-UTF-8 file: {err}");
        }

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
        stroke:
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

    #[test]
    fn reference_site_guard_all_validation_params_appear_in_inputs_all() {
        let registry = load_all_for_tests().0;
        for summary in registry.summaries() {
            let template = registry.get(&summary.id).expect("template");
            let inputs_all_names: HashSet<String> = test_inputs_all(template)
                .into_iter()
                .map(|i| i.name)
                .collect();

            // Check format refs
            if let TemplateFormat::Single { width, height, .. } = &template.format {
                for dim in [width, height] {
                    match dim {
                        DynamicDimension::Fixed(DynamicValue::Ref(r)) => {
                            assert!(
                                inputs_all_names.contains(r),
                                "template {} missing format ref {r} in inputs.all",
                                template.id
                            );
                        }
                        DynamicDimension::Dynamic { min, max } => {
                            if let Some(DynamicValue::Ref(r)) = min {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {} missing format min ref {r} in inputs.all",
                                    template.id
                                );
                            }
                            if let Some(DynamicValue::Ref(r)) = max {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {} missing format max ref {r} in inputs.all",
                                    template.id
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Check layout items refs and when keys
            fn check_items(
                items: &[LayoutItem],
                inputs_all_names: &HashSet<String>,
                template_id: &str,
            ) {
                for item in items {
                    if let Some(when) = item.when() {
                        for k in when.keys() {
                            assert!(
                                inputs_all_names.contains(k),
                                "template {template_id} missing when key {k} in inputs.all"
                            );
                        }
                    }
                    match item {
                        LayoutItem::Text {
                            placement,
                            font_weight,
                            color,
                            value,
                            ..
                        } => {
                            if let Extent::Size(size) = &placement.extent {
                                for sv in &size.0 {
                                    if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                        assert!(
                                            inputs_all_names.contains(r),
                                            "template {template_id} missing text size ref {r} in inputs.all"
                                        );
                                    }
                                }
                            }
                            if let Some(DynamicValue::Ref(r)) = font_weight {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {template_id} missing font_weight ref {r} in inputs.all"
                                );
                            }
                            if let Some(DynamicValue::Ref(r)) = color {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {template_id} missing text color ref {r} in inputs.all"
                                );
                            }
                            for name in bare_token_names(value) {
                                assert!(
                                    inputs_all_names.contains(name),
                                    "template {template_id} missing token {name} in inputs.all"
                                );
                            }
                        }
                        LayoutItem::Qr {
                            placement, value, ..
                        } => {
                            if let Extent::Size(size) = &placement.extent {
                                for sv in &size.0 {
                                    if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                        assert!(
                                            inputs_all_names.contains(r),
                                            "template {template_id} missing qr size ref {r} in inputs.all"
                                        );
                                    }
                                }
                            }
                            for name in bare_token_names(value) {
                                assert!(
                                    inputs_all_names.contains(name),
                                    "template {template_id} missing token {name} in inputs.all"
                                );
                            }
                        }
                        LayoutItem::Image {
                            placement,
                            name,
                            src,
                            ..
                        } => {
                            if let Extent::Size(size) = &placement.extent {
                                for sv in &size.0 {
                                    if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                        assert!(
                                            inputs_all_names.contains(r),
                                            "template {template_id} missing image size ref {r} in inputs.all"
                                        );
                                    }
                                }
                            }
                            if let Some(n) = name {
                                assert!(
                                    inputs_all_names.contains(n),
                                    "template {template_id} missing image name {n} in inputs.all"
                                );
                            }
                            if let Some(s) = src {
                                for name in bare_token_names(s) {
                                    assert!(
                                        inputs_all_names.contains(name),
                                        "template {template_id} missing token {name} in inputs.all"
                                    );
                                }
                            }
                        }
                        LayoutItem::Line { stroke, .. } => {
                            if let Some(Stroke {
                                color: DynamicValue::Ref(r),
                                ..
                            }) = stroke
                            {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {template_id} missing line stroke color ref {r} in inputs.all"
                                );
                            }
                        }
                        LayoutItem::Container {
                            placement,
                            stroke,
                            background,
                            items,
                            ..
                        } => {
                            if let Extent::Size(size) = &placement.extent {
                                for sv in &size.0 {
                                    if let SizeValue::Dynamic(DynamicValue::Ref(r)) = sv {
                                        assert!(
                                            inputs_all_names.contains(r),
                                            "template {template_id} missing container size ref {r} in inputs.all"
                                        );
                                    }
                                }
                            }
                            if let Some(Stroke {
                                color: DynamicValue::Ref(r),
                                ..
                            }) = stroke
                            {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {template_id} missing container stroke color ref {r} in inputs.all"
                                );
                            }
                            if let Some(DynamicValue::Ref(r)) = background {
                                assert!(
                                    inputs_all_names.contains(r),
                                    "template {template_id} missing container background ref {r} in inputs.all"
                                );
                            }
                            check_items(items, inputs_all_names, template_id);
                        }
                    }
                }
            }
            let Layout::Items(items) = &template.layout;
            check_items(items, &inputs_all_names, &template.id);
        }
    }

    fn whole_manifest_yaml() -> &'static str {
        r#"
name: Whole Manifest Fixture
unit: mm
dpi: 200
params:
  branch:
    type: enum
    values: [alpha, beta]
    default: alpha
  sub_branch:
    type: enum
    values: [sub1, sub2]
    default: sub1
  dyn_w:
    type: length
    min: 20
    max: 100
  weight:
    type: integer
    min: 100
    max: 900
    default: 400
  text_w:
    type: length
    min: 10
    max: 30
  qr_dim:
    type: length
    min: 10
    max: 40
  img_dim:
    type: length
    min: 10
    max: 30
  cont_dim:
    type: length
    min: 10
    max: 50
  img_param:
    type: string
  single_title:
    type: string
  alpha_text:
    type: string
  qr_code_val:
    type: string
  beta_multiline:
    type: string
    multiline: true
  asset_path:
    type: string
format:
  type: single
  width:
    min: "{dyn_w}"
    max: 100
  height: 50
layout:
  - type: line
    at: [0, 0]
    to: [10, 0]
    stroke:
      thickness: 0.5
  - type: container
    when:
      branch: alpha
    at: [0, 0]
    size: [50, 50]
    items:
      - type: text
        value: "{single_title} {alpha_text}"
        at: [0, 0]
        size: ["{text_w}", 10]
        font_size: 10
        font_weight: "{weight}"
      - type: image
        name: img_param
        at: [0, 10]
        size: ["{img_dim}", "{img_dim}"]
      - type: container
        when:
          sub_branch: sub1
        at: [0, 20]
        size: [30, 20]
        items:
          - type: qr
            value: "https://example.com/{qr_code_val}"
            at: [0, 0]
            size: ["{qr_dim}", "{qr_dim}"]
  - type: container
    when:
      branch: beta
    at: [0, 0]
    size: ["{cont_dim}", 50]
    items: []
  - type: container
    when:
      branch: beta
    at: [0, 0]
    size: [40, 50]
    items:
      - type: text
        value: "{beta_multiline}\n{vars.secret}"
        wrap: true
        at: [0, 0]
        size: [40, 20]
        font_size: 10
      - type: image
        src: "{asset_path}"
        at: [0, 20]
        size: [20, 20]
"#
    }

    #[test]
    fn whole_manifest_inputs_default_and_inputs_all() {
        let template = parse_template_ok(whole_manifest_yaml());

        let defaults = test_inputs_default(&template);
        let default_names: Vec<&str> = defaults.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            default_names,
            vec![
                "alpha_text",
                "branch",
                "dyn_w",
                "img_dim",
                "img_param",
                "qr_code_val",
                "qr_dim",
                "single_title",
                "sub_branch",
                "text_w",
                "weight",
            ]
        );

        // Check controls and properties on default list
        let get_def = |name: &str| defaults.iter().find(|i| i.name == name).unwrap();
        assert_eq!(get_def("branch").control, InputControl::Select);
        assert!(!get_def("branch").required);
        assert_eq!(
            get_def("branch").default,
            Some(ParamValue::String("alpha".to_string()))
        );

        assert_eq!(get_def("text_w").control, InputControl::Number);
        assert!(get_def("text_w").slider);
        assert_eq!(get_def("text_w").unit, Some("mm".to_string()));

        assert_eq!(get_def("img_param").control, InputControl::Image);
        assert!(get_def("img_param").interpolated);

        assert_eq!(get_def("single_title").control, InputControl::Text);
        assert!(get_def("single_title").truncated_elsewhere);

        assert_eq!(get_def("weight").control, InputControl::Integer);
        assert!(get_def("weight").slider);
        assert!(!get_def("weight").required);

        assert_eq!(get_def("alpha_text").control, InputControl::Text);
        assert!(get_def("alpha_text").required);
        assert!(get_def("alpha_text").truncated_elsewhere);

        assert_eq!(get_def("qr_code_val").control, InputControl::Text);
        assert!(get_def("qr_code_val").required);

        let all = test_inputs_all(&template);
        let all_names: Vec<&str> = all.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            all_names,
            vec![
                "alpha_text",
                "asset_path",
                "beta_multiline",
                "branch",
                "cont_dim",
                "dyn_w",
                "img_dim",
                "img_param",
                "qr_code_val",
                "qr_dim",
                "single_title",
                "sub_branch",
                "text_w",
                "weight"
            ]
        );

        let get_all = |name: &str| all.iter().find(|i| i.name == name).unwrap();
        assert_eq!(get_all("cont_dim").control, InputControl::Number);
        assert_eq!(get_all("beta_multiline").control, InputControl::Textarea);
        assert_eq!(get_all("asset_path").control, InputControl::Text);
        assert!(get_all("asset_path").interpolated);

        assert_eq!(template.variables(), vec!["secret".to_string()]);
    }

    #[test]
    fn endpoint_matches_render_for_whole_manifest() {
        let template = parse_template_ok(whole_manifest_yaml());

        // Label 1: branch alpha, sub_branch sub1
        let mut data1 = HashMap::new();
        data1.insert("dyn_w".to_string(), json!(50.0));
        data1.insert("text_w".to_string(), json!(20.0));
        data1.insert("single_title".to_string(), json!("Title"));
        data1.insert("alpha_text".to_string(), json!("Alpha"));
        data1.insert(
            "img_param".to_string(),
            json!(crate::render::SAMPLE_PNG_DATA_URI),
        );
        data1.insert("img_dim".to_string(), json!(15.0));
        data1.insert("qr_dim".to_string(), json!(15.0));
        data1.insert("qr_code_val".to_string(), json!("123"));

        let inputs1 = test_derive_inputs_for_label(&template, &data1);
        let input_names1: HashSet<String> = inputs1.into_iter().map(|i| i.name).collect();

        let resolved1 = crate::render::resolve_parameters(&template, &data1, None, None)
            .expect("resolve label 1");
        for k in data1.keys() {
            assert!(
                input_names1.contains(k),
                "data key {k} must be reported in inputs"
            );
        }
        for name in &input_names1 {
            assert!(
                resolved1.data.contains_key(name),
                "reported input {name} must be resolved by render"
            );
        }

        // Label 2: branch beta
        let mut data2 = HashMap::new();
        data2.insert("branch".to_string(), json!("beta"));
        data2.insert("dyn_w".to_string(), json!(60.0));
        data2.insert("cont_dim".to_string(), json!(35.0));
        data2.insert("beta_multiline".to_string(), json!("Multi\nLine"));
        data2.insert(
            "asset_path".to_string(),
            json!(crate::render::SAMPLE_PNG_DATA_URI),
        );

        let inputs2 = test_derive_inputs_for_label(&template, &data2);
        let input_names2: HashSet<String> = inputs2.into_iter().map(|i| i.name).collect();
        assert!(!input_names2.contains("alpha_text"));
        assert!(!input_names2.contains("qr_code_val"));
        assert!(!input_names2.contains("img_param"));
        assert!(input_names2.contains("cont_dim"));
        assert!(input_names2.contains("beta_multiline"));
        assert!(input_names2.contains("asset_path"));
        assert!(input_names2.contains("dyn_w"));

        let resolved2 = crate::render::resolve_parameters(&template, &data2, None, None)
            .expect("resolve label 2");
        for k in data2.keys() {
            assert!(
                input_names2.contains(k),
                "data key {k} must be reported in inputs"
            );
        }
        for name in &input_names2 {
            assert!(
                resolved2.data.contains_key(name),
                "reported input {name} must be resolved by render"
            );
        }
    }

    #[test]
    fn thumbnail_closure_renders_required_and_min_values() {
        let yaml = r#"
name: Thumbnail Closure
unit: mm
dpi: 200
params:
  mode:
    type: string
  subtitle:
    type: string
  length_param:
    type: length
    min: 15
    max: 50
  style:
    type: enum
    values: [normal, special]
    default: normal
  str_with_default:
    type: string
    default: my_default
  gate_only:
    type: string
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "{mode}"
    at: [0, 0]
    size: [50, 5]
    font_size: 8
  - type: container
    when:
      mode: mode
    at: [0, 5]
    size: [50, 15]
    items:
      - type: text
        value: "{subtitle} {style} {length_param} {str_with_default}"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
  - type: container
    when:
      gate_only: gate_only
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Never rendered"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert_eq!(ph.get("mode"), Some(&json!("mode")));
        assert_eq!(ph.get("subtitle"), Some(&json!("subtitle")));
        assert_eq!(ph.get("length_param"), Some(&json!(15.0)));
        assert_eq!(ph.get("style"), None);
        assert_eq!(ph.get("str_with_default"), None);
        assert_eq!(ph.get("gate_only"), None);

        let dt_formats = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let png = crate::render::render_thumbnail_png(&template, &ph, None, &BTreeMap::new(), &dt)
            .expect("thumbnail must render without missing data");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn gate_key_not_interpolated_is_never_invented_for() {
        let yaml = r#"
name: Gate Not Interpolated
unit: mm
dpi: 200
params:
  branch_mode:
    type: string
    default: standard
  uninterpolated_req:
    type: string
  message:
    type: string
format:
  type: single
  width: 50
  height: 20
layout:
  - type: container
    when:
      branch_mode: standard
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Active: {message}"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
  - type: container
    when:
      uninterpolated_req: uninterpolated_req
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Should not be active"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert_eq!(ph.get("uninterpolated_req"), None);
        assert_eq!(ph.get("branch_mode"), None);
        assert_eq!(ph.get("message"), Some(&json!("message")));

        let dt_formats = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let png = crate::render::render_thumbnail_png(&template, &ph, None, &BTreeMap::new(), &dt)
            .expect("thumbnail must render active default branch");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn name_service_resolves_on_its_own_is_never_invented_for() {
        let yaml = r#"
name: Service Resolved Enum
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: horizontal
  prefix:
    type: string
    default: custom_prefix
  count:
    type: integer
    default: 42
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "{orientation} {prefix} {count}"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
  - type: container
    when:
      prefix: custom_prefix
    at: [0, 10]
    size: [50, 10]
    items:
      - type: text
        value: "Gated on default prefix"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert_eq!(ph.get("orientation"), None);
        assert_eq!(ph.get("prefix"), None);
        assert_eq!(ph.get("count"), None);

        let now = chrono::Local::now();
        let resolved = crate::render::resolve_parameters(&template, &ph, None, None)
            .expect("resolve placeholder parameters");
        assert_eq!(resolved.data.get("orientation"), Some(&json!("horizontal")));
        assert_eq!(resolved.data.get("prefix"), Some(&json!("custom_prefix")));
        assert_eq!(resolved.data.get("count"), Some(&json!(42)));

        let dt_formats = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let png = crate::render::render_thumbnail_png(&template, &ph, None, &BTreeMap::new(), &dt)
            .expect("render thumbnail with resolved defaults");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn lenient_versus_strict_resolution() {
        let yaml = r#"
name: Lenient Strict
unit: mm
dpi: 200
params:
  choice:
    type: enum
    values: [one, two]
    default: one
  count:
    type: integer
    default: 5
  printed_on:
    type: datetime
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "{choice} {count} {printed_on}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_template_ok(yaml);

        // 1. Invalid enum value
        let mut bad_enum = HashMap::new();
        bad_enum.insert("choice".to_string(), json!("invalid_choice"));
        let lenient_enum = test_derive_inputs_for_label(&template, &bad_enum);
        assert_eq!(
            lenient_enum
                .iter()
                .find(|i| i.name == "choice")
                .unwrap()
                .default,
            Some(ParamValue::String("one".to_string()))
        );
        let strict_enum_err =
            crate::render::resolve_parameters(&template, &bad_enum, None, None).unwrap_err();
        assert_eq!(strict_enum_err.code(), "InvalidOptionValue");

        // 2. Non-numeric integer
        let mut bad_int = HashMap::new();
        bad_int.insert("count".to_string(), json!("not_a_number"));
        let lenient_int = test_derive_inputs_for_label(&template, &bad_int);
        assert_eq!(
            lenient_int
                .iter()
                .find(|i| i.name == "count")
                .unwrap()
                .default,
            Some(ParamValue::Integer(5))
        );
        let strict_int_err =
            crate::render::resolve_parameters(&template, &bad_int, None, None).unwrap_err();
        assert_eq!(
            strict_int_err.reason(),
            Some(Reason::RequestBodyInvalid.as_slug())
        );

        // 3. Unparseable datetime
        let mut bad_dt = HashMap::new();
        bad_dt.insert("printed_on".to_string(), json!("not_a_date"));
        let lenient_dt = test_derive_inputs_for_label(&template, &bad_dt);
        assert!(lenient_dt.iter().any(|i| i.name == "printed_on"));
        let strict_dt_err =
            crate::render::resolve_parameters(&template, &bad_dt, None, None).unwrap_err();
        assert_eq!(
            strict_dt_err.reason(),
            Some(Reason::DatetimeParamInvalid.as_slug())
        );
    }

    #[test]
    fn option_key_on_submitted_label_changes_neither_input_list_nor_render() {
        let yaml = r#"
name: Option Test
unit: mm
dpi: 200
params:
  title:
    type: string
  style:
    type: enum
    values: [plain, fancy]
    default: plain
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "{style} {title}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_template_ok(yaml);

        let mut data = HashMap::new();
        data.insert("title".to_string(), json!("Hello"));

        let inputs_no_opt = test_derive_inputs_for_label(&template, &data);

        let mut opt_map = BTreeMap::new();
        opt_map.insert("style".to_string(), "fancy".to_string());

        // Derive inputs uses data and lenient resolution
        let inputs_with_opt = test_derive_inputs_for_label(&template, &data);
        assert_eq!(inputs_no_opt, inputs_with_opt);

        let render_plain = crate::render::resolve_parameters(&template, &data, None, None).unwrap();
        assert_eq!(render_plain.data["style"], json!("plain"));
    }

    /// Proves that structural flow schema violations (missing/invalid direction, negative gaps,
    /// authored anchors or lines on packed children, bare or null flow) are refused at template
    /// load and quarantined with the exact JSON path.
    #[test]
    fn flow_load_refusals_and_quarantine() {
        let cases = [
            (
                "no_direction",
                r#"
name: No Direction
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { gap: 5 }
    items:
      - type: text
        value: "hi"
        size: [10, 10]
        font_size: 8
"#,
                "layout[0].flow.direction",
            ),
            (
                "bare_flow",
                r#"
name: Bare Flow
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow:
    items:
      - type: text
        value: "hi"
        size: [10, 10]
        font_size: 8
"#,
                "layout[0].flow.direction",
            ),
            (
                "null_flow",
                r#"
name: Null Flow
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: null
    items:
      - type: text
        value: "hi"
        size: [10, 10]
        font_size: 8
"#,
                "layout[0].flow.direction",
            ),
            (
                "unknown_direction",
                r#"
name: Unknown Direction
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: diagonal }
    items:
      - type: text
        value: "hi"
        size: [10, 10]
        font_size: 8
"#,
                "layout[0].flow.direction",
            ),
            (
                "negative_gap",
                r#"
name: Negative Gap
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row, gap: -2 }
    items:
      - type: text
        value: "hi"
        size: [10, 10]
        font_size: 8
"#,
                "layout[0].flow.gap",
            ),
            (
                "negative_line_gap",
                r#"
name: Negative Line Gap
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row, wrap: true, line_gap: -2 }
    items: []
"#,
                "layout[0].flow.line_gap",
            ),
            (
                "unknown_overflow",
                r#"
name: Unknown Overflow
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row, overflow: discard }
    items: []
"#,
                "layout[0].flow.overflow",
            ),
            (
                "packed_with_at",
                r#"
name: Packed With At
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row }
    items:
      - type: text
        value: "hi"
        at: [5, 5]
        size: [10, 10]
        font_size: 8
"#,
                "layout[0].items[0].at",
            ),
            (
                "packed_with_to",
                r#"
name: Packed With To
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row }
    items:
      - type: text
        value: "hi"
        to: [10, 10]
        font_size: 8
"#,
                "layout[0].items[0].to",
            ),
            (
                "packed_line",
                r#"
name: Packed Line
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row }
    items:
      - type: line
        at: [0, 0]
        to: [10, 0]
        stroke:
          thickness: 0.2
"#,
                "layout[0].items[0]",
            ),
        ];

        for (name, yaml, expected_fragment) in cases {
            let dir = temp_dir(&format!("flow_refusal_{name}"));
            write_template(&dir, &format!("{name}.yaml"), yaml);

            let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
            assert_eq!(
                registry.len(),
                0,
                "{name}: template with structural error must not be served"
            );
            let broken = registry.broken();
            assert_eq!(
                broken.len(),
                1,
                "{name}: template with structural error must be quarantined"
            );
            assert!(
                broken[0].error.contains(expected_fragment),
                "{name}: expected error to contain '{expected_fragment}', got '{}'",
                broken[0].error
            );

            fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn flow_wrap_and_trim_require_resolved_author_axes() {
        fn yaml(size: &str, direction: &str, rotate: Option<u16>, policy: &str) -> String {
            let rotation =
                rotate.map_or_else(String::new, |degrees| format!("    rotate: {degrees}\n"));
            format!(
                r#"
name: Flow Axis Validation
unit: mm
dpi: 200
format: {{ type: single, width: 100, height: 100 }}
layout:
  - type: container
    at: [0, 0]
    size: {size}
{rotation}    flow: {{ direction: {direction}, {policy} }}
    items: []
"#
            )
        }

        let refused = [
            (
                "row wrap",
                yaml("[content, 10]", "row", None, "wrap: true"),
                "wrap",
            ),
            (
                "column wrap",
                yaml("[10, content]", "column", None, "wrap: true"),
                "wrap",
            ),
            (
                "rotate 90 wrap",
                yaml("[10, content]", "row", Some(90), "wrap: true"),
                "wrap",
            ),
            (
                "rotate 270 wrap",
                yaml("[10, content]", "row", Some(270), "wrap: true"),
                "wrap",
            ),
            (
                "row trim",
                yaml("[content, 10]", "row", None, "overflow: trim"),
                "overflow",
            ),
            (
                "column trim",
                yaml("[10, content]", "column", None, "overflow: trim"),
                "overflow",
            ),
            (
                "rotate 90 trim",
                yaml("[content, 10]", "row", Some(90), "overflow: trim"),
                "overflow",
            ),
            (
                "rotate 270 trim",
                yaml("[content, 10]", "row", Some(270), "overflow: trim"),
                "overflow",
            ),
        ];
        for (name, yaml, key) in refused {
            let error = parse_and_validate(&yaml).expect_err(name);
            assert!(error.contains(key), "{name}: expected '{key}' in '{error}'");
        }

        let accepted = [
            yaml("[30, 10]", "row", None, "wrap: true"),
            yaml("[10, 30]", "column", None, "wrap: true"),
            yaml("[content, 10]", "row", Some(90), "wrap: true"),
            yaml("[content, 10]", "row", Some(270), "wrap: true"),
            yaml("[content, 10]", "row", None, "overflow: fail"),
            yaml("[10, content]", "column", None, "overflow: fail"),
            yaml("[30, 10]", "row", None, "overflow: trim"),
        ];
        for yaml in accepted {
            parse_and_validate(&yaml).expect("resolved flow axes should be accepted");
        }
    }

    #[test]
    fn flow_wrap_accepts_fill_from_sign_negative_anchor() {
        let yaml = r#"
name: Edge Relative Fill Flow
unit: mm
dpi: 200
format: { type: single, width: 100, height: 40 }
layout:
  - type: container
    at: [-100.0, -40.0]
    size: [fill, fill]
    flow: { direction: row, wrap: true }
    items: []
"#;
        parse_and_validate(yaml).expect("edge-relative fill axes are resolved");
    }

    #[test]
    fn unmigrated_multiline_text_template_is_quarantined_with_rename_error() {
        for (i, multiline_spec) in [
            "multiline: true",
            "multiline: false",
            "multiline: \"yes\"",
            "multiline:",
        ]
        .iter()
        .enumerate()
        {
            let dir = temp_dir(&format!("unmigrated_{i}"));
            write_template(&dir, "valid.yaml", &sample_yaml("Valid Template"));
            let bad_yaml = format!(
                r#"
name: Unmigrated
unit: mm
dpi: 180
format:
  type: single
  width: 60
  height: 20
layout:
  - type: text
    value: "test"
    at: [0, 0]
    size: [60, 20]
    font_size: 10
    {multiline_spec}
"#
            );
            write_template(&dir, "unmigrated.yaml", &bad_yaml);

            let registry =
                TemplateRegistry::load_from_dir(&dir).expect("registry load should not crash");
            assert_eq!(registry.len(), 1, "valid template must be served");
            assert!(registry.get("valid").is_some());
            assert!(registry.get("unmigrated").is_none());

            let broken = registry.broken();
            assert_eq!(broken.len(), 1, "unmigrated template must be quarantined");
            assert_eq!(
                broken[0].path, "unmigrated.yaml",
                "broken path must name the file"
            );
            assert!(
                broken[0].error.contains("layout[0].multiline"),
                "error must name the layout path: {}",
                broken[0].error
            );
            assert!(
                broken[0].error.contains("wrap"),
                "error must name the rename to wrap: {}",
                broken[0].error
            );

            fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn enum_key_on_integer_param_is_quarantined_with_unknown_field_error() {
        let dir = temp_dir("enum_integer_quarantine");
        write_template(&dir, "valid.yaml", &sample_yaml("Valid Template"));
        let bad_yaml = r#"
name: Bad Enum
unit: mm
dpi: 180
format:
  type: single
  width: 60
  height: 20
params:
  weight:
    type: integer
    default: 400
    enum: [100, 400, 700]
layout: []
"#;
        write_template(&dir, "bad_enum.yaml", bad_yaml);

        let registry =
            TemplateRegistry::load_from_dir(&dir).expect("registry load should not crash");
        assert_eq!(registry.len(), 1, "valid template must be served");
        assert!(registry.get("valid").is_some());
        assert!(registry.get("bad_enum").is_none());

        let broken = registry.broken();
        assert_eq!(
            broken.len(),
            1,
            "template carrying enum: must be quarantined"
        );
        assert_eq!(
            broken[0].path, "bad_enum.yaml",
            "broken path must name the offending file"
        );
        assert!(
            broken[0].error.contains("params.weight"),
            "error must name the parameter path, got: {}",
            broken[0].error
        );
        assert!(
            broken[0].error.contains("enum"),
            "error must name the unknown key `enum`, got: {}",
            broken[0].error
        );
        assert!(
            broken[0].error.contains("unknown field"),
            "error must be the generic unknown-field message, got: {}",
            broken[0].error
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn input_list_required_and_defaults() {
        let yaml = r#"
name: Input List Rules
unit: mm
dpi: 200
params:
  no_def_bool:
    type: boolean
  no_def_enum:
    type: enum
    values: [a, b]
  no_def_dt:
    type: datetime
  token_def:
    type: string
    default: "{vars.site}"
  lit_def:
    type: string
    default: "literal"
  gated_on_token:
    type: string
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "{no_def_bool} {no_def_enum} {no_def_dt} {token_def} {lit_def}"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
  - type: container
    when:
      token_def: "my_site"
    at: [0, 10]
    size: [50, 10]
    items:
      - type: text
        value: "{gated_on_token}"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let inputs_all = test_inputs_all(&template);

        let b = inputs_all.iter().find(|i| i.name == "no_def_bool").unwrap();
        assert!(b.required);
        assert!(b.default.is_none());

        let e = inputs_all.iter().find(|i| i.name == "no_def_enum").unwrap();
        assert!(e.required);
        assert!(e.default.is_none());

        let dt = inputs_all.iter().find(|i| i.name == "no_def_dt").unwrap();
        assert!(dt.required);
        assert!(dt.default.is_none());

        let tok = inputs_all.iter().find(|i| i.name == "token_def").unwrap();
        assert!(tok.required);
        assert!(tok.default.is_none());
        assert!(tok.default_error.is_some());

        let lit = inputs_all.iter().find(|i| i.name == "lit_def").unwrap();
        assert!(!lit.required);
        assert_eq!(lit.default, Some(ParamValue::String("literal".to_string())));

        // derive_inputs_for_label without variables/dt treats token_def as absent,
        // so when: token_def: "my_site" is inactive, and gated_on_token is omitted from derived inputs
        let derived = test_derive_inputs_for_label(&template, &HashMap::new());
        assert!(!derived.iter().any(|i| i.name == "gated_on_token"));
    }

    #[test]
    fn thumbnail_tests_for_new_default_rules() {
        // 1. Template with undefaulted datetime still renders a real date
        let yaml_dt = r#"
name: Thumbnail Undefaulted Datetime
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{printed_on:short_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_dt = parse_template_ok(yaml_dt);
        let now = chrono::Local::now();
        let ph_dt = test_placeholder_data(&t_dt, now);
        let dt_formats = BTreeMap::from([("short_date".to_string(), "%m/%d/%Y".to_string())]);
        let dt_res = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let png =
            crate::render::render_thumbnail_png(&t_dt, &ph_dt, None, &BTreeMap::new(), &dt_res)
                .unwrap();
        assert!(!png.is_empty());

        // 2. Reading undefaulted boolean renders via placeholder (false)
        let yaml_bool = r#"
name: Thumbnail Undefaulted Bool
unit: mm
dpi: 200
params:
  flag:
    type: boolean
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{flag}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_bool = parse_template_ok(yaml_bool);
        let ph_bool = test_placeholder_data(&t_bool, now);
        assert_eq!(ph_bool.get("flag"), Some(&json!(false)));
        let png =
            crate::render::render_thumbnail_png(&t_bool, &ph_bool, None, &BTreeMap::new(), &dt_res)
                .unwrap();
        assert!(!png.is_empty());

        // 3. Enum-gated container renders through option selection
        let yaml_enum_gate = r#"
name: Thumbnail Enum Gate
unit: mm
dpi: 200
params:
  mode:
    type: enum
    values: [primary, secondary]
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      mode: primary
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Primary branch"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
"#;
        let t_enum = parse_template_ok(yaml_enum_gate);
        let ph_enum = test_placeholder_data(&t_enum, now);
        assert!(!ph_enum.contains_key("mode"));
        let Layout::Items(items_enum) = &t_enum.layout;
        let images_enum = std::cell::RefCell::new(crate::render::ImageCollector::default());
        let resolved_enum =
            crate::render::resolve_parameters(&t_enum, &ph_enum, None, Some(&dt_res)).unwrap();
        let empty_settings_enum = BTreeMap::new();
        let env_enum = crate::render::RenderEnv {
            settings: &empty_settings_enum,
            datetime: &dt_res,
        };
        let ctx_enum = crate::render::RenderContext::new(
            "mm",
            200,
            &resolved_enum.data,
            None,
            &env_enum,
            &images_enum,
        )
        .with_instants(&resolved_enum.instants);
        assert!(
            !ctx_enum.is_item_active(&items_enum[0]),
            "enum container with no default must be inactive in thumbnail"
        );
        let png =
            crate::render::render_thumbnail_png(&t_enum, &ph_enum, None, &BTreeMap::new(), &dt_res)
                .unwrap();
        assert!(!png.is_empty());

        // 4. Boolean-gated container with no default does NOT render in thumbnail
        let yaml_bool_gate = r#"
name: Thumbnail Bool Gate
unit: mm
dpi: 200
params:
  enabled:
    type: boolean
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      enabled: "false"
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Disabled branch"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
"#;
        let t_bg = parse_template_ok(yaml_bool_gate);
        let ph_bg = test_placeholder_data(&t_bg, now);
        // enabled is not interpolated, so placeholder_data does not invent for it; it stays absent -> branch inactive
        assert!(!ph_bg.contains_key("enabled"));
        let Layout::Items(items_bg) = &t_bg.layout;
        let images = std::cell::RefCell::new(crate::render::ImageCollector::default());
        let resolved =
            crate::render::resolve_parameters(&t_bg, &ph_bg, None, Some(&dt_res)).unwrap();
        let empty_settings = BTreeMap::new();
        let env = crate::render::RenderEnv {
            settings: &empty_settings,
            datetime: &dt_res,
        };
        let ctx = crate::render::RenderContext::new("mm", 200, &resolved.data, None, &env, &images)
            .with_instants(&resolved.instants);
        assert!(
            !ctx.is_item_active(&items_bg[0]),
            "boolean container with no default must be inactive in thumbnail"
        );

        // 5. Broken default: thumbnail renders with placeholder because broken default is required
        let yaml_bad_def = r#"
name: Thumbnail Bad Def
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{vars.missing}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_bad = parse_template_ok(yaml_bad_def);
        let ph_bad = test_placeholder_data(&t_bad, now);
        assert_eq!(ph_bad.get("val"), Some(&json!("val")));
        let png =
            crate::render::render_thumbnail_png(&t_bad, &ph_bad, None, &BTreeMap::new(), &dt_res)
                .unwrap();
        assert!(!png.is_empty());

        // 6. List placeholder: required list with no default is invented as [name]
        let yaml_list_no_def = r#"
name: Thumbnail List NoDef
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_list_no_def = parse_template_ok(yaml_list_no_def);
        let ph_list_no_def = test_placeholder_data(&t_list_no_def, now);
        assert_eq!(
            ph_list_no_def.get("tags"),
            Some(&json!(["tags"])),
            "required list with no default must be invented as [name]"
        );
        let png_ld = crate::render::render_thumbnail_png(
            &t_list_no_def,
            &ph_list_no_def,
            None,
            &BTreeMap::new(),
            &dt_res,
        )
        .unwrap();
        assert!(!png_ld.is_empty());

        // 7. List with declared default is NOT invented
        let yaml_list_with_def = r#"
name: Thumbnail List WithDef
unit: mm
dpi: 200
params:
  tags:
    type: list
    default: [CONSUMABLE, KIDS]
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_list_def = parse_template_ok(yaml_list_with_def);
        let ph_list_def = test_placeholder_data(&t_list_def, now);
        assert!(
            !ph_list_def.contains_key("tags"),
            "list with resolvable default must not be invented"
        );
        let resolved_def = crate::render::resolve_parameters(
            &t_list_def,
            &std::collections::HashMap::new(),
            None,
            Some(&dt_res),
        )
        .unwrap();
        assert_eq!(
            resolved_def.data.get("tags"),
            Some(&json!(["CONSUMABLE", "KIDS"]))
        );

        // 8. List with default: [] is present and not invented (renders empty)
        let yaml_list_empty_def = r#"
name: Thumbnail List EmptyDef
unit: mm
dpi: 200
params:
  tags:
    type: list
    default: []
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_list_empty = parse_template_ok(yaml_list_empty_def);
        let ph_list_empty = test_placeholder_data(&t_list_empty, now);
        assert!(
            !ph_list_empty.contains_key("tags"),
            "list with default [] must not be invented"
        );
        let resolved_empty = crate::render::resolve_parameters(
            &t_list_empty,
            &std::collections::HashMap::new(),
            None,
            Some(&dt_res),
        )
        .unwrap();
        assert_eq!(resolved_empty.data.get("tags"), Some(&json!([])));
        let png_empty = crate::render::render_thumbnail_png(
            &t_list_empty,
            &ph_list_empty,
            None,
            &BTreeMap::new(),
            &dt_res,
        )
        .unwrap();
        assert!(!png_empty.is_empty());
    }

    #[test]
    fn thumbnail_printed_enum_with_declared_default_shows_default() {
        let yaml_enum = r#"
name: Enum Default Vertical
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: vertical
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_enum = parse_template_ok(yaml_enum);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&t_enum, now);
        assert_eq!(
            ph.get("orientation"),
            None,
            "defaulted enum must not be invented for"
        );
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let png_enum =
            crate::render::render_thumbnail_png(&t_enum, &ph, None, &BTreeMap::new(), &dt).unwrap();

        // control templates with literals
        let yaml_vertical = r#"
name: Control Vertical
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "vertical"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml_horizontal = r#"
name: Control Horizontal
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "horizontal"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let t_vertical = parse_template_ok(yaml_vertical);
        let t_horizontal = parse_template_ok(yaml_horizontal);
        let ph_vert = test_placeholder_data(&t_vertical, now);
        let ph_horiz = test_placeholder_data(&t_horizontal, now);
        let png_vert =
            crate::render::render_thumbnail_png(&t_vertical, &ph_vert, None, &BTreeMap::new(), &dt)
                .unwrap();
        let png_horiz = crate::render::render_thumbnail_png(
            &t_horizontal,
            &ph_horiz,
            None,
            &BTreeMap::new(),
            &dt,
        )
        .unwrap();
        assert_eq!(
            png_enum, png_vert,
            "enum thumbnail must match vertical literal control"
        );
        assert_ne!(
            png_enum, png_horiz,
            "enum thumbnail must differ from horizontal control"
        );
    }

    #[test]
    fn thumbnail_printed_enum_without_default_shows_first_value() {
        let yaml = r#"
name: Enum No Default
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [horizontal, vertical]
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert_eq!(ph.get("orientation"), Some(&json!("horizontal")));
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let png = crate::render::render_thumbnail_png(&template, &ph, None, &BTreeMap::new(), &dt)
            .expect("thumbnail with undefaulted printed enum must render");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        let resolved = crate::render::resolve_parameters(&template, &ph, None, Some(&dt)).unwrap();
        assert_eq!(resolved.data.get("orientation"), Some(&json!("horizontal")));
    }

    #[test]
    fn thumbnail_enum_only_gate_without_default_is_absent() {
        let yaml = r#"
name: Enum Gate No Default
unit: mm
dpi: 200
params:
  outline:
    type: enum
    values: [yes]
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      outline: yes
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Gated"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert!(!ph.contains_key("outline"));
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let resolved = crate::render::resolve_parameters(&template, &ph, None, Some(&dt)).unwrap();
        assert!(!resolved.data.contains_key("outline"));
        let Layout::Items(items) = &template.layout;
        let images = std::cell::RefCell::new(crate::render::ImageCollector::default());
        let env = crate::render::RenderEnv {
            settings: &BTreeMap::new(),
            datetime: &dt,
        };
        let ctx = crate::render::RenderContext::new("mm", 200, &resolved.data, None, &env, &images)
            .with_instants(&resolved.instants);
        assert!(!ctx.is_item_active(&items[0]));
        let png = crate::render::render_thumbnail_png(&template, &ph, None, &BTreeMap::new(), &dt)
            .unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn thumbnail_enum_only_gate_with_default_is_present() {
        let yaml = r#"
name: Enum Gate With Default
unit: mm
dpi: 200
params:
  outline:
    type: enum
    values: [yes]
    default: yes
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      outline: yes
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Gated"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert!(!ph.contains_key("outline"));
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let resolved = crate::render::resolve_parameters(&template, &ph, None, Some(&dt)).unwrap();
        assert_eq!(resolved.data.get("outline"), Some(&json!("yes")));
        let Layout::Items(items) = &template.layout;
        let images = std::cell::RefCell::new(crate::render::ImageCollector::default());
        let env = crate::render::RenderEnv {
            settings: &BTreeMap::new(),
            datetime: &dt,
        };
        let ctx = crate::render::RenderContext::new("mm", 200, &resolved.data, None, &env, &images)
            .with_instants(&resolved.instants);
        assert!(ctx.is_item_active(&items[0]));
        let png = crate::render::render_thumbnail_png(&template, &ph, None, &BTreeMap::new(), &dt)
            .unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn thumbnail_broken_enum_default_fails() {
        let yaml = r#"
name: Broken Enum Default
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [horizontal, vertical]
    default: "{vars.orient}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert!(
            !ph.contains_key("orientation"),
            "broken enum default must not be masked"
        );
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let variables = BTreeMap::new();
        let resolved_defaults =
            crate::render::resolve_declared_defaults(&template, &variables, &dt);
        let ph2 = template.placeholder_data(&resolved_defaults, now);
        assert!(!ph2.contains_key("orientation"));
        let err = crate::render::render_thumbnail_png(&template, &ph2, None, &variables, &dt)
            .unwrap_err();
        assert_eq!(err.code(), "TemplateInvalid");
        assert_eq!(err.reason(), Some("param_default_unresolvable"));
        assert!(
            err.message_text().contains("orientation"),
            "error must name orientation: {}",
            err.message_text()
        );
    }

    #[test]
    fn thumbnail_broken_string_default_is_masked() {
        let yaml = r#"
name: Broken String Default
unit: mm
dpi: 200
params:
  title:
    type: string
    default: "{vars.base}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{title}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let variables = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let resolved_defaults =
            crate::render::resolve_declared_defaults(&template, &variables, &dt);
        let ph = template.placeholder_data(&resolved_defaults, now);
        assert_eq!(ph.get("title"), Some(&json!("title")));
        let png =
            crate::render::render_thumbnail_png(&template, &ph, None, &variables, &dt).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // caller's render omitting title must still fail
        let err = crate::render::resolve_parameters(
            &template,
            &HashMap::new(),
            Some(&variables),
            Some(&dt),
        )
        .unwrap_err();
        assert_eq!(err.code(), "TemplateInvalid");
        assert_eq!(err.reason(), Some("param_default_unresolvable"));
    }

    #[test]
    fn thumbnail_enum_colour_ref_without_default_fails() {
        // An active text item that reads an undefaulted enum through a colour `{ref}` is
        // `interpolated: false` (`src/templates.rs:295`), so `placeholder_data` does not invent
        // for it. The thumbnail must therefore fail with `color_param_invalid`, while a caller's
        // render that supplies the enum succeeds. This pins the unlisted BREAKING change where the
        // deleted `default_option_selection` previously supplied every declared enum.
        let yaml = r#"
name: Enum Colour Ref
unit: mm
dpi: 200
params:
  palette:
    type: enum
    values: [red, blue]
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{palette}"
"#;
        let template = parse_template_ok(yaml);
        let now = chrono::Local::now();
        let variables = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now,
        };
        let resolved_defaults =
            crate::render::resolve_declared_defaults(&template, &variables, &dt);
        let ph = template.placeholder_data(&resolved_defaults, now);
        assert!(
            !ph.contains_key("palette"),
            "colour ref is not interpolated, so placeholder must not contain palette"
        );
        let err =
            crate::render::render_thumbnail_png(&template, &ph, None, &variables, &dt).unwrap_err();
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("color_param_invalid"));
        assert!(
            err.message_text().contains("palette"),
            "error must name palette: {}",
            err.message_text()
        );

        // Caller's render supplying the enum succeeds.
        let mut data = HashMap::new();
        data.insert("palette".to_string(), json!("red"));
        let png = crate::render::render_thumbnail_png(&template, &data, None, &variables, &dt)
            .expect("caller supplying palette must render");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn repeating_container_thumbnail_placeholder_draws_one_instance() {
        // 5.5: Thumbnail placeholder data invents 1 instance for repeat-only list parameter
        let rep_yaml = r#"
name: RepThumb
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  tags:
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
"#;
        let template = parse_template_ok(rep_yaml);
        let Layout::Items(items) = &template.layout;
        let now = chrono::Local::now();
        let ph = test_placeholder_data(&template, now);
        assert_eq!(ph.get("tags"), Some(&serde_json::json!(["tags"])));
        let dt_formats = BTreeMap::new();
        let dt_res = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let empty_settings = BTreeMap::new();
        let env = crate::render::RenderEnv {
            settings: &empty_settings,
            datetime: &dt_res,
        };
        let resolved =
            crate::render::resolve_parameters(&template, &ph, Some(&empty_settings), Some(&dt_res))
                .unwrap();
        let images = std::cell::RefCell::new(crate::render::ImageCollector::default());
        let ctx = crate::render::RenderContext::new("mm", 200, &resolved.data, None, &env, &images);
        let (meas, _) = ctx
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();
        assert_eq!(
            meas[0].children.len(),
            1,
            "thumbnail must measure 1 placeholder instance"
        );
        let src = ctx
            .render_items(
                items,
                &meas,
                (100.0, 100.0),
                &HashMap::new(),
                None,
                "layout",
            )
            .unwrap();
        assert!(
            src.contains("Tag:\u{a0}tags") || src.contains("Tag: tags"),
            "thumbnail must draw the placeholder instance"
        );
    }

    #[test]
    fn load_time_unescaped_brace_in_default_rejected() {
        let yaml = r#"
name: Bad Brace
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "unclosed { brace"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        assert!(parse_and_validate(yaml).is_err());
    }

    #[test]
    fn invalid_color_literal_on_text_item_fails_load() {
        for bad_color in [
            "chartreuse",
            "redmm",
            "\"#ff0000in\"",
            "\"ff0000\"",
            "\"#ff000\"",
            "\"\"",
            "16711680",
            "\"   \"",
            "\"re d\"",
            "\"# f0f\"",
        ] {
            let bad_yaml = format!(
                r#"
name: BadColor
unit: mm
dpi: 200
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: {bad_color}
"#
            );
            let err = match parse_and_validate(&bad_yaml) {
                Ok(val) => {
                    panic!("bad_color '{bad_color}' unexpectedly succeeded validation: {val:?}")
                }
                Err(err) => err,
            };
            let err_str = err.to_string();
            assert!(
                err_str.contains("layout[0]")
                    && (err_str.contains("color") || err_str.contains("colour")),
                "expected error naming layout path and color field for '{bad_color}', got: {err_str}"
            );
            assert!(
                !err_str.contains("unknown field"),
                "failure for '{bad_color}' must be colour validation rather than an unrecognised field, got: {err_str}"
            );
        }
    }

    #[test]
    fn invalid_color_literal_on_shape_items_fails_load() {
        for bad_color in ["chartreuse", "\"   \"", "\"re d\"", "\"# f0f\""] {
            // Container background invalid color
            let bad_bg_yaml = format!(
                r#"
name: BadBg
unit: mm
dpi: 200
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: {bad_color}
    items: []
"#
            );
            let err_bg = parse_and_validate(&bad_bg_yaml).unwrap_err();
            let err_bg_str = err_bg.to_string();
            assert!(
                err_bg_str.contains("layout[0]") && err_bg_str.contains("background"),
                "expected error naming layout[0] and background field for '{bad_color}', got: {err_bg_str}"
            );

            // Line stroke color invalid color
            let bad_stroke_line_yaml = format!(
                r#"
name: BadStrokeLine
unit: mm
dpi: 200
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 0.5
      color: {bad_color}
"#
            );
            let err_stroke_line = parse_and_validate(&bad_stroke_line_yaml).unwrap_err();
            let err_stroke_line_str = err_stroke_line.to_string();
            assert!(
                err_stroke_line_str.contains("layout[0]")
                    && err_stroke_line_str.contains("stroke")
                    && err_stroke_line_str.contains("color"),
                "expected error naming layout[0], stroke, and color for '{bad_color}', got: {err_stroke_line_str}"
            );

            // Container stroke color invalid color
            let bad_stroke_container_yaml = format!(
                r#"
name: BadStrokeContainer
unit: mm
dpi: 200
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    stroke:
      thickness: 0.5
      color: {bad_color}
    items: []
"#
            );
            let err_stroke_container = parse_and_validate(&bad_stroke_container_yaml).unwrap_err();
            let err_stroke_container_str = err_stroke_container.to_string();
            assert!(
                err_stroke_container_str.contains("layout[0]")
                    && err_stroke_container_str.contains("stroke")
                    && err_stroke_container_str.contains("color"),
                "expected error naming layout[0], stroke, and color for '{bad_color}', got: {err_stroke_container_str}"
            );
        }
    }

    #[test]
    fn color_rejected_on_non_paint_items() {
        let qr_yaml = r#"
name: QR Color
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: qr
    value: "test"
    at: [0, 0]
    size: [20, 20]
    color: red
"#;
        let err = parse_and_validate(qr_yaml).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("layout[0]") && err_str.contains("unknown field `color`"),
            "got: {err_str}"
        );

        let image_yaml = r#"
name: Image Color
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: image
    name: "logo.png"
    at: [0, 0]
    size: [20, 20]
    color: red
"#;
        let err = parse_and_validate(image_yaml).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("layout[0]") && err_str.contains("unknown field `color`"),
            "got: {err_str}"
        );

        let line_yaml = r#"
name: Line Color
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 1
    color: red
"#;
        let err = parse_and_validate(line_yaml).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("layout[0]") && err_str.contains("unknown field `color`"),
            "got: {err_str}"
        );

        let container_yaml = r#"
name: Container Color
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    color: red
    items: []
"#;
        let err = parse_and_validate(container_yaml).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("layout[0]") && err_str.contains("unknown field `color`"),
            "got: {err_str}"
        );
    }

    #[test]
    fn ink_rejected_on_text_item() {
        let ink_yaml = r#"
name: InkText
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    ink: red
"#;
        let err = parse_and_validate(ink_yaml).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("layout[0]") && err_str.contains("unknown field `ink`"),
            "ink on a text item must be refused with unknown field `ink` naming layout path, got: {err_str}"
        );
    }

    #[test]
    fn reject_undeclared_or_bad_type_color_parameter_reference() {
        // 1. Text color
        let undeclared_yaml = r#"
name: Undeclared Color Ref
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{missing}"
"#;
        let err = parse_and_validate(undeclared_yaml).unwrap_err();
        assert!(err.to_string().contains("missing"), "got: {err}");

        for bad_type in ["length", "number", "integer", "boolean", "datetime"] {
            let yaml = format!(
                r#"
name: Bad Type Color Ref
unit: mm
dpi: 200
params:
  color_param:
    type: {bad_type}
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{{color_param}}"
"#
            );
            let err = parse_and_validate(&yaml).unwrap_err();
            let err_str = err.to_string().to_lowercase();
            assert!(
                err_str.contains("color_param"),
                "expected error to name color_param for type {bad_type}, got: {err}"
            );
            assert!(
                err_str.contains(bad_type),
                "expected error to name type {bad_type}, got: {err}"
            );
        }

        // string and enum are accepted on text color
        let string_yaml = r#"
name: String Color Ref
unit: mm
dpi: 200
params:
  brand:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{brand}"
"#;
        assert!(parse_and_validate(string_yaml).is_ok());

        let enum_yaml = r#"
name: Enum Color Ref
unit: mm
dpi: 200
params:
  brand:
    type: enum
    values: [red, blue]
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{brand}"
"#;
        assert!(parse_and_validate(enum_yaml).is_ok());

        // 2. Container background
        let undeclared_bg = r#"
name: Undeclared Bg Ref
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: "{missing}"
    items: []
"#;
        let err = parse_and_validate(undeclared_bg).unwrap_err();
        assert!(err.to_string().contains("missing"), "got: {err}");

        for bad_type in ["length", "number", "integer", "boolean", "datetime"] {
            let bad_yaml = format!(
                r#"
name: Bad Type Bg Ref
unit: mm
dpi: 200
params:
  bg_param:
    type: {bad_type}
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: "{{bg_param}}"
    items: []
"#
            );
            let err = parse_and_validate(&bad_yaml).unwrap_err();
            let err_str = err.to_string().to_lowercase();
            assert!(
                err_str.contains("bg_param"),
                "expected error to name bg_param for type {bad_type}, got: {err}"
            );
            assert!(
                err_str.contains(bad_type),
                "expected error to name type {bad_type}, got: {err}"
            );
        }

        let good_bg = r#"
name: Good Bg Ref
unit: mm
dpi: 200
params:
  bg_param:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: "{bg_param}"
    items: []
"#;
        assert!(parse_and_validate(good_bg).is_ok());

        // 3. Line and container stroke.color
        let undeclared_line_stroke = r#"
name: Undeclared Line Stroke Ref
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 1
      color: "{missing}"
"#;
        let err = parse_and_validate(undeclared_line_stroke).unwrap_err();
        assert!(err.to_string().contains("missing"), "got: {err}");

        for bad_type in ["length", "number", "integer", "boolean", "datetime"] {
            let bad_line_yaml = format!(
                r#"
name: Bad Line Stroke Ref
unit: mm
dpi: 200
params:
  border:
    type: {bad_type}
format: {{ type: single, width: 50, height: 20 }}
layout:
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 1
      color: "{{border}}"
"#
            );
            let err = parse_and_validate(&bad_line_yaml).unwrap_err();
            let err_str = err.to_string().to_lowercase();
            assert!(
                err_str.contains("border"),
                "expected error to name border for type {bad_type}, got: {err}"
            );
            assert!(
                err_str.contains(bad_type),
                "expected error to name type {bad_type}, got: {err}"
            );
        }

        let good_line_stroke = r#"
name: Good Line Stroke Ref
unit: mm
dpi: 200
params:
  border:
    type: enum
    values: [red, green]
format: { type: single, width: 50, height: 20 }
layout:
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 1
      color: "{border}"
"#;
        assert!(parse_and_validate(good_line_stroke).is_ok());
    }

    #[test]
    fn input_derivation_for_color_references() {
        // 1. Ungated color, background, stroke.color (on container and line) references marked not interpolated
        let ungated_yaml = r#"
name: Ungated Colors
unit: mm
dpi: 200
params:
  brand:
    type: string
  bg_color:
    type: string
  border_color:
    type: string
  line_color:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: line
    at: [0, 0]
    to: [50, 0]
    stroke:
      thickness: 1
      color: "{line_color}"
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: "{bg_color}"
    stroke:
      thickness: 1
      color: "{border_color}"
    items:
      - type: text
        value: "Hello"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
        color: "{brand}"
"#;
        let t_ungated = parse_template_ok(ungated_yaml);
        let inputs = test_inputs_all(&t_ungated);
        for param_name in ["brand", "bg_color", "border_color", "line_color"] {
            let input = inputs
                .iter()
                .find(|i| i.name == param_name)
                .unwrap_or_else(|| panic!("{param_name} in inputs_all"));
            assert!(
                !input.interpolated,
                "{param_name} reference must not be marked interpolated"
            );
        }

        // 2. when-gated-off item (container background or line stroke) contributes nothing while when's own parameters still appear
        let gated_yaml = r#"
name: Gated Color
unit: mm
dpi: 200
params:
  brand:
    type: string
  line_color:
    type: string
  show_brand:
    type: boolean
    default: false
  show_line:
    type: boolean
    default: false
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: "{brand}"
    when:
      show_brand: "true"
    items: []
  - type: line
    at: [0, 0]
    to: [50, 0]
    stroke:
      thickness: 1
      color: "{line_color}"
    when:
      show_line: "true"
"#;
        let t_gated = parse_template_ok(gated_yaml);
        let mut data = HashMap::new();
        data.insert("show_brand".to_string(), serde_json::json!(false));
        data.insert("show_line".to_string(), serde_json::json!(false));
        let inputs_for_label = test_derive_inputs_for_label(&t_gated, &data);
        assert!(
            !inputs_for_label.iter().any(|i| i.name == "brand"),
            "gated-off container background color param must not be in input list"
        );
        assert!(
            !inputs_for_label.iter().any(|i| i.name == "line_color"),
            "gated-off line stroke color param must not be in input list"
        );
        assert!(
            inputs_for_label.iter().any(|i| i.name == "show_brand"),
            "when param must be in input list"
        );
        assert!(
            inputs_for_label.iter().any(|i| i.name == "show_line"),
            "when param must be in input list"
        );

        // 3. Parameter used as a color and interpolated elsewhere appears once, interpolated
        let dual_yaml = r#"
name: Dual Color
unit: mm
dpi: 200
params:
  brand:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Brand: {brand}"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
  - type: text
    value: "Title"
    at: [0, 10]
    size: [50, 10]
    font_size: 10
    color: "{brand}"
"#;
        let t_dual = parse_template_ok(dual_yaml);
        let inputs = test_inputs_all(&t_dual);
        let matching: Vec<_> = inputs.iter().filter(|i| i.name == "brand").collect();
        assert_eq!(matching.len(), 1, "brand must appear exactly once");
        assert!(
            matching[0].interpolated,
            "interpolated wins when parameter is used both ways"
        );
    }

    #[test]
    fn whitespace_only_color_template_is_quarantined() {
        let dir = temp_dir("whitespace_only_color_template_is_quarantined");
        let bad_yaml = r#"
name: WhitespaceColor
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "   "
"#;
        write_template(&dir, "whitespace_color.yaml", bad_yaml);
        let registry = TemplateRegistry::load_from_dir(&dir).expect("registry load must not fail");
        assert_eq!(registry.len(), 0);
        let broken = registry.broken();
        assert_eq!(broken.len(), 1);
        let item = &broken[0];
        assert_eq!(item.path, "whitespace_color.yaml");
        assert!(
            item.error.contains("layout[0]") && item.error.contains("color"),
            "expected layout path and field in error: {}",
            item.error
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_parameter_load_refusals_quarantine_files() {
        let dir = temp_dir("list_load_refusals");
        let valid_yaml = sample_yaml("valid");
        write_template(&dir, "valid.yaml", &valid_yaml);

        // 1. Declared list in when condition
        let when_list_yaml = r#"
name: WhenList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  tags: { type: list }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: { tags: KIDS }
"#;
        write_template(&dir, "when_list.yaml", when_list_yaml);

        // 2. Image binding declared list
        let image_list_yaml = r#"
name: ImageList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  tags: { type: list }
layout:
  - type: image
    name: tags
    at: [0, 0]
    size: [50, 20]
"#;
        write_template(&dir, "image_list.yaml", image_list_yaml);

        // 3. Bare token on declared list
        let bare_list_yaml = r#"
name: BareList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  tags: { type: list }
layout:
  - type: text
    value: "{tags}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "bare_list.yaml", bare_list_yaml);

        // 4. Format on declared list
        let format_list_yaml = r#"
name: FormatList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  tags: { type: list }
layout:
  - type: text
    value: "{tags:short_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "format_list.yaml", format_list_yaml);

        // 5. Bare reader on declared list {tags:join}
        let bare_join_yaml = r#"
name: BareJoin
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  tags: { type: list }
layout:
  - type: text
    value: "{tags:join}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "bare_join.yaml", bare_join_yaml);

        // 6. Join on non-list string
        let join_string_yaml = r#"
name: JoinString
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  title: { type: string }
layout:
  - type: text
    value: "{title:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "join_string.yaml", join_string_yaml);

        // 7. Join on undeclared
        let join_undeclared_yaml = r#"
name: JoinUndeclared
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{items:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "join_undeclared.yaml", join_undeclared_yaml);

        // 8. Join on sys.now
        let join_sys_yaml = r#"
name: JoinSys
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{sys.now:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "join_sys.yaml", join_sys_yaml);

        // 9. Valid list with join parses and loads cleanly
        let valid_list_yaml = r#"
name: ValidList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  tags: { type: list, default: [CONSUMABLE, KIDS] }
layout:
  - type: text
    value: "{tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "valid_list.yaml", valid_list_yaml);

        // 10. Explicit null when: null loads cleanly as unconditional
        let when_null_yaml = r#"
name: WhenNull
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Unconditional"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: null
"#;
        write_template(&dir, "when_null.yaml", when_null_yaml);

        // 11. Undeclared when: key keeps existing message without layout path
        let when_undeclared_yaml = r#"
name: WhenUndeclared
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: { undeclared_key: KIDS }
"#;
        write_template(&dir, "when_undeclared.yaml", when_undeclared_yaml);

        // 12. when: {} is refused
        let when_empty_yaml = r#"
name: WhenEmpty
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: {}
"#;
        write_template(&dir, "when_empty.yaml", when_empty_yaml);

        // 13. Blank when: key is refused
        let when_blank_key_yaml = r#"
name: WhenBlankKey
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: { " ": "KIDS" }
"#;
        write_template(&dir, "when_blank_key.yaml", when_blank_key_yaml);

        // 14. Blank when: value is refused
        let when_blank_val_yaml = r#"
name: WhenBlankVal
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  category: { type: string }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: { category: " " }
"#;
        write_template(&dir, "when_blank_val.yaml", when_blank_val_yaml);

        // 15. List driving dimension is refused
        let dim_list_yaml = r#"
name: DimList
unit: mm
dpi: 200
format:
  type: single
  width: "{tags}"
  height: 20
params:
  tags:
    type: list
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        write_template(&dir, "dim_list.yaml", dim_list_yaml);

        // 16. List driving color is refused
        let color_list_yaml = r#"
name: ColorList
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
params:
  tags:
    type: list
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{tags}"
"#;
        write_template(&dir, "color_list.yaml", color_list_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).expect("registry load must not fail");
        assert_eq!(registry.len(), 3); // valid, valid_list, when_null
        assert!(registry.get("valid").is_some());
        assert!(registry.get("valid_list").is_some());
        assert!(registry.get("when_null").is_some());

        let broken = registry.broken();
        assert_eq!(broken.len(), 14);

        // Verify specific error messages
        let find_broken = |path: &str| broken.iter().find(|b| b.path == path).unwrap();

        let b_when = find_broken("when_list.yaml");
        assert!(b_when.error.contains("tags") && b_when.error.contains("layout[0]"));

        let b_img = find_broken("image_list.yaml");
        assert!(b_img.error.contains("tags") && b_img.error.contains("image name"));

        let b_bare = find_broken("bare_list.yaml");
        assert!(b_bare.error.contains("{tags}"));

        let b_fmt = find_broken("format_list.yaml");
        assert!(b_fmt.error.contains("{tags:short_date}"));

        let b_join_bare = find_broken("bare_join.yaml");
        assert!(
            b_join_bare.error.contains("{tags:join}")
                && b_join_bare.error.contains("join('<separator>')")
        );

        let b_join_str = find_broken("join_string.yaml");
        assert!(b_join_str.error.contains("{title:join(', ')}"));

        let b_join_und = find_broken("join_undeclared.yaml");
        assert!(b_join_und.error.contains("{items:join(', ')}"));

        let b_join_sys = find_broken("join_sys.yaml");
        assert!(b_join_sys.error.contains("{sys.now:join(', ')}"));

        let b_when_und = find_broken("when_undeclared.yaml");
        assert!(b_when_und
            .error
            .contains("undeclared parameter 'undeclared_key' referenced in when condition"));

        let b_when_empty = find_broken("when_empty.yaml");
        assert!(b_when_empty.error.contains("when must not be empty"));

        let b_when_blank_key = find_broken("when_blank_key.yaml");
        assert!(b_when_blank_key
            .error
            .contains("undeclared parameter ' ' referenced in when condition"));

        let b_when_blank_val = find_broken("when_blank_val.yaml");
        assert!(b_when_blank_val
            .error
            .contains("when must not contain empty values"));

        let b_dim_list = find_broken("dim_list.yaml");
        assert!(b_dim_list
            .error
            .contains("parameter 'tags' of type List cannot be used in format width"));

        let b_color_list = find_broken("color_list.yaml");
        assert!(b_color_list
            .error
            .contains("parameter 'tags' of type List cannot be used in color"));

        fs::remove_dir_all(&dir).ok();
    }

    // Issue 322: Task 1.3 - Unit-test the refusal for each site: text value, qr value, image src
    #[test]
    fn issue_322_bare_token_undeclared_refused_at_all_sites() {
        // 1. text value
        let yaml_text = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: text
    value: "{sku}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml_text).unwrap_err();
        assert_eq!(err, "template contains '{sku}': undeclared parameter 'sku'");

        // 2. qr value
        let yaml_qr = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: qr
    value: "https://example.com/{sku}"
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml_qr).unwrap_err();
        assert_eq!(err, "template contains '{sku}': undeclared parameter 'sku'");

        // 3. image src
        let yaml_img = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: image
    src: "logos/{sku}.png"
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml_img).unwrap_err();
        assert_eq!(err, "template contains '{sku}': undeclared parameter 'sku'");
    }

    // Issue 322: Task 1.4 - Unit-test what the rule does not touch
    #[test]
    fn issue_322_namespaced_tokens_and_defaults_and_datetime() {
        // 1. vars and sys tokens load without declaration
        let yaml_namespaced = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: text
    value: "{vars.site} {sys.now} {sys.now:iso_date}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        assert!(parse_and_validate(yaml_namespaced).is_ok());

        // 2. default with bare token reports bare-token-in-default message
        let yaml_default = r#"
name: T
unit: mm
dpi: 200
params:
  declared:
    type: string
    default: "{message}"
format: { type: single, width: 20, height: 10 }
layout: []
"#;
        let err = parse_and_validate(yaml_default).unwrap_err();
        assert_eq!(
            err,
            "bare token '{message}' is not allowed in a default; only namespaced tokens ({vars.…}, {sys.…}) are supported"
        );

        // 3. template printing {datetime} loads when declared and is quarantined/rejected when not
        let yaml_dt_undeclared = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: text
    value: "{datetime}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        let err = parse_and_validate(yaml_dt_undeclared).unwrap_err();
        assert_eq!(
            err,
            "template contains '{datetime}': undeclared parameter 'datetime'"
        );

        let yaml_dt_declared = r#"
name: T
unit: mm
dpi: 200
params:
  datetime:
    type: string
format: { type: single, width: 20, height: 10 }
layout:
  - type: text
    value: "{datetime}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        assert!(parse_and_validate(yaml_dt_declared).is_ok());
    }

    // Issue 322: Task 1.5 - Existence is the only condition (any type)
    #[test]
    fn issue_322_bare_token_accepts_any_declared_parameter_type() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  copies:
    type: integer
  bold:
    type: boolean
  width:
    type: length
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{copies} {bold} {width}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        assert!(parse_and_validate(yaml).is_ok());
    }

    // Issue 322: Task 2.2 - Image name validation: undeclared, wrong type, invalid characters
    #[test]
    fn issue_322_image_name_validation_outcomes() {
        // 1. name: logo with no logo declared
        let yaml_undeclared = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: image
    name: logo
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml_undeclared).unwrap_err();
        assert_eq!(err, "undeclared parameter 'logo' referenced in image name");

        // 2. logo declared as integer (wrong type)
        let yaml_wrong_type = r#"
name: T
unit: mm
dpi: 200
params:
  logo:
    type: integer
format: { type: single, width: 20, height: 10 }
layout:
  - type: image
    name: logo
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml_wrong_type).unwrap_err();
        assert_eq!(
            err,
            "parameter 'logo' of type Integer cannot be used in image name"
        );

        // 3. name: "my logo" with spaces reports charset first
        let yaml_bad_charset = r#"
name: T
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: image
    name: "my logo"
    at: [0, 0]
    size: [10, 10]
"#;
        let err = parse_and_validate(yaml_bad_charset).unwrap_err();
        assert_eq!(
            err,
            "image name 'my logo' contains invalid characters; must match ^[a-zA-Z0-9_-]+$"
        );
    }

    // Issue 322: Task 2.3 - Declared string image name binds and renders/errors as expected
    #[test]
    fn issue_322_image_name_declared_string_binding_and_render() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  logo:
    type: string
format: { type: single, width: 20, height: 10 }
layout:
  - type: image
    name: logo
    at: [0, 0]
    size: [10, 10]
"#;
        let template = parse_template_ok(yaml);
        let dt_formats = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now: chrono::Local::now(),
        };

        // Render with logo supplied as PNG data URI -> Ok
        let mut data = HashMap::new();
        data.insert(
            "logo".to_string(),
            json!(crate::render::SAMPLE_PNG_DATA_URI),
        );
        let png = crate::render::render_single_label_image(
            &template,
            &data,
            None,
            &BTreeMap::new(),
            &dt,
            crate::render::ImageRenderOptions::default(),
        );
        assert!(png.is_ok());

        // Render omitting logo -> 422 MissingField naming logo
        let empty_data = HashMap::new();
        let err = crate::render::render_single_label_image(
            &template,
            &empty_data,
            None,
            &BTreeMap::new(),
            &dt,
            crate::render::ImageRenderOptions::default(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "MissingField");
        assert_eq!(err.details().unwrap()["field"], "logo");
    }

    // Issue 322: Task 4.2 - Post-change input entry derivation rules
    #[test]
    fn issue_322_input_entry_derivation_rules() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  zeta_text:
    type: string
    multiline: false
  alpha_area:
    type: string
    multiline: true
  asset_path:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{zeta_text}"
    wrap: true
    at: [0, 0]
    size: [50, 10]
    font_size: 8
  - type: text
    value: "{alpha_area}"
    at: [0, 10]
    size: [50, 10]
    font_size: 8
  - type: image
    src: "{asset_path}"
    at: [0, 0]
    size: [10, 10]
"#;
        let template = parse_template_ok(yaml);
        let inputs = test_inputs_all(&template);
        let names: Vec<&str> = inputs.iter().map(|i| i.name.as_str()).collect();

        // 1. Sorted alphabetically by name
        assert_eq!(names, vec!["alpha_area", "asset_path", "zeta_text"]);

        // 2. multiline: true string gets Textarea
        assert_eq!(inputs[0].control, InputControl::Textarea);

        // 3. image src over declared param gets Text
        assert_eq!(inputs[1].control, InputControl::Text);
        assert!(inputs[1].interpolated);

        // 4. multiline: false string read by wrap: true item keeps Text (no promotion)
        assert_eq!(inputs[2].control, InputControl::Text);
    }

    // Issue 322: Task 4.3 - Union rule: image name in one branch and text in another gets Image
    #[test]
    fn issue_322_input_entry_union_rule_image_wins() {
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  mode:
    type: enum
    values: [img, txt]
    default: img
  shared:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      mode: img
    at: [0, 0]
    size: [50, 20]
    items:
      - type: image
        name: shared
        at: [0, 0]
        size: [10, 10]
  - type: container
    when:
      mode: txt
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "{shared}"
        at: [0, 0]
        size: [50, 10]
        font_size: 8
"#;
        let template = parse_template_ok(yaml);
        let all = test_inputs_all(&template);
        let shared_input = all.iter().find(|i| i.name == "shared").unwrap();
        assert_eq!(
            shared_input.control,
            InputControl::Image,
            "image binding must win in inputs.all union"
        );
    }

    #[test]
    fn repeat_scope_reference_refusals_and_permissions() {
        // 3.4: inside repeat scope: size, color, image name referencing list parameter are refused
        let size_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: text
            value: "hi"
            size: ["{tags}", 10]
            font_size: 8
"#;
        let err = parse_and_validate(size_yaml).unwrap_err();
        assert!(err.contains("parameter 'tags' of type List cannot be used in text width"));

        let color_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: text
            value: "hi"
            size: [10, 10]
            color: "{tags}"
            font_size: 8
"#;
        let err = parse_and_validate(color_yaml).unwrap_err();
        assert!(err.contains("parameter 'tags' of type List cannot be used in color"));

        let image_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: image
            name: tags
            size: [10, 10]
"#;
        let err = parse_and_validate(image_yaml).unwrap_err();
        assert!(err.contains("parameter 'tags' of type List cannot be used in image name"));

        // 3.5: outside repeat scope: bare {tags} on list is refused
        let bare_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: text
    value: "{tags}"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#;
        let err = parse_and_validate(bare_yaml).unwrap_err();
        assert!(err.contains("list parameter cannot be used as a bare token; a list is read through join('<separator>')"));

        // 3.5: outside repeat scope: {tags:join(', ')} is accepted
        let join_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: text
    value: "{tags:join(', ')}"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#;
        assert!(parse_and_validate(join_yaml).is_ok());

        // 3.5: when: naming list parameter outside repeat scope is refused
        let when_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: text
    when:
      tags: foo
    value: "hi"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#;
        let err = parse_and_validate(when_yaml).unwrap_err();
        assert!(err.contains(
            "references list parameter 'tags'; list parameters cannot be used in when conditions"
        ));

        // 3.3 / 3.5: repeating container's own when: naming tags is refused (checked in outer scope)
        let container_own_when_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        when:
          tags: foo
        items:
          - type: text
            value: "{tags}"
            size: [10, 10]
            font_size: 8
"#;
        let err = parse_and_validate(container_own_when_yaml).unwrap_err();
        assert!(err.contains(
            "references list parameter 'tags'; list parameters cannot be used in when conditions"
        ));

        // 3.3: sibling of a repeating container naming the repeated list in when: is refused
        let sibling_when_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: text
            value: "{tags}"
            size: [10, 10]
            font_size: 8
      - type: text
        when:
          tags: foo
        value: "sibling"
        size: [10, 10]
        font_size: 8
"#;
        let err = parse_and_validate(sibling_when_yaml).unwrap_err();
        assert!(err.contains(
            "references list parameter 'tags'; list parameters cannot be used in when conditions"
        ));

        // 3.2 / 3.3: inside repeating container, when: { tags: foo } and bare {tags} are permitted
        let valid_repeat_yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: text
            when:
              tags: special
            value: "Special: {tags}"
            size: [10, 10]
            font_size: 8
"#;
        assert!(parse_and_validate(valid_repeat_yaml).is_ok());
    }

    #[test]
    fn repetition_load_refusals_quarantine_files() {
        // 2.7: Test each of the eight refusals (1.3, 1.4, 1.5, 2.2, 2.3, 2.4, 2.5, 2.6):
        // the file is quarantined, the service still starts and still serves every other template,
        // and the message names the offending item's layout path.
        let dir = temp_dir("repetition_load_refusals_quarantine");
        let valid_yaml = sample_yaml("valid");
        write_template(&dir, "valid.yaml", &valid_yaml);

        // 1. (1.3) Null repeat on packed container inside root
        let null_repeat_yaml = r#"
name: NullRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  tags: { type: list }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: null
        size: [10, 10]
        items: []
"#;
        write_template(&dir, "null_repeat.yaml", null_repeat_yaml);

        // 2. (1.4) Repeat on root container (unpacked)
        let unpacked_repeat_yaml = r#"
name: UnpackedRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  tags: { type: list }
layout:
  - type: container
    repeat: tags
    at: [0, 0]
    size: [50, 50]
    items: []
"#;
        write_template(&dir, "unpacked_repeat.yaml", unpacked_repeat_yaml);

        // 3. (1.5) Repeat on text (non-container)
        let text_repeat_yaml = r#"
name: TextRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  tags: { type: list }
layout:
  - type: text
    repeat: tags
    value: "hello"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#;
        write_template(&dir, "text_repeat.yaml", text_repeat_yaml);

        // 4. (2.2) Repeat naming undeclared parameter
        let undeclared_repeat_yaml = r#"
name: UndeclaredRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: missing_param
        size: [10, 10]
        items: []
"#;
        write_template(&dir, "undeclared_repeat.yaml", undeclared_repeat_yaml);

        // 5. (2.3) Repeat naming declared parameter of type string (non-list)
        let wrong_type_repeat_yaml = r#"
name: WrongTypeRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  title: { type: string }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: title
        size: [10, 10]
        items: []
"#;
        write_template(&dir, "wrong_type_repeat.yaml", wrong_type_repeat_yaml);

        // 6. (2.4) Nested repeat on same parameter
        let nested_same_repeat_yaml = r#"
name: NestedSameRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  tags: { type: list }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        flow: { direction: column }
        items:
          - type: container
            repeat: tags
            size: [10, 10]
            items: []
"#;
        write_template(&dir, "nested_same_repeat.yaml", nested_same_repeat_yaml);

        // 7. (2.5) {p:join(...)} inside repeat scope of p
        let join_in_repeat_yaml = r#"
name: JoinInRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  tags: { type: list }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        flow: { direction: column }
        items:
          - type: text
            value: "{tags:join(', ')}"
            size: [10, 5]
            font_size: 8
"#;
        write_template(&dir, "join_in_repeat.yaml", join_in_repeat_yaml);

        // 8. (2.6) Bare reader / format {tags:short_date} inside repeat scope of tags
        let format_in_repeat_yaml = r#"
name: FormatInRepeat
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  tags: { type: list }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        flow: { direction: column }
        items:
          - type: text
            value: "{tags:short_date}"
            size: [10, 5]
            font_size: 8
"#;
        write_template(&dir, "format_in_repeat.yaml", format_in_repeat_yaml);

        let registry = TemplateRegistry::load_from_dir(&dir).expect("registry load must not fail");
        assert_eq!(registry.len(), 1, "valid template must be served");
        assert!(registry.get("valid").is_some());

        let broken = registry.broken();
        assert_eq!(
            broken.len(),
            8,
            "all 8 refusal templates must be quarantined"
        );

        let find_broken = |path: &str| broken.iter().find(|b| b.path == path).unwrap();

        // 1.3: null repeat -> layout[0].items[0]
        let b1 = find_broken("null_repeat.yaml");
        assert!(b1.error.contains("repeat") && b1.error.contains("layout[0].items[0]"));

        // 1.4: unpacked container -> layout[0]
        let b2 = find_broken("unpacked_repeat.yaml");
        assert!(b2.error.contains("repeat") && b2.error.contains("layout[0]"));

        // 1.5: text repeat -> layout[0]
        let b3 = find_broken("text_repeat.yaml");
        assert!(b3.error.contains("layout[0]"));

        // 2.2: undeclared repeat -> layout[0].items[0]
        let b4 = find_broken("undeclared_repeat.yaml");
        assert!(b4.error.contains("missing_param") && b4.error.contains("layout[0].items[0]"));

        // 2.3: wrong type repeat -> layout[0].items[0]
        let b5 = find_broken("wrong_type_repeat.yaml");
        assert!(b5.error.contains("title") && b5.error.contains("layout[0].items[0]"));

        // 2.4: nested same repeat -> layout[0].items[0].items[0]
        let b6 = find_broken("nested_same_repeat.yaml");
        assert!(b6.error.contains("tags") && b6.error.contains("layout[0].items[0].items[0]"));

        // 2.5: join in repeat -> layout[0].items[0].items[0]
        let b7 = find_broken("join_in_repeat.yaml");
        assert!(b7.error.contains("tags:join") && b7.error.contains("layout[0].items[0].items[0]"));

        // 2.6: format in repeat -> layout[0].items[0].items[0]
        let b8 = find_broken("format_in_repeat.yaml");
        assert!(
            b8.error.contains("tags:short_date")
                && b8.error.contains("layout[0].items[0].items[0]")
        );
    }

    #[test]
    fn repeat_input_derivation() {
        // 5.4: template with repeat: tags reports tags in inputs.all with interpolated: true, control: list
        // child {tags} token does not add an extra input
        let yaml = r#"
name: T
unit: mm
dpi: 200
params:
  tags:
    type: list
  extra_a:
    type: string
  extra_b:
    type: string
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: text
            value: "Tag: {tags}"
            size: [10, 5]
            font_size: 8
          - type: text
            when:
              tags: A
            value: "Extra A: {extra_a}"
            size: [10, 5]
            font_size: 8
          - type: text
            when:
              tags: B
            value: "Extra B: {extra_b}"
            size: [10, 5]
            font_size: 8
"#;
        let template = parse_template_ok(yaml);
        let all = test_inputs_all(&template);
        let tag_input = all
            .iter()
            .find(|i| i.name == "tags")
            .expect("tags in inputs.all");
        assert_eq!(tag_input.control, InputControl::List);
        assert!(tag_input.interpolated);
        assert!(tag_input.required);

        // Per-label derive_inputs with tags: ["A", "B"] expands instances and evaluates when: gates
        let mut data = HashMap::new();
        data.insert("tags".to_string(), serde_json::json!(["A", "B"]));
        let derived = test_derive_inputs_for_label(&template, &data);
        let names: Vec<&str> = derived.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"tags"));
        assert!(names.contains(&"extra_a"));
        assert!(names.contains(&"extra_b"));

        // With tags: ["A"], only extra_a is active
        let mut data_a = HashMap::new();
        data_a.insert("tags".to_string(), serde_json::json!(["A"]));
        let derived_a = test_derive_inputs_for_label(&template, &data_a);
        let names_a: Vec<&str> = derived_a.iter().map(|i| i.name.as_str()).collect();
        assert!(names_a.contains(&"tags"));
        assert!(names_a.contains(&"extra_a"));
        assert!(!names_a.contains(&"extra_b"));

        // With tags: ["C"], neither extra_a nor extra_b is active
        let mut data_c = HashMap::new();
        data_c.insert("tags".to_string(), serde_json::json!(["C"]));
        let derived_c = test_derive_inputs_for_label(&template, &data_c);
        let names_c: Vec<&str> = derived_c.iter().map(|i| i.name.as_str()).collect();
        assert!(names_c.contains(&"tags"));
        assert!(!names_c.contains(&"extra_a"));
        assert!(!names_c.contains(&"extra_b"));
    }
}
