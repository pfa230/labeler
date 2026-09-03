mod helpers;

pub const MAX_RENDER_DPI: u32 = 1200;

use crate::errors::AppError;
use crate::models::{
    resolve_coord, DynamicDimension, Fit, LabelInput, Layout, LayoutItem, ParamSpec, Placement,
    Point, Position, Rotation, Shape, Stroke, TemplateFormat,
};
use crate::reason::Reason;
use crate::templates::{TemplateContent, TemplateDefinition};
use chrono::{DateTime, Local};
use helpers::{
    assets_root, binarize_rgba, build_qr_svg, escape_typst_string, format_length, interpolate,
    parse_image_data_uri, resolve_dimension, resolve_dynamic_value_color,
    resolve_dynamic_value_f32, resolve_dynamic_value_u16, resolve_image_asset, to_page_coords,
    typst_alignment, typst_font_options,
};

pub(crate) use helpers::value_to_string;
use serde_json::Value as JsonValue;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Strict,
    Lenient,
}

#[derive(Debug, Clone)]
pub struct ResolvedParams {
    pub data: HashMap<String, JsonValue>,
    pub instants: BTreeMap<String, DateTime<Local>>,
}

enum CoercedParam {
    Value(JsonValue),
    Datetime(DateTime<Local>, String),
}

fn coerce_param_value(
    val: &JsonValue,
    param_type: &crate::models::ParamType,
) -> Result<CoercedParam, String> {
    match param_type {
        crate::models::ParamType::Datetime { .. } => {
            let dt_str = match val {
                JsonValue::String(s) => s.clone(),
                other => value_to_string(other),
            };
            let trimmed = dt_str.trim();
            match crate::datetime_fmt::parse_datetime_override(trimmed) {
                Ok(dt) => {
                    let formatted = crate::datetime_fmt::format_now(
                        crate::datetime_fmt::BARE_DATETIME_FORMAT,
                        dt,
                    );
                    Ok(CoercedParam::Datetime(dt, formatted))
                }
                Err(_) => Err(trimmed.to_string()),
            }
        }
        crate::models::ParamType::Enum { values } => {
            let s = match val {
                JsonValue::String(s) => s.clone(),
                other => value_to_string(other),
            };
            if values.contains(&s) {
                Ok(CoercedParam::Value(JsonValue::String(s)))
            } else {
                Err(s)
            }
        }
        crate::models::ParamType::Boolean => {
            let b_res = match val {
                JsonValue::Bool(b) => Ok(*b),
                JsonValue::String(s) => {
                    let trimmed = s.trim();
                    if trimmed == "true" || trimmed == "1" {
                        Ok(true)
                    } else if trimmed == "false" || trimmed == "0" {
                        Ok(false)
                    } else {
                        Err(())
                    }
                }
                JsonValue::Number(n) => {
                    if n.as_i64() == Some(1) {
                        Ok(true)
                    } else if n.as_i64() == Some(0) {
                        Ok(false)
                    } else {
                        Err(())
                    }
                }
                _ => Err(()),
            };
            match b_res {
                Ok(b) => Ok(CoercedParam::Value(JsonValue::Bool(b))),
                Err(()) => Err(value_to_string(val)),
            }
        }
        crate::models::ParamType::Integer => {
            let i_res = match val {
                JsonValue::Number(n) => n
                    .as_i64()
                    .or_else(|| n.as_f64().map(|f| f.round() as i64))
                    .ok_or(()),
                JsonValue::String(s) => s.trim().parse::<i64>().map_err(|_| ()),
                _ => Err(()),
            };
            match i_res {
                Ok(i) => Ok(CoercedParam::Value(serde_json::json!(i))),
                Err(()) => Err(value_to_string(val)),
            }
        }
        crate::models::ParamType::Length | crate::models::ParamType::Number => {
            let f_res = match val {
                JsonValue::Number(n) => n.as_f64().map(|f| f as f32).ok_or(()),
                JsonValue::String(s) => {
                    let trimmed = s.trim();
                    let num_str = trimmed
                        .strip_suffix("mm")
                        .or_else(|| trimmed.strip_suffix("in"))
                        .unwrap_or(trimmed);
                    num_str.trim().parse::<f32>().map_err(|_| ())
                }
                _ => Err(()),
            };
            match f_res {
                Ok(f) => Ok(CoercedParam::Value(serde_json::json!(f))),
                Err(()) => Err(value_to_string(val)),
            }
        }
        crate::models::ParamType::String { .. } => match val {
            JsonValue::String(s) => Ok(CoercedParam::Value(JsonValue::String(s.clone()))),
            JsonValue::Array(_) => Err("an array is not a valid string".to_string()),
            other => Ok(CoercedParam::Value(JsonValue::String(value_to_string(
                other,
            )))),
        },
        crate::models::ParamType::List => match val {
            JsonValue::Array(arr) => {
                let mut strings = Vec::with_capacity(arr.len());
                for (idx, elem) in arr.iter().enumerate() {
                    match elem {
                        JsonValue::String(s) => strings.push(JsonValue::String(s.clone())),
                        _ => return Err(format!("position {idx}")),
                    }
                }
                Ok(CoercedParam::Value(JsonValue::Array(strings)))
            }
            _ => Err("not an array".to_string()),
        },
    }
}

/// Returns the parameter names that match no key of `template.params`,
/// sorted ascending by Unicode code point (`str`'s `Ord`), empty when there are none.
pub fn unknown_param_names<'a>(
    template: &TemplateContent,
    names: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut unknown: Vec<String> = names
        .filter(|name| !template.params.contains_key(*name))
        .map(String::from)
        .collect();
    unknown.sort();
    unknown.dedup();
    unknown
}

/// Validates that every key in a label's `data` map names a declared parameter of `template`.
/// Returns an `InvalidRequest` error with reason `data_key_unknown` naming all unrecognized keys
/// (sorted ascending) and the template id if any are found.
pub fn validate_label_data_keys(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
) -> Result<(), AppError> {
    let unknown = unknown_param_names(template, data.keys().map(|k| k.as_str()));
    if unknown.is_empty() {
        Ok(())
    } else {
        let keys_str = unknown
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let noun = if unknown.len() == 1 { "key" } else { "keys" };
        let verb = if unknown.len() == 1 {
            "is not a declared parameter"
        } else {
            "are not declared parameters"
        };
        Err(AppError::invalid_request(
            Reason::DataKeyUnknown,
            format!(
                "data {noun} {keys_str} {verb} of template '{}'",
                template.id
            ),
        ))
    }
}

/// Resolve parameters by merging request data and template parameter defaults.
pub fn resolve_parameters(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    variables: Option<&BTreeMap<String, String>>,
    datetime: Option<&crate::datetime_fmt::DateTimeResolver>,
) -> Result<ResolvedParams, AppError> {
    resolve_parameters_mode(template, data, variables, datetime, ResolveMode::Strict)
}

pub fn resolve_parameters_mode(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    variables: Option<&BTreeMap<String, String>>,
    datetime: Option<&crate::datetime_fmt::DateTimeResolver>,
    mode: ResolveMode,
) -> Result<ResolvedParams, AppError> {
    let mut resolved = data.clone();
    let mut instants = BTreeMap::new();

    for (name, spec) in &template.params {
        match &spec.param_type {
            crate::models::ParamType::Datetime { .. } => {
                let raw_val = resolved.get(name);
                match raw_val {
                    None | Some(JsonValue::Null) => {
                        resolve_and_coerce_default(
                            name,
                            spec,
                            variables,
                            datetime,
                            &mut instants,
                            &mut resolved,
                            mode,
                        )?;
                    }
                    Some(JsonValue::String(s)) if s.trim().is_empty() => {
                        resolve_and_coerce_default(
                            name,
                            spec,
                            variables,
                            datetime,
                            &mut instants,
                            &mut resolved,
                            mode,
                        )?;
                    }
                    Some(val) => match coerce_param_value(val, &spec.param_type) {
                        Ok(CoercedParam::Datetime(dt, formatted)) => {
                            instants.insert(name.clone(), dt);
                            resolved.insert(name.clone(), JsonValue::String(formatted));
                        }
                        Ok(CoercedParam::Value(_)) => unreachable!(),
                        Err(bad_str) => {
                            if mode == ResolveMode::Lenient {
                                resolve_and_coerce_default(
                                    name,
                                    spec,
                                    variables,
                                    datetime,
                                    &mut instants,
                                    &mut resolved,
                                    mode,
                                )?;
                            } else {
                                return Err(AppError::invalid_request(
                                    Reason::DatetimeParamInvalid,
                                    format!(
                                        "Invalid value for datetime parameter '{name}': {bad_str}"
                                    ),
                                ));
                            }
                        }
                    },
                }
            }
            crate::models::ParamType::List => {
                let raw_val = resolved.get(name);
                match raw_val {
                    None | Some(JsonValue::Null) => {
                        resolve_and_coerce_default(
                            name,
                            spec,
                            variables,
                            datetime,
                            &mut instants,
                            &mut resolved,
                            mode,
                        )?;
                    }
                    Some(val) => match coerce_param_value(val, &spec.param_type) {
                        Ok(CoercedParam::Value(coerced)) => {
                            resolved.insert(name.clone(), coerced);
                        }
                        Ok(CoercedParam::Datetime(..)) => unreachable!(),
                        Err(bad_str) => {
                            if mode == ResolveMode::Lenient {
                                resolve_and_coerce_default(
                                    name,
                                    spec,
                                    variables,
                                    datetime,
                                    &mut instants,
                                    &mut resolved,
                                    mode,
                                )?;
                            } else if let Some(pos) = bad_str.strip_prefix("position ") {
                                return Err(AppError::invalid_request(
                                    Reason::RequestBodyInvalid,
                                    format!("element at position {pos} of parameter '{name}' must be a string"),
                                ));
                            } else {
                                return Err(AppError::invalid_request(
                                    Reason::RequestBodyInvalid,
                                    format!("parameter '{name}' is not a valid list"),
                                ));
                            }
                        }
                    },
                }
            }
            _ => {
                if let Some(val) = resolved.get(name) {
                    match coerce_param_value(val, &spec.param_type) {
                        Ok(CoercedParam::Value(coerced)) => {
                            resolved.insert(name.clone(), coerced);
                        }
                        Ok(CoercedParam::Datetime(..)) => unreachable!(),
                        Err(bad_str) => {
                            if mode == ResolveMode::Lenient {
                                resolve_and_coerce_default(
                                    name,
                                    spec,
                                    variables,
                                    datetime,
                                    &mut instants,
                                    &mut resolved,
                                    mode,
                                )?;
                            } else {
                                match &spec.param_type {
                                    crate::models::ParamType::Enum { values } => {
                                        let mut selection = BTreeMap::new();
                                        selection.insert(name.clone(), bad_str);
                                        let mut allowed = BTreeMap::new();
                                        allowed.insert(name.clone(), values.clone());
                                        return Err(AppError::invalid_enum_value(
                                            &selection, &allowed,
                                        ));
                                    }
                                    crate::models::ParamType::Boolean => {
                                        return Err(AppError::invalid_request(
                                            Reason::RequestBodyInvalid,
                                            format!("parameter '{name}' is not a valid boolean"),
                                        ));
                                    }
                                    crate::models::ParamType::Integer => {
                                        return Err(AppError::invalid_request(
                                            Reason::RequestBodyInvalid,
                                            format!("parameter '{name}' is not a valid integer"),
                                        ));
                                    }
                                    crate::models::ParamType::Length
                                    | crate::models::ParamType::Number => {
                                        return Err(AppError::invalid_request(
                                            Reason::RequestBodyInvalid,
                                            format!("parameter '{name}' is not a valid number"),
                                        ));
                                    }
                                    crate::models::ParamType::String { .. } => {
                                        return Err(AppError::invalid_request(
                                            Reason::RequestBodyInvalid,
                                            format!("parameter '{name}' is not a valid string"),
                                        ));
                                    }
                                    crate::models::ParamType::Datetime { .. }
                                    | crate::models::ParamType::List => unreachable!(),
                                }
                            }
                        }
                    }
                } else {
                    resolve_and_coerce_default(
                        name,
                        spec,
                        variables,
                        datetime,
                        &mut instants,
                        &mut resolved,
                        mode,
                    )?;
                }
            }
        }
    }

    Ok(ResolvedParams {
        data: resolved,
        instants,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDefaultFailure {
    pub param: String,
    pub message: String,
    pub token: Option<String>,
    pub value: Option<String>,
}

impl ParamDefaultFailure {
    pub fn new(param: impl Into<String>, token: Option<String>, value: Option<String>) -> Self {
        let param = param.into();
        let message = match (&token, &value) {
            (Some(tok), Some(val)) => {
                format!("Failed to resolve default for parameter '{param}': token '{tok}' resolved to invalid value '{val}'")
            }
            (Some(tok), None) => {
                format!("Failed to resolve default for parameter '{param}': token '{tok}' could not be resolved")
            }
            (None, Some(val)) => {
                format!(
                    "Failed to resolve default for parameter '{param}': value '{val}' is invalid"
                )
            }
            (None, None) => {
                format!("Failed to resolve default for parameter '{param}'")
            }
        };
        Self {
            param,
            message,
            token,
            value,
        }
    }

    pub fn to_error_report(&self) -> crate::models::ParamDefaultError {
        crate::models::ParamDefaultError {
            reason: crate::reason::Reason::ParamDefaultUnresolvable
                .as_slug()
                .to_string(),
            message: self.message.clone(),
            token: self.token.clone(),
            value: self.value.clone(),
        }
    }
}

pub fn json_to_param_value(val: &serde_json::Value) -> crate::models::ParamValue {
    match val {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                crate::models::ParamValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                crate::models::ParamValue::Float(f as f32)
            } else {
                crate::models::ParamValue::Float(0.0)
            }
        }
        serde_json::Value::Bool(b) => crate::models::ParamValue::Boolean(*b),
        serde_json::Value::Array(arr) => {
            let mut list = Vec::with_capacity(arr.len());
            for v in arr {
                match v {
                    serde_json::Value::String(s) => list.push(s.clone()),
                    other => panic!(
                        "json_to_param_value: non-string list element after coercion: {other:?}"
                    ),
                }
            }
            crate::models::ParamValue::List(list)
        }
        serde_json::Value::String(s) => crate::models::ParamValue::String(s.clone()),
        other => crate::models::ParamValue::String(other.to_string()),
    }
}

fn resolve_parameter_default_candidate(
    name: &str,
    spec: &ParamSpec,
    variables: Option<&BTreeMap<String, String>>,
    datetime: Option<&crate::datetime_fmt::DateTimeResolver>,
    mode: ResolveMode,
) -> Result<Option<CoercedParam>, Result<ParamDefaultFailure, String>> {
    let default_val = match &spec.default {
        Some(v) => v,
        None => return Ok(None),
    };

    let candidate = match default_val {
        crate::models::ParamValue::String(s) => {
            if s.contains('{') || s.contains('}') {
                if let (Some(vars), Some(dt)) = (variables, datetime) {
                    match helpers::interpolate(s, &HashMap::new(), vars, dt, None) {
                        Ok(interpolated) => JsonValue::String(interpolated),
                        Err(err) => {
                            if mode == ResolveMode::Lenient {
                                return Ok(None);
                            } else {
                                let token = match err.details() {
                                    Some(serde_json::Value::Object(map)) => {
                                        map.get("field").and_then(|v| v.as_str()).map(String::from)
                                    }
                                    _ => None,
                                };
                                return Err(Ok(ParamDefaultFailure::new(name, token, None)));
                            }
                        }
                    }
                } else if mode == ResolveMode::Lenient {
                    return Ok(None);
                } else {
                    return Err(Err(format!(
                        "strict resolution called without required variables or datetime context for tokened default of parameter '{name}'"
                    )));
                }
            } else {
                JsonValue::String(s.clone())
            }
        }
        crate::models::ParamValue::Float(f) => serde_json::json!(f),
        crate::models::ParamValue::Integer(i) => serde_json::json!(i),
        crate::models::ParamValue::Boolean(b) => JsonValue::Bool(*b),
        crate::models::ParamValue::List(l) => serde_json::json!(l),
    };

    match coerce_param_value(&candidate, &spec.param_type) {
        Ok(coerced) => Ok(Some(coerced)),
        Err(bad_str) => {
            if mode == ResolveMode::Lenient {
                Ok(None)
            } else {
                Err(Ok(ParamDefaultFailure::new(name, None, Some(bad_str))))
            }
        }
    }
}

pub fn resolve_parameter_default(
    name: &str,
    declared_default: &crate::models::ParamValue,
    param_type: &crate::models::ParamType,
    variables: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<serde_json::Value, ParamDefaultFailure> {
    // Build a temporary ParamSpec to reuse the candidate logic without reimplementing
    // interpolation/coercion. The only caller is resolve_declared_defaults, which has
    // already unwrapped the declared default, so this function cannot be called for a
    // parameter that declares none; the type makes that a compile-time property of the
    // call site rather than a runtime panic on `spec.default.is_none()`.
    let spec = ParamSpec {
        param_type: param_type.clone(),
        default: Some(declared_default.clone()),
        min: None,
        max: None,
        description: None,
    };
    match resolve_parameter_default_candidate(
        name,
        &spec,
        Some(variables),
        Some(datetime),
        ResolveMode::Strict,
    ) {
        Ok(Some(CoercedParam::Datetime(_dt, formatted))) => Ok(JsonValue::String(formatted)),
        Ok(Some(CoercedParam::Value(coerced))) => Ok(coerced),
        Ok(None) => unreachable!(
            "resolve_parameter_default: candidate returned None for declared default of '{name}' in Strict mode"
        ),
        Err(Ok(failure)) => Err(failure),
        Err(Err(internal_msg)) => Err(ParamDefaultFailure::new(name, None, Some(internal_msg))),
    }
}

pub fn resolve_declared_defaults(
    template: &TemplateContent,
    variables: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> crate::models::ResolvedDefaults {
    let mut map = BTreeMap::new();
    for (name, spec) in &template.params {
        if let Some(declared) = spec.default.as_ref() {
            match resolve_parameter_default(name, declared, &spec.param_type, variables, datetime) {
                Ok(val) => {
                    map.insert(
                        name.clone(),
                        crate::models::ParamDefaultReport::Resolved {
                            resolved: json_to_param_value(&val),
                        },
                    );
                }
                Err(failure) => {
                    map.insert(
                        name.clone(),
                        crate::models::ParamDefaultReport::Error {
                            error: failure.to_error_report(),
                        },
                    );
                }
            }
        }
    }
    map
}

fn resolve_and_coerce_default(
    name: &str,
    spec: &ParamSpec,
    variables: Option<&BTreeMap<String, String>>,
    datetime: Option<&crate::datetime_fmt::DateTimeResolver>,
    instants: &mut BTreeMap<String, DateTime<Local>>,
    resolved: &mut HashMap<String, JsonValue>,
    mode: ResolveMode,
) -> Result<(), AppError> {
    match resolve_parameter_default_candidate(name, spec, variables, datetime, mode) {
        Ok(Some(CoercedParam::Datetime(dt, formatted))) => {
            instants.insert(name.to_string(), dt);
            resolved.insert(name.to_string(), JsonValue::String(formatted));
            Ok(())
        }
        Ok(Some(CoercedParam::Value(coerced))) => {
            resolved.insert(name.to_string(), coerced);
            Ok(())
        }
        Ok(None) => {
            resolved.remove(name);
            Ok(())
        }
        Err(Ok(failure)) => Err(AppError::param_default_unresolvable(&failure)),
        Err(Err(internal_msg)) => Err(AppError::internal(internal_msg)),
    }
}

fn check_dimension_limit(
    val: f32,
    unit: &str,
    max_dim_mm: f32,
    label: &str,
) -> Result<(), AppError> {
    let val_mm = if unit == "in" { val * 25.4 } else { val };
    if !val_mm.is_finite() || val_mm <= 0.0 || val_mm > max_dim_mm {
        return Err(AppError::unsupported_layout_item(
            Reason::DimensionExceedsLimit,
            format!("{label} {val} {unit} exceeds limit of {max_dim_mm} mm"),
        ));
    }
    Ok(())
}

/// Typst 0.15's `typst_render::render` takes `&RenderOptions` instead of a bare pixels-per-point
/// scalar; build one carrying the requested scale (bleed off, matching the previous behavior).
/// Wrap `body` in a `#pad` at the aligned edge. Typst's `#pad` grows the frame and translates the
/// child inward, so aligning the padded block insets the content by exactly `pad` — which is how ink
/// falling outside the cap-height/baseline line box (accents above, descenders below) stays inside
/// the clipped slot (#124). Center pads nothing: centring the metric box already splits the slack,
/// so placement needs no inset (ADR-0084).
fn pad_block(body: &str, pad: f32, vertical: crate::models::VerticalAlign) -> String {
    use crate::models::VerticalAlign;
    if pad <= 0.0 {
        return body.to_string();
    }
    match vertical {
        VerticalAlign::Top => format!("#pad(top: {pad:.2}pt)[{body}]"),
        VerticalAlign::Bottom => format!("#pad(bottom: {pad:.2}pt)[{body}]"),
        VerticalAlign::Center => body.to_string(),
    }
}

fn render_options(pixels_per_point: f32) -> typst_render::RenderOptions {
    typst_render::RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(pixels_per_point as f64),
        ..Default::default()
    }
}

#[derive(Default)]
pub(crate) struct ImageCollector {
    files: Vec<(String, Vec<u8>)>,
}

impl ImageCollector {
    fn add(&mut self, ext: &str, bytes: Vec<u8>) -> String {
        let vpath = format!("/labeler-img-{}.{}", self.files.len(), ext);
        self.files.push((vpath.clone(), bytes));
        vpath
    }
}

fn compile_paged(source: String, files: Vec<(String, Vec<u8>)>) -> Result<PagedDocument, AppError> {
    let mut builder = TypstEngine::builder()
        .main_file(source)
        .search_fonts_with(typst_font_options());
    if !files.is_empty() {
        builder = builder
            .with_static_file_resolver(files.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
    }
    let engine = builder.build();
    let warned = engine.compile::<PagedDocument>();
    warned.output.map_err(|err| {
        AppError::render_failed(
            Reason::TypstCompileFailed,
            format!("typst compile failed: {err}"),
        )
    })
}

fn compile_single_doc(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
) -> Result<PagedDocument, AppError> {
    if !matches!(template.format, TemplateFormat::Single { .. }) {
        return Err(AppError::unsupported_format(
            "render_label only supports single format",
        ));
    }
    compile_label_doc(template, data, env)
}

struct CompiledSource {
    source: String,
    files: Vec<(String, Vec<u8>)>,
}

fn compile_label_source(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
) -> Result<CompiledSource, AppError> {
    let unit = &template.unit;
    let resolved = resolve_parameters(template, data, Some(env.settings), Some(env.datetime))?;
    let resolved_data = &resolved.data;
    let items = select_layout_items(template)?;
    let images = RefCell::new(ImageCollector::default());

    let max_dim_mm = crate::settings::resolve_max_label_dimension_mm_from(
        env.settings
            .get(crate::settings::MAX_LABEL_DIMENSION_MM)
            .cloned(),
    )
    .unwrap_or(crate::settings::DEFAULT_MAX_LABEL_DIMENSION_MM);

    // Resolve initial width/height; Dynamic single may be overridden after measurement.
    let (mut width_units, height_units) = match &template.format {
        TemplateFormat::Single { width, height, .. } => (
            resolve_dimension(width, resolved_data)?,
            resolve_dimension(height, resolved_data)?,
        ),
        TemplateFormat::Sheet {
            label_width,
            label_height,
            ..
        } => (*label_width, *label_height),
    };

    check_dimension_limit(height_units, unit, max_dim_mm, "height")?;

    let geometry_values = render_geometry_values(resolved_data, template);

    let measured: Vec<Measured>;

    if let TemplateFormat::Single {
        width: DynamicDimension::Dynamic { min, max },
        ..
    } = &template.format
    {
        let max_w = max
            .as_ref()
            .map(|v| resolve_dynamic_value_f32(v, resolved_data))
            .transpose()?
            .ok_or_else(|| AppError::unsupported_format("dynamic single width requires max"))?;
        let min_w = min
            .as_ref()
            .map(|v| resolve_dynamic_value_f32(v, resolved_data))
            .transpose()?
            .ok_or_else(|| AppError::unsupported_format("dynamic single width requires min"))?;

        check_dimension_limit(min_w, unit, max_dim_mm, "width min")?;
        check_dimension_limit(max_w, unit, max_dim_mm, "width max")?;

        let probe = RenderContext::new(unit, template.dpi, resolved_data, env, &images)
            .with_instants(&resolved.instants);
        let (m_tree, root_w_req) = probe.measure_items(
            items,
            (max_w, height_units),
            [false, true],
            &geometry_values,
            "layout",
        )?;
        width_units = root_w_req.clamp(min_w, max_w);
        check_dimension_limit(width_units, unit, max_dim_mm, "width")?;
        measured = m_tree;
    } else {
        check_dimension_limit(width_units, unit, max_dim_mm, "width")?;
        let probe = RenderContext::new(unit, template.dpi, resolved_data, env, &images)
            .with_instants(&resolved.instants);
        let (m_tree, _) = probe.measure_items(
            items,
            (width_units, height_units),
            [true, true],
            &geometry_values,
            "layout",
        )?;
        measured = m_tree;
    }

    let mut source = String::new();
    let page_width = format_length(width_units, unit)?;
    let page_height = format_length(height_units, unit)?;
    writeln!(
        source,
        "#set page(width: {page_width}, height: {page_height}, margin: 0{unit})"
    )
    .map_err(|err| {
        AppError::render_failed(
            Reason::TypstSourceBuildFailed,
            format!("failed to build typst source: {err}"),
        )
    })?;
    writeln!(source, "#set text(font: \"Inter\")").map_err(|err| {
        AppError::render_failed(
            Reason::TypstSourceBuildFailed,
            format!("failed to build typst source: {err}"),
        )
    })?;

    let context = RenderContext::new(unit, template.dpi, resolved_data, env, &images)
        .with_instants(&resolved.instants);
    let body = context.render_items(
        items,
        &measured,
        (width_units, height_units),
        &geometry_values,
        None,
        "layout",
    )?;
    source.push_str(&body);

    tracing::debug!(name = %template.name, typst = %source, "render typst source");
    Ok(CompiledSource {
        source,
        files: images.into_inner().files,
    })
}

/// Compile a single label for any template: a `Single` uses its width/height; a `Sheet`
/// renders one slot at label_width/label_height. Shared by `compile_single_doc` (after its
/// Single-only guard) and the thumbnail path.
fn compile_label_doc(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
) -> Result<PagedDocument, AppError> {
    let compiled = compile_label_source(template, data, env)?;
    compile_paged(compiled.source, compiled.files)
}

/// Render a single representative label to PNG. For sheets, renders one slot.
pub fn render_thumbnail_png(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let doc = compile_label_doc(template, data, &env)?;
    let page = doc.pages().first().ok_or_else(|| {
        AppError::render_failed(Reason::TypstNoPages, "typst did not produce any pages")
    })?;
    let pixmap = typst_render::render(page, &render_options(template.dpi as f32 / 72.0));
    pixmap.encode_png().map_err(|err| {
        AppError::render_failed(
            Reason::PngEncodeFailed,
            format!("failed to encode png: {err}"),
        )
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorMode {
    #[default]
    Color,
    BiLevel,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ImageRenderOptions {
    pub color_mode: ColorMode,
    pub resolution_dpi: Option<u32>,
}

pub fn render_single_label(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    render_single_label_image(
        template,
        data,
        settings,
        datetime,
        ImageRenderOptions::default(),
    )
}

pub fn render_single_label_image(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
    opts: ImageRenderOptions,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let doc = compile_single_doc(template, data, &env)?;
    let page = doc.pages().first().ok_or_else(|| {
        AppError::render_failed(Reason::TypstNoPages, "typst did not produce any pages")
    })?;

    let dpi = opts.resolution_dpi.unwrap_or(template.dpi);
    let mut pixmap = typst_render::render(page, &render_options(dpi as f32 / 72.0));
    if opts.color_mode == ColorMode::BiLevel {
        binarize_rgba(pixmap.data_mut());
    }
    pixmap.encode_png().map_err(|err| {
        AppError::render_failed(
            Reason::PngEncodeFailed,
            format!("failed to encode png: {err}"),
        )
    })
}

pub fn render_single_label_pdf(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let doc = compile_single_doc(template, data, &env)?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(|err| {
        AppError::render_failed(
            Reason::PdfEncodeFailed,
            format!("failed to encode pdf: {err:?}"),
        )
    })
}

pub fn render_sheet_pages(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    start_slot: u32,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let TemplateFormat::Sheet {
        paper_width,
        paper_height,
        label_width,
        label_height,
        positions,
    } = &template.format
    else {
        return Err(AppError::unsupported_format(
            "render_sheet_pages only supports sheet format",
        ));
    };

    let start_slot = start_slot as usize;
    if start_slot >= positions.len() && !labels.is_empty() {
        return Err(AppError::invalid_request(
            Reason::StartSlotOutOfRange,
            "start_slot is out of range",
        ));
    }

    let page_width_units = *paper_width;
    let page_height_units = *paper_height;
    let unit = &template.unit;
    let items = select_layout_items(template)?;

    let max_dim_mm = crate::settings::resolve_max_label_dimension_mm_from(
        env.settings
            .get(crate::settings::MAX_LABEL_DIMENSION_MM)
            .cloned(),
    )
    .unwrap_or(crate::settings::DEFAULT_MAX_LABEL_DIMENSION_MM);

    check_dimension_limit(page_width_units, unit, max_dim_mm, "paper width")?;
    check_dimension_limit(page_height_units, unit, max_dim_mm, "paper height")?;
    check_dimension_limit(*label_width, unit, max_dim_mm, "label width")?;
    check_dimension_limit(*label_height, unit, max_dim_mm, "label height")?;

    let slots_per_page = positions.len();
    let mut placements: Vec<(usize, usize)> = Vec::with_capacity(labels.len());
    let mut slot = start_slot;
    let mut page = 0usize;
    for _ in labels {
        if slot >= slots_per_page {
            page += 1;
            slot = 0;
        }
        placements.push((page, slot));
        slot += 1;
    }
    let page_count = placements.last().map(|(p, _)| p + 1).unwrap_or(1);

    let images = RefCell::new(ImageCollector::default());

    let mut rendered: Vec<String> = Vec::with_capacity(labels.len());
    let mut failures: Vec<crate::errors::BatchFailure> = Vec::new();
    for (idx, lbl) in labels.iter().enumerate() {
        if let Err(err) = validate_label_data_keys(template, &lbl.data) {
            failures.push(crate::errors::BatchFailure {
                index: idx,
                code: err.code(),
                reason: err.reason(),
                message: err.message_text(),
            });
            rendered.push(String::new());
            continue;
        }
        let resolved =
            match resolve_parameters(template, &lbl.data, Some(env.settings), Some(env.datetime)) {
                Ok(data) => data,
                Err(err) => {
                    failures.push(crate::errors::BatchFailure {
                        index: idx,
                        code: err.code(),
                        reason: err.reason(),
                        message: err.message_text(),
                    });
                    rendered.push(String::new());
                    continue;
                }
            };
        let geometry_values = render_geometry_values(&resolved.data, template);
        let context = RenderContext::new(unit, template.dpi, &resolved.data, &env, &images)
            .with_instants(&resolved.instants);
        let (measured, _) = match context.measure_items(
            items,
            (*label_width, *label_height),
            [true, true],
            &geometry_values,
            "layout",
        ) {
            Ok(m) => m,
            Err(err) => {
                failures.push(crate::errors::BatchFailure {
                    index: idx,
                    code: err.code(),
                    reason: err.reason(),
                    message: err.message_text(),
                });
                rendered.push(String::new());
                continue;
            }
        };
        match context.render_items(
            items,
            &measured,
            (*label_width, *label_height),
            &geometry_values,
            None,
            "layout",
        ) {
            Ok(content) => rendered.push(content),
            Err(err) => {
                failures.push(crate::errors::BatchFailure {
                    index: idx,
                    code: err.code(),
                    reason: err.reason(),
                    message: err.message_text(),
                });
                rendered.push(String::new());
            }
        }
    }
    if !failures.is_empty() {
        return Err(AppError::batch_invalid(failures));
    }

    let mut source = String::new();
    let page_w = format_length(page_width_units, unit)?;
    let page_h = format_length(page_height_units, unit)?;
    for p in 0..page_count {
        if p == 0 {
            writeln!(
                source,
                "#set page(width: {page_w}, height: {page_h}, margin: 0{unit})"
            )
            .map_err(|err| {
                AppError::render_failed(
                    Reason::TypstSourceBuildFailed,
                    format!("failed to build typst source: {err}"),
                )
            })?;
            writeln!(source, "#set text(font: \"Inter\")").map_err(|err| {
                AppError::render_failed(
                    Reason::TypstSourceBuildFailed,
                    format!("failed to build typst source: {err}"),
                )
            })?;
        } else {
            writeln!(source, "#pagebreak()").map_err(|err| {
                AppError::render_failed(
                    Reason::TypstSourceBuildFailed,
                    format!("failed to build typst source: {err}"),
                )
            })?;
        }
        for (idx, (lp, ls)) in placements.iter().enumerate() {
            if *lp != p {
                continue;
            }
            let point = positions[*ls].point();
            let top = point.y + *label_height;
            let dx = format_length(point.x, unit)?;
            let dy = format_length(page_height_units - top, unit)?;
            let bw = format_length(*label_width, unit)?;
            let bh = format_length(*label_height, unit)?;
            writeln!(
                source,
                "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {bw}, height: {bh}, clip: true)[{}]]",
                rendered[idx]
            )
            .map_err(|err| {
                AppError::render_failed(
                Reason::TypstSourceBuildFailed,
                format!("failed to build typst source: {err}"),
            )
            })?;
        }
    }
    tracing::debug!(name = %template.name, typst = %source, "render typst source");

    let doc = compile_paged(source, images.into_inner().files)?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(|err| {
        AppError::render_failed(
            Reason::PdfEncodeFailed,
            format!("failed to encode pdf: {err:?}"),
        )
    })
}

/// Count rendered PDF pages by counting "/Type /Page" objects (excluding the "/Type /Pages" tree
/// node). Used by pagination tests.
pub fn count_pdf_pages(pdf: &[u8]) -> usize {
    // typst-pdf 0.15 serializes dictionary keys without whitespace (`/Type/Page`, `/Type/Pages`).
    let needle = b"/Type/Page";
    let mut count = 0usize;
    let mut i = 0;
    while let Some(pos) = pdf[i..].windows(needle.len()).position(|w| w == needle) {
        let at = i + pos;
        let after = at + needle.len();
        if pdf.get(after) != Some(&b's') {
            count += 1;
        }
        i = after;
    }
    count
}

fn select_layout_items(template: &TemplateContent) -> Result<&[LayoutItem], AppError> {
    match &template.layout {
        Layout::Items(items) => Ok(items.as_slice()),
    }
}

#[derive(Debug, Clone)]
pub struct Measured {
    pub intrinsic: [Option<f32>; 2],
    pub text: Option<helpers::TextFit>,
    pub children: Vec<Measured>,
}

/// What the intrinsic dispatch needs to answer for one item: the box its content is measured
/// against, which axes asked, and — for a container — the children already measured against the
/// frame it gives them.
struct IntrinsicInput<'a> {
    item: &'a LayoutItem,
    measure_box: (f32, f32),
    demands: [bool; 2],
    children: &'a [Measured],
    child_frame: (f32, f32),
    geometry_values: &'a HashMap<String, f32>,
    path: &'a str,
}

/// How render words a resolver [`crate::resolver::Violation`]. Render reports reason slugs, so the
/// mapping is a table of reasons and nothing else; the rule that produced the violation lives in
/// the resolver.
fn violation_error(violation: crate::resolver::Violation, path: &str) -> AppError {
    use crate::resolver::Violation;
    let (reason, message) = match violation {
        Violation::AnchorBeforeFrame { .. } => (
            Reason::CoordOutOfFrame,
            format!("at {path}: coordinate resolves outside frame"),
        ),
        Violation::AuthoredExtentNotPositive { .. } => (
            Reason::SizeInvalid,
            format!("at {path}: authored size must be greater than 0"),
        ),
        Violation::ExtentInverted { .. } => (
            Reason::EdgeRectInverted,
            format!("at {path}: to must be above and to the right of at"),
        ),
        // A `to` never reaches here: `place` refuses a non-positive one as inverted first.
        Violation::ExtentNegative { .. } => (
            Reason::SizeInvalid,
            format!("at {path}: inverted or negative resolved size"),
        ),
        Violation::AnchorBeyondFrame { .. } | Violation::ExtentBeyondFrame { .. } => (
            Reason::ItemOutOfFrame,
            format!("at {path}: item resolves outside frame bounds"),
        ),
    };
    AppError::unsupported_layout_item(reason, message)
}

fn render_geometry_values(
    data: &HashMap<String, JsonValue>,
    template: &TemplateContent,
) -> HashMap<String, f32> {
    let mut map = HashMap::new();
    for (name, spec) in &template.params {
        if let Some(val) = data.get(name) {
            if let Some(f) = val.as_f64() {
                map.insert(name.clone(), f as f32);
            } else if let Some(s) = val.as_str() {
                if let Ok(f) = s.trim().parse::<f32>() {
                    map.insert(name.clone(), f);
                }
            }
        } else {
            let v = match &spec.default {
                Some(crate::models::ParamValue::Float(f)) => *f,
                Some(crate::models::ParamValue::Integer(i)) => *i as f32,
                _ => spec.min.unwrap_or(0.0),
            };
            map.insert(name.clone(), v);
        }
    }
    map
}

/// Render-time environment: the variables map and the datetime resolver, passed together through
/// every render call so related configuration travels as a unit.
pub(crate) struct RenderEnv<'a> {
    pub settings: &'a BTreeMap<String, String>,
    pub datetime: &'a crate::datetime_fmt::DateTimeResolver<'a>,
}

pub(crate) struct RenderContext<'a> {
    pub unit: &'a str,
    pub dpi: u32,
    pub data: &'a HashMap<String, JsonValue>,
    pub env: &'a RenderEnv<'a>,
    pub images: &'a RefCell<ImageCollector>,
    pub instants: Option<&'a BTreeMap<String, DateTime<Local>>>,
}

#[derive(Debug, Clone, Copy)]
struct PlacedBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub frame: (f32, f32),
}

struct SingleItemRenderArgs<'a> {
    pub item: &'a LayoutItem,
    pub measured_node: &'a Measured,
    pub pbox: PlacedBox,
    pub frame: (f32, f32),
    pub geometry_values: &'a HashMap<String, f32>,
    pub path: &'a str,
}

struct ContainerRenderArgs<'a> {
    pub placement: &'a Placement,
    pub shape: Shape,
    pub stroke: &'a Option<crate::models::Stroke>,
    pub background: &'a Option<crate::models::DynamicValue<crate::models::Color>>,
    pub rounded: &'a Option<f32>,
    pub padding: &'a crate::models::Padding,
    pub flow: &'a Option<crate::models::Flow>,
    pub items: &'a [LayoutItem],
    pub children_measured: &'a [Measured],
    pub pbox: PlacedBox,
    pub geometry_values: &'a HashMap<String, f32>,
    pub path: &'a str,
}

struct TextRenderArgs<'a> {
    pub placement: &'a Placement,
    pub font_weight: Option<u16>,
    pub color: Option<&'a crate::models::Color>,
    pub alignment: &'a crate::models::Alignment,
    pub line_spacing: Option<f32>,
    pub pbox: PlacedBox,
    pub text_fit: &'a helpers::TextFit,
}

pub(crate) struct ExpandedItem<'a> {
    pub orig_idx: usize,
    pub elem_idx: Option<usize>,
    pub item: &'a LayoutItem,
    pub data: Option<HashMap<String, JsonValue>>,
}

impl<'a> RenderContext<'a> {
    pub(crate) fn new(
        unit: &'a str,
        dpi: u32,
        data: &'a HashMap<String, JsonValue>,
        env: &'a RenderEnv<'a>,
        images: &'a RefCell<ImageCollector>,
    ) -> Self {
        Self {
            unit,
            dpi,
            data,
            env,
            images,
            instants: None,
        }
    }

    pub(crate) fn with_data<'b>(
        &'b self,
        data: &'b HashMap<String, JsonValue>,
    ) -> RenderContext<'b> {
        RenderContext {
            unit: self.unit,
            dpi: self.dpi,
            data,
            env: self.env,
            images: self.images,
            instants: self.instants,
        }
    }

    pub(crate) fn with_instants(mut self, instants: &'a BTreeMap<String, DateTime<Local>>) -> Self {
        self.instants = Some(instants);
        self
    }

    pub(crate) fn is_item_active(&self, item: &LayoutItem) -> bool {
        if let Some(when) = item.when() {
            when.iter().all(
                |(param_name, expected_val)| match self.data.get(param_name) {
                    Some(val) => &value_to_string(val) == expected_val,
                    None => false,
                },
            )
        } else {
            true
        }
    }

    pub(crate) fn expand_items<'b>(
        &self,
        items: &'b [LayoutItem],
    ) -> Result<Vec<ExpandedItem<'b>>, AppError> {
        let mut expanded = Vec::new();
        for (orig_idx, item) in items.iter().enumerate() {
            if !self.is_item_active(item) {
                continue;
            }
            if let LayoutItem::Container {
                repeat: Some(rep_name),
                ..
            } = item
            {
                let val = self
                    .data
                    .get(rep_name)
                    .ok_or_else(|| AppError::missing_field(rep_name))?;
                if let Some(elements) = val.as_array() {
                    for (elem_idx, elem) in elements.iter().enumerate() {
                        let elem_str = value_to_string(elem);
                        let mut new_data = self.data.clone();
                        new_data.insert(rep_name.clone(), JsonValue::String(elem_str));
                        expanded.push(ExpandedItem {
                            orig_idx,
                            elem_idx: Some(elem_idx),
                            item,
                            data: Some(new_data),
                        });
                    }
                } else {
                    return Err(AppError::unsupported_layout_item(
                        Reason::FieldValueNotScalar,
                        format!("parameter '{rep_name}' must be a list"),
                    ));
                }
            } else {
                expanded.push(ExpandedItem {
                    orig_idx,
                    elem_idx: None,
                    item,
                    data: None,
                });
            }
        }
        Ok(expanded)
    }

    fn resolve_item_text(&self, value: &str) -> Result<String, AppError> {
        interpolate(
            value,
            self.data,
            self.env.settings,
            self.env.datetime,
            self.instants,
        )
    }

    fn resolve_point(
        &self,
        p: &Position,
        frame: (f32, f32),
        path: &str,
    ) -> Result<Point, AppError> {
        const EPS: f32 = 1.0e-4;
        let x = resolve_coord(p.x(), frame.0);
        let y = resolve_coord(p.y(), frame.1);
        if x < -EPS || y < -EPS {
            return Err(AppError::unsupported_layout_item(
                Reason::CoordOutOfFrame,
                format!(
                    "at {path}: a coordinate resolves outside the frame: [{}, {}] against {}x{}",
                    p.x(),
                    p.y(),
                    frame.0,
                    frame.1
                ),
            ));
        }
        Ok(Point { x, y })
    }

    fn check_line(
        &self,
        start: &Point,
        end: &Point,
        frame: (f32, f32),
        path: &str,
    ) -> Result<(), AppError> {
        const EPS: f32 = 1.0e-4;
        for p in [start, end] {
            if p.x > frame.0 + EPS || p.y > frame.1 + EPS {
                return Err(AppError::unsupported_layout_item(
                    Reason::LineEndpointOutOfFrame,
                    format!(
                        "at {path}: a line endpoint resolves outside the frame: [{}, {}] in {}x{}",
                        p.x, p.y, frame.0, frame.1
                    ),
                ));
            }
        }
        if (start.x - end.x).abs() < EPS && (start.y - end.y).abs() < EPS {
            return Err(AppError::unsupported_layout_item(
                Reason::LineDegenerate,
                format!("at {path}: line start and end must differ after resolution"),
            ));
        }
        Ok(())
    }

    pub fn measure_items(
        &self,
        items: &[LayoutItem],
        frame: (f32, f32),
        axes_resolved: [bool; 2],
        geometry_values: &HashMap<String, f32>,
        path_prefix: &str,
    ) -> Result<(Vec<Measured>, f32), AppError> {
        let mut measured_nodes = Vec::new();
        let mut max_req_w = 0.0_f32;

        let expanded_items = self.expand_items(items)?;

        for exp in &expanded_items {
            let item_ctx = match &exp.data {
                Some(d) => self.with_data(d),
                None => self.with_data(self.data),
            };
            let path = match exp.elem_idx {
                Some(e_idx) => format!("{path_prefix}[{}]#{e_idx}", exp.orig_idx),
                None => format!("{path_prefix}[{}]", exp.orig_idx),
            };

            let node = match exp.item.placement() {
                // A `line` has endpoints rather than a box: nothing to size, nothing to measure.
                None => Measured {
                    intrinsic: [None, None],
                    text: None,
                    children: vec![],
                },
                Some(placement) => {
                    // The rules that do not depend on a measurement hold before one is taken, so a
                    // box a request has already invalidated is refused as such rather than as
                    // whatever its content then fails to do inside it.
                    let (measure_box, spec_0, spec_1) = if placement.at.is_none() {
                        let (w, h) = crate::resolver::resolve_packed(
                            placement,
                            frame,
                            geometry_values,
                            [None, None],
                        )
                        .map_err(|violation| violation_error(violation, &path))?;
                        let spec_0 = crate::resolver::source_of(placement, 0, geometry_values);
                        let spec_1 = crate::resolver::source_of(placement, 1, geometry_values);
                        ((w, h), spec_0, spec_1)
                    } else {
                        crate::resolver::precheck(placement, Some(frame), geometry_values)
                            .map_err(|violation| violation_error(violation, &path))?;

                        let spec_0 = crate::resolver::source_of(placement, 0, geometry_values);
                        let spec_1 = crate::resolver::source_of(placement, 1, geometry_values);
                        let measure_box = (
                            crate::resolver::resolve_unmeasured(&spec_0, frame.0, placement.max_w),
                            crate::resolver::resolve_unmeasured(&spec_1, frame.1, placement.max_h),
                        );
                        (measure_box, spec_0, spec_1)
                    };

                    // A container's children are measured against the frame it gives them, which
                    // is its own unmeasured box less rotation and padding.
                    let (children, child_frame) = match exp.item {
                        LayoutItem::Container {
                            placement,
                            padding,
                            items: child_items,
                            ..
                        } => {
                            let geometry = crate::resolver::container_geometry(
                                placement,
                                padding,
                                frame,
                                axes_resolved,
                                geometry_values,
                            );
                            let (children, _) = item_ctx.measure_items(
                                child_items,
                                geometry.inner,
                                geometry.child_axes_resolved,
                                geometry_values,
                                &format!("{path}.items"),
                            )?;
                            (children, geometry.inner)
                        }
                        _ => (vec![], measure_box),
                    };

                    let (intrinsic, text) = item_ctx.intrinsic(IntrinsicInput {
                        item: exp.item,
                        measure_box,
                        demands: [spec_0.demands_intrinsic(), spec_1.demands_intrinsic()],
                        children: &children,
                        child_frame,
                        geometry_values,
                        path: &path,
                    })?;

                    Measured {
                        intrinsic,
                        text,
                        children,
                    }
                }
            };

            max_req_w = max_req_w.max(item_ctx.item_axis_requirement(
                exp.item,
                0,
                frame,
                geometry_values,
                &node,
            ));
            measured_nodes.push(node);
        }

        Ok((measured_nodes, max_req_w))
    }

    /// What an item requires of its frame on `axis`. A `line` claims its endpoints; everything else
    /// is the resolver's composition over the item's classified axis and what it measured.
    fn item_axis_requirement(
        &self,
        item: &LayoutItem,
        axis: usize,
        frame: (f32, f32),
        geometry_values: &HashMap<String, f32>,
        measured: &Measured,
    ) -> f32 {
        let frame_extent = if axis == 0 { frame.0 } else { frame.1 };
        match item {
            LayoutItem::Line { at, to, .. } => {
                let (at_coord, to_coord) = if axis == 0 {
                    (at.x(), to.x())
                } else {
                    (at.y(), to.y())
                };
                crate::resolver::line_axis_requirement(at_coord, to_coord)
            }
            _ => match item.placement() {
                Some(placement) => crate::resolver::axis_requirement(
                    placement,
                    axis,
                    frame_extent,
                    geometry_values,
                    measured.intrinsic[axis],
                ),
                None => 0.0,
            },
        }
    }

    /// The one place item type is visible in sizing: the intrinsic extent an item's own content
    /// has, and — for a `text` — the layout that produced it. Load never calls this; it supplies
    /// availability in place of an intrinsic, which makes `content` resolve exactly as `fill` does.
    ///
    /// It answers both axes at once rather than one at a time, because a `text` produces its width
    /// and its height from a single layout pass and asking per axis would run that pass twice.
    fn intrinsic(
        &self,
        input: IntrinsicInput<'_>,
    ) -> Result<([Option<f32>; 2], Option<helpers::TextFit>), AppError> {
        let IntrinsicInput {
            item,
            measure_box,
            demands,
            children,
            child_frame,
            geometry_values,
            path,
        } = input;
        let per_axis = |extents: (f32, f32)| {
            [
                demands[0].then_some(extents.0),
                demands[1].then_some(extents.1),
            ]
        };

        match item {
            LayoutItem::Text {
                value,
                font_size,
                font_weight,
                wrap,
                line_spacing,
                alignment,
                overflow,
                ..
            } => {
                let text = self.resolve_item_text(value)?;
                let dyn_weight = font_weight.as_ref().map(|dw| match dw {
                    crate::models::DynamicValue::Literal(w) => {
                        crate::models::DynamicValue::Literal(*w)
                    }
                    crate::models::DynamicValue::Ref(r) => {
                        let resolved = self
                            .data
                            .get(r)
                            .and_then(|v| v.as_u64())
                            .map(|u| u as u16)
                            .unwrap_or(400);
                        crate::models::DynamicValue::Literal(resolved)
                    }
                });

                // The layout pass is unconditional: the emitted lines are the render payload
                // whether or not either axis asked for an intrinsic.
                let text_fit = helpers::layout_text(
                    helpers::TextLayoutItem {
                        raw_text: &text,
                        font_size,
                        font_weight: dyn_weight,
                        wrap: *wrap,
                        line_spacing: *line_spacing,
                        alignment: alignment.clone(),
                        overflow: *overflow,
                    },
                    measure_box,
                    self.unit,
                    path,
                )?;

                let extents = (text_fit.width_units, text_fit.height_units);
                Ok((per_axis(extents), Some(text_fit)))
            }
            LayoutItem::Qr { value, params, .. } => {
                if !demands[0] && !demands[1] {
                    return Ok(([None, None], None));
                }
                let payload = self.resolve_item_text(value)?;
                let module_size = params.as_ref().and_then(|p| p.module_size);
                let m = module_size.ok_or_else(|| {
                    AppError::unsupported_layout_item(
                        Reason::IntrinsicSizeUndefined,
                        format!("at {path}: qr with content or fill size requires module_size"),
                    )
                })?;
                let ecc = params
                    .as_ref()
                    .and_then(|p| p.error_correction.as_deref())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| v.to_ascii_uppercase())
                    .map(|v| match v.as_str() {
                        "L" => Ok(qrcode::EcLevel::L),
                        "M" => Ok(qrcode::EcLevel::M),
                        "Q" => Ok(qrcode::EcLevel::Q),
                        "H" => Ok(qrcode::EcLevel::H),
                        _ => Err(AppError::unsupported_layout_item(
                            Reason::QrErrorCorrectionInvalid,
                            "qr error_correction must be one of L, M, Q, H",
                        )),
                    })
                    .transpose()?
                    .unwrap_or(qrcode::EcLevel::M);
                let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), ecc)
                    .map_err(|err| {
                        AppError::render_failed(
                            Reason::QrGenerationFailed,
                            format!("qr generation failed: {err}"),
                        )
                    })?;
                let qz = params.as_ref().and_then(|p| p.quiet_zone).unwrap_or(0.0);
                let qr_dim = (code.width() as f32 + 2.0 * qz) * m;
                Ok((per_axis((qr_dim, qr_dim)), None))
            }
            LayoutItem::Image { name, src, .. } => {
                if !demands[0] && !demands[1] {
                    return Ok(([None, None], None));
                }
                let (bytes, fmt) = match (src.as_deref(), name.as_deref()) {
                    (Some(src_str), _) => {
                        let resolved_src = helpers::interpolate(
                            src_str,
                            self.data,
                            self.env.settings,
                            self.env.datetime,
                            self.instants,
                        )?;
                        helpers::resolve_image_asset(&helpers::assets_root(), &resolved_src)?
                    }
                    (_, Some(name_str)) => {
                        let value = self
                            .data
                            .get(name_str)
                            .ok_or_else(|| AppError::missing_field(name_str))?;
                        if matches!(value, JsonValue::Array(_)) {
                            return Err(AppError::field_value_not_scalar(name_str));
                        }
                        helpers::parse_image_data_uri(&helpers::value_to_string(value))?
                    }
                    (None, None) => {
                        return Err(AppError::unsupported_layout_item(
                            Reason::ImageSourceMissing,
                            format!("at {path}: image missing source"),
                        ));
                    }
                };

                let extents = match fmt {
                    helpers::ImageFmt::Png | helpers::ImageFmt::Jpg => {
                        helpers::raster_image_dimensions(&bytes, fmt, self.dpi, self.unit, path)?
                    }
                    helpers::ImageFmt::Svg => {
                        let svg_str = std::str::from_utf8(&bytes).map_err(|_| {
                            AppError::unsupported_layout_item(
                                Reason::ImageDataInvalid,
                                format!("at {path}: svg data is not valid utf-8"),
                            )
                        })?;
                        let w = if demands[0] {
                            helpers::svg_axis_intrinsic(svg_str, 0, self.unit, self.dpi, path)?
                        } else {
                            0.0
                        };
                        let h = if demands[1] {
                            helpers::svg_axis_intrinsic(svg_str, 1, self.unit, self.dpi, path)?
                        } else {
                            0.0
                        };
                        (w, h)
                    }
                };
                Ok((per_axis(extents), None))
            }
            LayoutItem::Container {
                placement,
                padding,
                flow,
                items: child_items,
                ..
            } => {
                if !demands[0] && !demands[1] {
                    return Ok(([None, None], None));
                }
                let expanded_children = self.expand_items(child_items)?;

                let author = match flow {
                    Some(flow) => {
                        let mut flow_inputs = Vec::with_capacity(expanded_children.len());
                        for (exp, measured) in expanded_children.iter().zip(children.iter()) {
                            let item_ctx = match &exp.data {
                                Some(d) => self.with_data(d),
                                None => self.with_data(self.data),
                            };
                            let (req_0, req_1) = (
                                item_ctx.item_axis_requirement(
                                    exp.item,
                                    0,
                                    child_frame,
                                    geometry_values,
                                    measured,
                                ),
                                item_ctx.item_axis_requirement(
                                    exp.item,
                                    1,
                                    child_frame,
                                    geometry_values,
                                    measured,
                                ),
                            );
                            let child_path = match exp.elem_idx {
                                Some(e_idx) => format!("{path}.items[{}]#{e_idx}", exp.orig_idx),
                                None => format!("{path}.items[{}]", exp.orig_idx),
                            };
                            let resolved_box = if let Some(p) = exp.item.placement() {
                                crate::resolver::resolve_packed(
                                    p,
                                    child_frame,
                                    geometry_values,
                                    measured.intrinsic,
                                )
                                .map_err(|v| violation_error(v, &child_path))?
                            } else {
                                (0.0, 0.0)
                            };
                            flow_inputs.push(crate::resolver::FlowChildInput {
                                resolved_box,
                                requirement: (req_0, req_1),
                            });
                        }
                        let flow_res =
                            crate::resolver::arrange_flow(child_frame, flow, &flow_inputs)
                                .map_err(|(act_idx, v)| {
                                    let exp = &expanded_children[act_idx];
                                    let child_path = match exp.elem_idx {
                                        Some(e_idx) => {
                                            format!("{path}.items[{}]#{e_idx}", exp.orig_idx)
                                        }
                                        None => format!("{path}.items[{}]", exp.orig_idx),
                                    };
                                    violation_error(v, &child_path)
                                })?;
                        flow_res.assembled
                    }
                    None => {
                        let mut author = (0.0_f32, 0.0_f32);
                        for (exp, measured) in expanded_children.iter().zip(children.iter()) {
                            let item_ctx = match &exp.data {
                                Some(d) => self.with_data(d),
                                None => self.with_data(self.data),
                            };
                            author.0 = author.0.max(item_ctx.item_axis_requirement(
                                exp.item,
                                0,
                                child_frame,
                                geometry_values,
                                measured,
                            ));
                            author.1 = author.1.max(item_ctx.item_axis_requirement(
                                exp.item,
                                1,
                                child_frame,
                                geometry_values,
                                measured,
                            ));
                        }
                        author
                    }
                };

                // The contribution is computed in author space and swapped as a completed pair, so
                // a quarter turn moves the padded footprint rather than each term separately.
                let author = (
                    padding.left + padding.right + author.0,
                    padding.top + padding.bottom + author.1,
                );
                let extents = if crate::resolver::rotation_of(placement).swaps_axes() {
                    (author.1, author.0)
                } else {
                    author
                };
                Ok((per_axis(extents), None))
            }
            LayoutItem::Line { .. } => Ok(([None, None], None)),
        }
    }

    /// Recursively render layout items into the output string
    pub fn render_items(
        &self,
        items: &[LayoutItem],
        measured: &[Measured],
        frame: (f32, f32),
        geometry_values: &HashMap<String, f32>,
        flow: Option<&crate::models::Flow>,
        path_prefix: &str,
    ) -> Result<String, AppError> {
        let mut out = String::new();
        let expanded = self.expand_items(items)?;

        match flow {
            Some(flow) => {
                let mut flow_inputs = Vec::with_capacity(expanded.len());
                for (measured_node, exp) in measured.iter().zip(&expanded) {
                    let item_ctx = match &exp.data {
                        Some(d) => self.with_data(d),
                        None => self.with_data(self.data),
                    };
                    let (req_0, req_1) = (
                        item_ctx.item_axis_requirement(
                            exp.item,
                            0,
                            frame,
                            geometry_values,
                            measured_node,
                        ),
                        item_ctx.item_axis_requirement(
                            exp.item,
                            1,
                            frame,
                            geometry_values,
                            measured_node,
                        ),
                    );
                    let child_path = match exp.elem_idx {
                        Some(e_idx) => format!("{path_prefix}[{}]#{e_idx}", exp.orig_idx),
                        None => format!("{path_prefix}[{}]", exp.orig_idx),
                    };
                    let resolved_box = if let Some(p) = exp.item.placement() {
                        crate::resolver::resolve_packed(
                            p,
                            frame,
                            geometry_values,
                            measured_node.intrinsic,
                        )
                        .map_err(|v| violation_error(v, &child_path))?
                    } else {
                        (0.0, 0.0)
                    };
                    flow_inputs.push(crate::resolver::FlowChildInput {
                        resolved_box,
                        requirement: (req_0, req_1),
                    });
                }
                let flow_res = crate::resolver::arrange_flow(frame, flow, &flow_inputs).map_err(
                    |(act_idx, v)| {
                        let exp = &expanded[act_idx];
                        let child_path = match exp.elem_idx {
                            Some(e_idx) => format!("{path_prefix}[{}]#{e_idx}", exp.orig_idx),
                            None => format!("{path_prefix}[{}]", exp.orig_idx),
                        };
                        violation_error(v, &child_path)
                    },
                )?;

                for (placed, (measured_node, exp)) in flow_res
                    .rects
                    .into_iter()
                    .zip(measured.iter().zip(&expanded))
                {
                    let item_ctx = match &exp.data {
                        Some(d) => self.with_data(d),
                        None => self.with_data(self.data),
                    };
                    let path = match exp.elem_idx {
                        Some(e_idx) => format!("{path_prefix}[{}]#{e_idx}", exp.orig_idx),
                        None => format!("{path_prefix}[{}]", exp.orig_idx),
                    };
                    let pbox = PlacedBox {
                        x: placed.x,
                        y: placed.y,
                        w: placed.w,
                        h: placed.h,
                        frame,
                    };
                    item_ctx.render_single_item(
                        &mut out,
                        SingleItemRenderArgs {
                            item: exp.item,
                            measured_node,
                            pbox,
                            frame,
                            geometry_values,
                            path: &path,
                        },
                    )?;
                }
            }
            None => {
                for (measured_node, exp) in measured.iter().zip(&expanded) {
                    let item_ctx = match &exp.data {
                        Some(d) => self.with_data(d),
                        None => self.with_data(self.data),
                    };
                    let path = match exp.elem_idx {
                        Some(e_idx) => format!("{path_prefix}[{}]#{e_idx}", exp.orig_idx),
                        None => format!("{path_prefix}[{}]", exp.orig_idx),
                    };
                    let pbox = match exp.item.placement() {
                        Some(placement) => item_ctx.resolve_placement_box(
                            placement,
                            frame,
                            geometry_values,
                            measured_node.intrinsic,
                            &path,
                        )?,
                        None => PlacedBox {
                            x: 0.0,
                            y: 0.0,
                            w: 0.0,
                            h: 0.0,
                            frame,
                        },
                    };
                    item_ctx.render_single_item(
                        &mut out,
                        SingleItemRenderArgs {
                            item: exp.item,
                            measured_node,
                            pbox,
                            frame,
                            geometry_values,
                            path: &path,
                        },
                    )?;
                }
            }
        }
        Ok(out)
    }

    fn render_single_item(
        &self,
        out: &mut String,
        args: SingleItemRenderArgs<'_>,
    ) -> Result<(), AppError> {
        match args.item {
            LayoutItem::Line { at, to, stroke, .. } => {
                self.render_line_item(out, at, to, stroke.as_ref(), args.frame, args.path)?;
            }
            LayoutItem::Text {
                placement,
                font_weight,
                color,
                alignment,
                line_spacing,
                ..
            } => {
                let resolved_weight = match font_weight {
                    Some(dyn_val) => Some(resolve_dynamic_value_u16(dyn_val, self.data)?),
                    None => None,
                };
                let resolved_color = match color {
                    Some(dyn_val) => Some(resolve_dynamic_value_color(dyn_val, self.data)?),
                    None => None,
                };
                self.render_text_item(
                    out,
                    TextRenderArgs {
                        placement,
                        font_weight: resolved_weight,
                        color: resolved_color.as_ref(),
                        alignment,
                        line_spacing: *line_spacing,
                        pbox: args.pbox,
                        text_fit: args.measured_node.text.as_ref().unwrap(),
                    },
                )?;
            }
            LayoutItem::Qr {
                value,
                placement,
                params,
                ..
            } => {
                let payload = self.resolve_item_text(value)?;
                self.render_qr_item(out, payload, placement, params, args.pbox)?;
            }
            LayoutItem::Image {
                name,
                src,
                placement,
                fit,
                ..
            } => {
                self.render_image_item(
                    out,
                    name.as_deref(),
                    src.as_deref(),
                    placement,
                    fit,
                    args.pbox,
                )?;
            }
            LayoutItem::Container {
                placement,
                shape,
                stroke,
                background,
                rounded,
                padding,
                flow: child_flow,
                items: child_items,
                ..
            } => {
                self.render_container_item(
                    out,
                    ContainerRenderArgs {
                        placement,
                        shape: *shape,
                        stroke,
                        background,
                        rounded,
                        padding,
                        flow: child_flow,
                        items: child_items,
                        children_measured: &args.measured_node.children,
                        pbox: args.pbox,
                        geometry_values: args.geometry_values,
                        path: args.path,
                    },
                )?;
            }
        }
        Ok(())
    }

    fn resolve_placement_box(
        &self,
        placement: &Placement,
        frame: (f32, f32),
        geometry_values: &HashMap<String, f32>,
        intrinsic: [Option<f32>; 2],
        path: &str,
    ) -> Result<PlacedBox, AppError> {
        let placed = crate::resolver::place(placement, frame, geometry_values, intrinsic)
            .map_err(|violation| violation_error(violation, path))?;
        Ok(PlacedBox {
            x: placed.x,
            y: placed.y,
            w: placed.w,
            h: placed.h,
            frame,
        })
    }

    fn render_text_item(&self, out: &mut String, args: TextRenderArgs<'_>) -> Result<(), AppError> {
        let weight_arg = args
            .font_weight
            .map(|w| format!(", weight: {w}"))
            .unwrap_or_default();
        let weight = args.font_weight.unwrap_or(400);

        let fill_arg = args
            .color
            .map(|c| format!(", fill: rgb(\"{}\")", c.hex()))
            .unwrap_or_default();

        let mut body = args
            .text_fit
            .lines
            .iter()
            .map(|l| format!("#text(\"{}\")", escape_typst_string(l)))
            .collect::<Vec<_>>()
            .join("#linebreak()");

        if args.text_fit.lines.last().is_some_and(|l| l.is_empty()) {
            body.push_str("#linebreak()");
        }

        let leading_pt =
            helpers::derived_leading_pt(weight, args.text_fit.font_size_pt, args.line_spacing)?;

        let body = format!(
            "#text(size: {}pt{weight_arg}{fill_arg})[#set par(leading: {leading_pt}pt)\n{body}]",
            args.text_fit.font_size_pt
        );

        let body = pad_block(
            &body,
            helpers::pad_pt(weight, args.text_fit.font_size_pt, args.alignment.vertical)?,
            args.alignment.vertical,
        );

        let inner = format!("#align({})[{body}]", typst_alignment(args.alignment));
        let top = args.pbox.y + args.pbox.h;
        let dx = format_length(args.pbox.x, self.unit)?;
        let dy = format_length(args.pbox.frame.1 - top, self.unit)?;
        let box_width = format_length(args.pbox.w, self.unit)?;
        let box_height = format_length(args.pbox.h, self.unit)?;
        let content = self.wrap_rotation(inner, args.placement.rotate);

        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
        )
        .map_err(|err| {
            AppError::render_failed(
                Reason::TypstSourceBuildFailed,
                format!("failed to build typst source: {err}"),
            )
        })?;

        Ok(())
    }

    fn render_qr_item(
        &self,
        out: &mut String,
        payload: String,
        placement: &Placement,
        params: &Option<crate::models::QrParams>,
        pbox: PlacedBox,
    ) -> Result<(), AppError> {
        let top = pbox.y + pbox.h;
        let dx = format_length(pbox.x, self.unit)?;
        let dy = format_length(pbox.frame.1 - top, self.unit)?;
        let box_width = format_length(pbox.w, self.unit)?;
        let box_height = format_length(pbox.h, self.unit)?;
        let svg_xml = build_qr_svg(payload.as_bytes(), params)?;
        let svg_xml = escape_typst_string(&svg_xml);

        let content = format!(
            "#image(bytes(\"{svg_xml}\"), format: \"svg\", width: {box_width}, height: {box_height}, fit: \"contain\")"
        );
        let content = self.wrap_rotation(content, placement.rotate);
        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
        )
        .map_err(|err| {
            AppError::render_failed(
                Reason::TypstSourceBuildFailed,
                format!("failed to build typst source: {err}"),
            )
        })?;

        Ok(())
    }

    fn render_image_item(
        &self,
        out: &mut String,
        name: Option<&str>,
        src: Option<&str>,
        placement: &Placement,
        fit: &Fit,
        pbox: PlacedBox,
    ) -> Result<(), AppError> {
        let (bytes, fmt) = match (src, name) {
            (Some(src), _) => {
                let resolved_src = interpolate(
                    src,
                    self.data,
                    self.env.settings,
                    self.env.datetime,
                    self.instants,
                )?;
                resolve_image_asset(&assets_root(), &resolved_src)?
            }
            (_, Some(name)) => {
                let value = self
                    .data
                    .get(name)
                    .ok_or_else(|| AppError::missing_field(name))?;
                if matches!(value, JsonValue::Array(_)) {
                    return Err(AppError::field_value_not_scalar(name));
                }
                parse_image_data_uri(&value_to_string(value))?
            }
            (None, None) => {
                return Err(AppError::unsupported_layout_item(
                    Reason::ImageSourceMissing,
                    "image requires src or name",
                ))
            }
        };
        let top = pbox.y + pbox.h;
        let vpath = self.images.borrow_mut().add(fmt.ext(), bytes);
        let dx = format_length(pbox.x, self.unit)?;
        let dy = format_length(pbox.frame.1 - top, self.unit)?;
        let box_width = format_length(pbox.w, self.unit)?;
        let box_height = format_length(pbox.h, self.unit)?;
        let content = format!(
            "#image(\"{vpath}\", width: {box_width}, height: {box_height}, fit: \"{fit}\")",
            fit = fit.as_typst()
        );
        let content = self.wrap_rotation(content, placement.rotate);
        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
        )
        .map_err(|err| {
            AppError::render_failed(
                Reason::TypstSourceBuildFailed,
                format!("failed to build typst source: {err}"),
            )
        })?;

        Ok(())
    }

    fn render_line_item(
        &self,
        out: &mut String,
        at: &Position,
        to: &Position,
        stroke: Option<&Stroke>,
        frame: (f32, f32),
        path: &str,
    ) -> Result<(), AppError> {
        let start_point = self.resolve_point(at, frame, path)?;
        let end_point = self.resolve_point(to, frame, path)?;
        self.check_line(&start_point, &end_point, frame, path)?;
        let Some(stroke) = stroke else {
            return Ok(());
        };
        let (start_x, start_y) = to_page_coords(&start_point, frame.1);
        let (end_x, end_y) = to_page_coords(&end_point, frame.1);
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let start_x = format_length(start_x, self.unit)?;
        let start_y = format_length(start_y, self.unit)?;
        let dx = format_length(dx, self.unit)?;
        let dy = format_length(dy, self.unit)?;
        let zero = format_length(0.0, self.unit)?;
        let thickness = format_length(stroke.thickness, self.unit)?;
        let resolved_color = resolve_dynamic_value_color(&stroke.color, self.data)?;
        let color = format!("rgb(\"{}\")", resolved_color.hex());

        let content = format!(
            "#line(start: ({zero}, {zero}), end: ({dx}, {dy}), stroke: {thickness} + {color})"
        );
        writeln!(
            out,
            "#place(top + left, dx: {start_x}, dy: {start_y})[{content}]"
        )
        .map_err(|err| {
            AppError::render_failed(
                Reason::TypstSourceBuildFailed,
                format!("failed to build typst source: {err}"),
            )
        })?;

        Ok(())
    }

    fn render_container_item(
        &self,
        out: &mut String,
        args: ContainerRenderArgs<'_>,
    ) -> Result<(), AppError> {
        if matches!(args.shape, Shape::Circle)
            && (args.pbox.w - args.pbox.h).abs() > crate::resolver::BOUNDS_EPSILON
        {
            return Err(AppError::unsupported_layout_item(
                Reason::CircleBoxNotSquare,
                format!("circle container at '{}' is not square", args.path),
            ));
        }
        let rotation = crate::resolver::rotation_of(args.placement);

        let top = args.pbox.y + args.pbox.h;
        let dx = format_length(args.pbox.x, self.unit)?;
        let dy = format_length(args.pbox.frame.1 - top, self.unit)?;
        let box_width = format_length(args.pbox.w, self.unit)?;
        let box_height = format_length(args.pbox.h, self.unit)?;

        let ((canvas_w, canvas_h), inner) =
            crate::resolver::container_frames((args.pbox.w, args.pbox.h), rotation, args.padding);

        let child_source = self.render_items(
            args.items,
            args.children_measured,
            inner,
            args.geometry_values,
            args.flow.as_ref(),
            &format!("{}.items", args.path),
        )?;

        let inner = if args.padding == &crate::models::Padding::ZERO {
            child_source
        } else {
            let pad_left = format_length(args.padding.left, self.unit)?;
            let pad_top = format_length(args.padding.top, self.unit)?;
            format!("#place(top + left, dx: {pad_left}, dy: {pad_top})[{child_source}]")
        };

        let rotated = if rotation.is_rotated() {
            let canvas_w_len = format_length(canvas_w, self.unit)?;
            let canvas_h_len = format_length(canvas_h, self.unit)?;
            let canvas = format!("#box(width: {canvas_w_len}, height: {canvas_h_len})[{inner}]");
            self.wrap_rotation(canvas, args.placement.rotate)
        } else {
            self.wrap_rotation(inner, args.placement.rotate)
        };

        let fill = match args.background {
            Some(bg) => {
                let resolved_bg = resolve_dynamic_value_color(bg, self.data)?;
                format!("rgb(\"{}\")", resolved_bg.hex())
            }
            None => "none".to_string(),
        };
        let stroke = match args.stroke {
            Some(st) => {
                let thickness = format_length(st.thickness, self.unit)?;
                let resolved_st_color = resolve_dynamic_value_color(&st.color, self.data)?;
                let color = format!("rgb(\"{}\")", resolved_st_color.hex());
                format!("{thickness} + {color}")
            }
            None => "none".to_string(),
        };

        match args.shape {
            Shape::Rect => {
                let radius = match args.rounded {
                    Some(r) => {
                        let max_radius = args.pbox.w.min(args.pbox.h) / 2.0;
                        let clamped = r.min(max_radius);
                        format_length(clamped, self.unit)?
                    }
                    None => format_length(0.0, self.unit)?,
                };
                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, fill: {fill}, stroke: {stroke}, radius: {radius}, clip: true)[{rotated}]]"
                )
                .map_err(|err| {
                    AppError::render_failed(
                        Reason::TypstSourceBuildFailed,
                        format!("failed to build typst source: {err}"),
                    )
                })?;
            }
            Shape::Ellipse | Shape::Circle => {
                if args.stroke.is_some() || args.background.is_some() {
                    let frame_content = format!(
                        "#ellipse(width: {box_width}, height: {box_height}, fill: {fill}, stroke: {stroke})"
                    );
                    writeln!(
                        out,
                        "#place(top + left, dx: {dx}, dy: {dy})[{frame_content}]"
                    )
                    .map_err(|err| {
                        AppError::render_failed(
                            Reason::TypstSourceBuildFailed,
                            format!("failed to build typst source: {err}"),
                        )
                    })?;
                }
                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{rotated}]]"
                )
                .map_err(|err| {
                    AppError::render_failed(
                        Reason::TypstSourceBuildFailed,
                        format!("failed to build typst source: {err}"),
                    )
                })?;
            }
        }

        Ok(())
    }

    fn wrap_rotation(&self, content: String, rotate: Option<f32>) -> String {
        // Typst positive angles rotate clockwise (screen coords); our `rotate` contract is
        // counter-clockwise, so negate. `reflow: true` normalizes the box to the rotated footprint.
        match rotate
            .and_then(Rotation::from_degrees)
            .unwrap_or(Rotation::R0)
        {
            Rotation::R0 => content,
            Rotation::R90 => format!("#rotate(-90deg, reflow: true)[{content}]"),
            Rotation::R180 => format!("#rotate(180deg, reflow: true)[{content}]"),
            Rotation::R270 => format!("#rotate(90deg, reflow: true)[{content}]"),
        }
    }
}

/// 1×1 transparent PNG data URI: a valid stand-in for data-bound image fields.
pub const SAMPLE_PNG_DATA_URI: &str =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

#[cfg(test)]
mod tests {
    use super::{
        count_pdf_pages, render_sheet_pages, render_single_label, render_single_label_image,
        render_single_label_pdf, render_thumbnail_png, SAMPLE_PNG_DATA_URI,
    };
    use crate::errors::AppError;
    use crate::models::{
        Alignment, Color, Dimension, DynamicDimension, DynamicValue, Extent, Fit, FontSize,
        HorizontalAlign, LabelInput, Layout, LayoutItem, Overflow, Padding, ParamSpec, ParamType,
        Placement, Position, Shape, SheetPosition, Size, SizeValue, Stroke, TemplateFormat,
        VerticalAlign,
    };
    use crate::reason::Reason;
    use crate::templates::{TemplateContent, TemplateDefinition};
    use indexmap::IndexMap;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    fn render_test_items(items: &[LayoutItem], frame: (f32, f32)) -> Result<String, AppError> {
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);
        let geometry_values = HashMap::new();
        let (measured, _) =
            ctx.measure_items(items, frame, [true, true], &geometry_values, "layout")?;
        ctx.render_items(items, &measured, frame, &geometry_values, None, "layout")
    }

    /// Build a one-text-item source. `size_w` of `None` means an auto width, which routes through
    /// the auto-length path; `Some(w)` takes the fixed-size path. Both must carry the weight.
    fn text_source(
        weight: Option<u16>,
        size_w: Option<f32>,
        font_size: FontSize,
        text: &str,
    ) -> String {
        text_source_aligned(weight, size_w, font_size, text, VerticalAlign::Top)
    }

    fn text_source_aligned(
        weight: Option<u16>,
        size_w: Option<f32>,
        font_size: FontSize,
        text: &str,
        vertical: VerticalAlign,
    ) -> String {
        text_source_h_aligned_weighted(
            weight,
            HorizontalAlign::Left,
            vertical,
            size_w,
            font_size,
            text,
        )
    }

    fn text_source_with_size(
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        size_w: SizeValue,
        font_size: FontSize,
        text: &str,
    ) -> String {
        let item = LayoutItem::Text {
            value: text.to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([size_w, SizeValue::fixed(30.0)]),
            ),
            font_size,
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment {
                horizontal,
                vertical,
            },
            overflow: Overflow::Ellipsis,
            when: None,
        };
        render_test_items(&[item], (80.0, 40.0)).expect("render text item")
    }

    fn text_source_h_aligned_weighted(
        weight: Option<u16>,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        size_w: Option<f32>,
        font_size: FontSize,
        text: &str,
    ) -> String {
        let item = LayoutItem::Text {
            value: text.to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([
                    match size_w {
                        Some(w) => SizeValue::fixed(w),
                        None => SizeValue::fill(),
                    },
                    SizeValue::fixed(30.0),
                ]),
            ),
            font_size,
            font_weight: weight.map(Into::into),
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment {
                horizontal,
                vertical,
            },
            overflow: Overflow::Ellipsis,
            when: None,
        };
        render_test_items(&[item], (80.0, 40.0)).expect("render text item")
    }

    /// The fitted size of the text at the start of `source`.
    ///
    /// Emission wraps a block in `#text(size: Npt)[...]` and leaves the inner runs unsized, so the
    /// size that applies to a literal is the nearest one *before* it, not after: searching forward
    /// finds the next item's size instead. Callers pass the whole source and the literal's offset.
    fn fitted_pt_at(source: &str, at: usize) -> f32 {
        let before = &source[..at];
        let start = before.rfind("size: ").expect("a size enclosing the text") + 6;
        let rest = &source[start..];
        let end = rest.find("pt").expect("pt suffix");
        rest[..end].parse().expect("a number")
    }

    fn fitted_pt(source: &str) -> f32 {
        let at = source.find("size: ").expect("a size in the source") + 6;
        let rest = &source[at..];
        let end = rest.find("pt").expect("pt suffix");
        rest[..end].parse().expect("a number")
    }

    /// #97 on the fixed-size path.
    #[test]
    fn font_weight_is_emitted_on_the_fixed_size_path() {
        let src = text_source(Some(700), Some(60.0), FontSize::Fixed(10.0), "Widget");
        assert!(src.contains("weight: 700"), "no weight in source: {src}");
    }

    /// The emitted pad is `pad_em × size` for the aligned edge — the *placement* constant. Not
    /// `overflow_em`: for `top` and `bottom` the fitter's reservation is twice this and never reaches the source (#124).
    #[test]
    fn the_emitted_pad_is_the_aligned_edge_metric() {
        // 0.2412em at 20pt = 4.82pt, and in Inter the top and bottom pads are the same 494 units —
        // a coincidence of this font, not a shared constant.
        let bottom = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "gjpqy",
            VerticalAlign::Bottom,
        );
        assert!(
            bottom.contains("#pad(bottom: 4.82"),
            "unexpected source: {bottom}"
        );
        let top = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "Édgy",
            VerticalAlign::Top,
        );
        assert!(top.contains("#pad(top: 4.82"), "unexpected source: {top}");
        let centered = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "Hxy",
            VerticalAlign::Center,
        );
        assert!(
            !centered.contains("#pad"),
            "center must not pad: {centered}"
        );
    }

    /// #180: on an auto-length frame, auto-width text with `horizontal: center` or `right`
    /// emits a box spanning the full alignment slot (the frame remainder), while `left`
    /// continues to emit the fitted content width.
    #[test]
    fn auto_width_text_horizontal_alignment_on_dynamic_frame() {
        let centered = text_source_with_size(
            HorizontalAlign::Center,
            VerticalAlign::Top,
            SizeValue::fill(),
            FontSize::Fixed(10.0),
            "Hi",
        );
        assert!(
            centered.contains("#box(width: 80mm"),
            "center must emit full slot box (80mm), got: {centered}"
        );
        assert!(
            centered.contains("#align(top + center)"),
            "expected center alignment in: {centered}"
        );

        let right = text_source_with_size(
            HorizontalAlign::Right,
            VerticalAlign::Top,
            SizeValue::fill(),
            FontSize::Fixed(10.0),
            "Hi",
        );
        assert!(
            right.contains("#box(width: 80mm"),
            "right must emit full slot box (80mm), got: {right}"
        );
        assert!(
            right.contains("#align(top + right)"),
            "expected right alignment in: {right}"
        );

        let left = text_source_with_size(
            HorizontalAlign::Left,
            VerticalAlign::Top,
            SizeValue::content(),
            FontSize::Fixed(10.0),
            "Hi",
        );
        assert!(
            !left.contains("#box(width: 80mm"),
            "left must keep fitted width box, got: {left}"
        );
        assert!(
            left.contains("#align(top + left)"),
            "expected left alignment in: {left}"
        );
    }

    /// #180: max_w caps the alignment slot at render time for center and right alignment.
    #[test]
    fn auto_width_text_max_w_caps_alignment_slot_at_render() {
        let item = LayoutItem::Text {
            value: "Hi".to_string(),
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: Extent::Size(Size([SizeValue::fill(), SizeValue::fixed(8.0)])),
                max_w: Some(30.0),
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: Alignment {
                horizontal: HorizontalAlign::Center,
                vertical: VerticalAlign::Top,
            },
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let source = render_test_items(&[item], (80.0, 40.0)).expect("render");
        assert!(
            source.contains("#box(width: 30mm"),
            "max_w: 30mm must cap the 80mm frame remainder, got: {source}"
        );
    }

    /// #180: centred auto-width text inside a padded container on a dynamic frame
    /// gets the container's padded inner remainder, not the outer label frame.
    #[test]
    fn auto_width_text_in_padded_container_on_dynamic_frame() {
        let child = LayoutItem::Text {
            value: "Hi".to_string(),
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: Extent::Size(Size([SizeValue::fill(), SizeValue::fixed(8.0)])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: Alignment {
                horizontal: HorizontalAlign::Center,
                vertical: VerticalAlign::Top,
            },
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let container = LayoutItem::Container {
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: Extent::Size(Size([SizeValue::fixed(50.0), SizeValue::fixed(20.0)])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: Padding {
                top: 0.0,
                right: 5.0,
                bottom: 0.0,
                left: 5.0,
            },
            flow: None,
            repeat: None,
            items: vec![child],
        };
        let source = render_test_items(&[container], (80.0, 40.0)).expect("render");
        assert!(
            source.contains("#box(width: 40mm"),
            "nested text must span container inner width (40mm), not 80mm or 50mm, got: {source}"
        );
    }

    /// #180: dynamic-width template tests end to end for content-fitting (3.3) and min-clamping (3.4).
    #[test]
    fn dynamic_width_template_centered_text_end_to_end() {
        let yaml = r#"
name: Dynamic Centered E2E
unit: mm
dpi: 200
params:
  - name: message
    type: string
format:
  type: single
  height: 12
  width:
    min: 20
    max: 100
layout:
  - type: text
    value: "{message}"
    at: [0, 0]
    size: [content, 12]
    font_size: 10
    alignment:
      horizontal: center
"#;
        let template = parse_and_validate(yaml).unwrap();

        // 3.3: Content between min and max (e.g. ~50mm).
        // Prove measurement pass is untouched: label is sized to content, not clamped to min or max.
        let mut data_fit = HashMap::new();
        data_fit.insert(
            "message".to_string(),
            json!("This is a medium length label message"),
        );
        let png_fit =
            render_single_label(&template, &data_fit, &BTreeMap::new(), &resolver()).unwrap();
        let img_fit = image::load_from_memory(&png_fit).unwrap();
        let min_px = (20.0_f32 / 25.4 * 200.0).round() as u32;
        let max_px = (100.0_f32 / 25.4 * 200.0).round() as u32;
        assert!(
            img_fit.width() > min_px,
            "fitted text width in px ({}) must be > min_px ({min_px})",
            img_fit.width()
        );
        assert!(
            img_fit.width() < max_px,
            "fitted text width in px ({}) must be < max_px ({max_px})",
            img_fit.width()
        );

        // 3.4: Content narrower than min (e.g. "Hi", ~3.5mm).
        // Label is clamped to width.min (20mm).
        let mut data_short = HashMap::new();
        data_short.insert("message".to_string(), json!("Hi"));
        let png_short =
            render_single_label(&template, &data_short, &BTreeMap::new(), &resolver()).unwrap();
        let img_short = image::load_from_memory(&png_short).unwrap();
        assert_eq!(
            img_short.width(),
            min_px,
            "short message must render at width.min (20mm = {min_px}px), got {}px",
            img_short.width()
        );

        // Also assert that the emitted text box with fill spans the full 20mm slot:
        let clamped_src = {
            let item = LayoutItem::Text {
                value: "Hi".to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: Extent::Size(Size([SizeValue::fill(), SizeValue::fixed(12.0)])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment {
                    horizontal: HorizontalAlign::Center,
                    vertical: VerticalAlign::Top,
                },
                overflow: Overflow::Ellipsis,
                when: None,
            };
            render_test_items(&[item], (20.0, 12.0)).expect("render")
        };
        assert!(
            clamped_src.contains("#box(width: 20mm"),
            "clamped 20mm frame must emit full 20mm slot box, got: {clamped_src}"
        );
    }

    /// #97 on the auto-length path. Wired separately from the fixed-size path, and a field carried
    /// by only one of the two is a failure this codebase has had before.
    #[test]
    fn font_weight_is_emitted_on_the_auto_length_path() {
        let src = text_source(Some(700), None, FontSize::Fixed(10.0), "Widget");
        assert!(src.contains("weight: 700"), "no weight in source: {src}");
    }

    /// Absent means absent: existing templates keep byte-identical source (spec, Decision 2).
    #[test]
    fn no_font_weight_emits_no_weight_argument() {
        let src = text_source(None, Some(60.0), FontSize::Fixed(10.0), "Widget");
        assert!(
            !src.contains("weight:"),
            "unexpected weight in source: {src}"
        );
    }

    /// The measure pre-pass is a third, separate consumer of the weight, and the source assertions
    /// cannot see it: it decides the auto width of a tape label before any text is emitted. If it
    /// measured unweighted, a bold label would be sized for narrower text than it renders (#96).
    #[test]
    fn the_measure_pre_pass_sizes_an_auto_width_item_for_its_weight() {
        fn measured_width(weight: Option<u16>) -> f32 {
            use std::cell::RefCell;
            let data: HashMap<String, super::JsonValue> = HashMap::new();
            let settings = no_settings();
            let datetime = no_datetime();
            let env = super::RenderEnv {
                settings: &settings,
                datetime: &datetime,
            };
            let images = RefCell::new(super::ImageCollector::default());
            let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);
            let item = LayoutItem::Text {
                value: "Widget A-42 Storage".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::content(), SizeValue::fixed(8.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: weight.map(Into::into),
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            };
            let geometry_values = HashMap::new();
            let (measured, _) = ctx
                .measure_items(
                    &[item],
                    (200.0, 40.0),
                    [true, true],
                    &geometry_values,
                    "layout",
                )
                .expect("measure");
            measured[0].intrinsic[0].unwrap()
        }

        let regular = measured_width(None);
        let bold = measured_width(Some(900));
        assert!(
            bold > regular,
            "the pre-pass measured {bold} for weight 900 and {regular} unweighted: it ignored the weight"
        );
    }

    /// The renderer emitting `weight:` proves nothing about the *fitter* getting it: leaving 400
    /// wired into the fit calls would keep every other test green while #96 stayed unfixed. Bold is
    /// wider, so the same string in the same box must fit at a smaller size.
    #[test]
    fn a_bold_item_fits_at_a_smaller_size_than_an_unweighted_one() {
        let range = FontSize::Range {
            min: 6.0,
            max: 40.0,
        };
        let text = "Widget A-42 Storage Bin";
        let regular = fitted_pt(&text_source(None, Some(40.0), range.clone(), text));
        let bold = fitted_pt(&text_source(Some(900), Some(40.0), range, text));
        assert!(
            regular < 40.0,
            "the box must actually constrain the fit (got {regular}pt)"
        );
        assert!(
            bold < regular,
            "bold fitted at {bold}pt, regular at {regular}pt: the fitter ignored the weight"
        );
    }

    /// A rotated container used to have its subtree skipped by the measure pass, which is what made
    /// a content-sized descendant illegal. It is measured now, exactly as an unrotated one is.
    #[test]
    fn a_rotated_container_measures_its_children_like_an_unrotated_one() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);

        let auto_text = LayoutItem::Text {
            value: "hello".to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::content(), SizeValue::fixed(10.0)]),
            ),
            font_size: FontSize::Fixed(6.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let make_container = |rotate: Option<f32>| LayoutItem::Container {
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: Extent::Size(Size([SizeValue::fixed(80.0), SizeValue::fixed(40.0)])),
                max_w: None,
                max_h: None,
                rotate,
            },
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![auto_text.clone()],
        };

        let geometry_values = HashMap::new();
        let (out_rot, _) = ctx
            .measure_items(
                &[make_container(Some(90.0))],
                (80.0, 40.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .unwrap();
        let (out_plain, _) = ctx
            .measure_items(
                &[make_container(None)],
                (80.0, 40.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .unwrap();
        for (label, measured) in [("rotated", &out_rot), ("plain", &out_plain)] {
            assert_eq!(measured.len(), 1, "{label}");
            let child = measured[0]
                .children
                .first()
                .unwrap_or_else(|| panic!("{label} container measured no child"));
            assert!(
                child.intrinsic[0].is_some_and(|w| w > 0.0),
                "{label} child has no measured content width: {:?}",
                child.intrinsic
            );
            assert!(
                child.text.is_some(),
                "{label} child carries no laid-out text"
            );
        }
    }

    /// The vertical axis contributes exactly as the horizontal one does, and a container's
    /// contribution is its children's requirements — offsets included — plus its own padding, taken
    /// recursively. Nothing in the sizing rules is written per axis or per depth.
    #[test]
    fn a_content_height_container_contributes_its_nested_children_and_offsets() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);

        let text = LayoutItem::Text {
            value: "hi".to_string(),
            placement: Placement::sized(
                Position([0.0, 3.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(8.0)]),
            ),
            font_size: FontSize::Fixed(6.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let inner = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 5.0]),
                Size([SizeValue::fixed(30.0), SizeValue::content()]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: Padding {
                top: 1.0,
                right: 0.0,
                bottom: 2.0,
                left: 0.0,
            },
            flow: None,
            repeat: None,
            items: vec![text],
        };
        let outer = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(50.0), SizeValue::content()]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![inner],
        };

        let geometry_values = HashMap::new();
        let (measured, _) = ctx
            .measure_items(
                &[outer],
                (100.0, 100.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .expect("measure");

        // inner: 3 (padding) + 3 (child offset) + 8 (child height) = 14
        let inner_height = measured[0].children[0].intrinsic[1].expect("inner height");
        assert!(
            (inner_height - 14.0).abs() < 1e-3,
            "inner contributed {inner_height}, expected 14"
        );
        // outer: the inner container's offset of 5 plus its 14
        let outer_height = measured[0].intrinsic[1].expect("outer height");
        assert!(
            (outer_height - 19.0).abs() < 1e-3,
            "outer contributed {outer_height}, expected 19"
        );
    }

    fn measured_extent_of(item: LayoutItem, budget: f32) -> (f32, usize) {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);
        let geometry_values = HashMap::new();
        let (measured, max_req_w) = ctx
            .measure_items(
                &[item],
                (budget, 40.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .expect("measure");
        fn count_text(nodes: &[super::Measured]) -> usize {
            nodes
                .iter()
                .map(|n| {
                    let this = if n.text.is_some() { 1 } else { 0 };
                    this + count_text(&n.children)
                })
                .sum()
        }
        let text_count = count_text(&measured);
        (max_req_w, text_count)
    }

    /// Builds a `RenderContext` over a dynamic-width frame, so `render_container_item` takes its
    /// auto-width branch. Empty `texts` is legitimate: the mode comes from the format, not from
    /// whether any text needed measuring.
    fn dynamic_ctx_source(frame_w: f32, item: LayoutItem) -> String {
        render_test_items(&[item], (frame_w, 12.0)).expect("render")
    }

    fn capped_container(at_x: f32, max_w: Option<f32>, items: Vec<LayoutItem>) -> LayoutItem {
        LayoutItem::Container {
            placement: Placement {
                at: Some(Position([at_x, 0.0])),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::fill(),
                    SizeValue::fixed(12.0),
                ])),
                max_w,
                max_h: None,
                rotate: None,
            },
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: crate::models::Padding::ZERO,
            flow: None,
            repeat: None,
            items,
        }
    }

    /// `measure`'s fixed-width text branch consumed only the width but resolved both axes, so an
    /// `auto` height with no `max_h` errored in the pre-pass even though measurement never wanted
    /// the height. Nothing about this item's height affects the label's width.
    #[test]
    fn measuring_a_fixed_width_text_ignores_its_auto_height() {
        let item = LayoutItem::Text {
            value: "hi".to_string(),
            placement: Placement {
                at: Some(Position([0.0, 10.0])),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::fixed(20.0),
                    SizeValue::fill(),
                ])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(6.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let (extent, _) = measured_extent_of(item, 100.0);
        assert_eq!(extent, 20.0, "a fixed-width text contributes its width");
    }

    /// Spec §7. `measure_container_footprint` resolved this width with no fallback, so a
    /// right-anchored auto-width container errored in the pre-pass even though render handles it as
    /// `frame_width - left`. The child must resolve to the remainder, 60 - 30 = 30, at both passes —
    /// asserting the parent's 60mm footprint instead would pass against a full-frame fallback.
    #[test]
    fn a_nested_right_anchored_auto_container_resolves_to_the_remainder() {
        fn nested() -> LayoutItem {
            LayoutItem::Container {
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::fixed(60.0),
                        SizeValue::fixed(12.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                when: None,
                shape: Shape::Rect,
                stroke: None,
                background: None,
                rounded: None,
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
                items: vec![LayoutItem::Container {
                    placement: Placement {
                        at: Some(Position([-30.0, 0.0])),
                        extent: crate::models::Extent::Size(Size([
                            SizeValue::fill(),
                            SizeValue::fixed(12.0),
                        ])),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    when: None,
                    shape: Shape::Rect,
                    stroke: None,
                    background: None,
                    rounded: None,
                    padding: crate::models::Padding::ZERO,
                    flow: None,
                    repeat: None,
                    items: vec![],
                }],
            }
        }

        // Measurement must not error on the child's auto width.
        let (extent, _) = measured_extent_of(nested(), 100.0);
        assert_eq!(extent, 60.0, "the fixed-width parent's own footprint");

        // And the child renders at the remainder of the parent's inner box.
        let source = dynamic_ctx_source(100.0, nested());
        assert!(
            source.contains("width: 30mm"),
            "the child must resolve to 60 - 30 = 30, not the whole frame: {source}"
        );
    }

    /// The render half of #152. The frame is 100mm wide and the container sits at x=90, so the
    /// remainder is 10mm and the 5mm cap is the binding constraint. Before the fix this branch
    /// ignores `max_w` entirely and emits the 10mm remainder.
    #[test]
    fn max_w_caps_a_dynamic_container_at_render() {
        let source = dynamic_ctx_source(100.0, capped_container(90.0, Some(5.0), vec![]));
        assert!(
            source.contains("width: 5mm"),
            "the container must render at max_w: 5mm, not the 10mm remainder: {source}"
        );
    }

    /// The measure half of #152. The child is load-bearing: the cap only binds when the content
    /// would otherwise exceed it, so an *empty* container measures the same before and after and
    /// proves nothing. Uncapped this contributes at_x plus the child's full natural width; capped
    /// it contributes at_x plus the cap.
    #[test]
    fn max_w_caps_a_dynamic_container_during_measurement() {
        let child = LayoutItem::Text {
            value: "a string far wider than any five millimetre cap".to_string(),
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::content(),
                    SizeValue::fixed(8.0),
                ])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let (uncapped, _) =
            measured_extent_of(capped_container(10.0, None, vec![child.clone()]), 100.0);
        let (capped, _) = measured_extent_of(capped_container(10.0, Some(5.0), vec![child]), 100.0);
        assert!(
            uncapped > 30.0,
            "the child must be wide enough for the cap to bind, got {uncapped}"
        );
        assert!(
            capped <= 15.0 + 1.0e-3 && capped > 10.0,
            "a container at x=10 capped to 5mm contributes <= 15, got {capped}"
        );
    }

    /// #152's own repro template, asserted as *correctly* rejected. The load-time check was right
    /// all along; the renderer was the liar. Testing only the rejection would pass even against
    /// unfixed code, so this also pins that the container really does render at its cap, which is
    /// what makes a child line reaching x=50 genuinely not fit.
    #[test]
    fn the_152_repro_is_rejected_and_the_rejection_is_correct() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    size: [fill, 12.0]\n    max_w: 30.0\n    items:\n      - type: line\n        at: [0.0, 3.0]\n        to: [50.0, 3.0]\n        stroke:\n          thickness: 0.2\n";
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml).expect("parses");
        let template = crate::templates::TemplateContent::try_from(raw).expect("converts");
        assert!(
            template.validate().is_err(),
            "a 50mm line inside a 30mm-capped container must be rejected"
        );
        // And the rejection is correct because the container really is 30mm at render.
        let source = dynamic_ctx_source(100.0, capped_container(0.0, Some(30.0), vec![]));
        assert!(
            source.contains("width: 30mm"),
            "the container renders at its cap, so the rejected line truly does not fit: {source}"
        );
    }

    /// The render half of #155: the fixed path had no fallback, so max_h resolved uncapped to 200
    /// and overflowed a 40mm frame. Asserting the render succeeds is the point; before this task it
    /// is a 422 on every label.
    #[test]
    fn the_155_repro_renders() {
        let template = TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(60.0),
                }
                .into(),
                height: Dimension::Fixed(40.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "x".to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::fixed(20.0),
                        SizeValue::fill(),
                    ])),
                    max_w: None,
                    max_h: Some(200.0),
                    rotate: None,
                },
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        assert_eq!(template.validate(), Ok(()));
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("#155: a max_h above the frame must cap, not overflow");
    }

    /// The number, not just the absence of an error. `render/mod.rs` has its own `resolve_size`
    /// copy, so a fix applied only to `templates.rs` — or one that produced a different but still
    /// in-bounds height — would satisfy the test above and still be wrong. Assert the emitted box.
    #[test]
    fn the_155_repro_renders_at_the_capped_height() {
        let source = render_test_items(
            &[LayoutItem::Text {
                value: "x".to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::fixed(20.0),
                        SizeValue::fill(),
                    ])),
                    max_w: None,
                    max_h: Some(200.0),
                    rotate: None,
                },
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }],
            (60.0, 40.0),
        )
        .expect("renders");
        assert!(
            source.contains("height: 40mm"),
            "the box must be capped to the frame, not the 200mm max_h: {source}"
        );
    }

    /// The render side of the fixed-format cases. Each asserts the emitted box, not that rendering
    /// succeeded: a render-side fallback bug on either axis produces a different but still in-bounds
    /// number, which an `.expect()` would accept.
    #[test]
    fn fixed_format_text_renders_at_the_remainder() {
        fn source_at(frame: (f32, f32), at: [f32; 2], size: Size, max_h: Option<f32>) -> String {
            render_test_items(
                &[LayoutItem::Text {
                    value: "x".to_string(),
                    placement: Placement {
                        at: Some(Position(at)),
                        extent: crate::models::Extent::Size(size),
                        max_w: None,
                        max_h,
                        rotate: None,
                    },
                    font_size: FontSize::Fixed(6.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: crate::models::Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                }],
                frame,
            )
            .expect("renders")
        }

        // Height axis on a fixed label: 40 - 10 = 30.
        let s = source_at(
            (100.0, 40.0),
            [0.0, 10.0],
            Size([SizeValue::fixed(20.0), SizeValue::fill()]),
            None,
        );
        assert!(
            s.contains("height: 30mm"),
            "expected the remainder above at.y: {s}"
        );

        // The same with an oversized max_h: min(35, 30) = 30, not 35.
        let s = source_at(
            (100.0, 40.0),
            [0.0, 10.0],
            Size([SizeValue::fixed(20.0), SizeValue::fill()]),
            Some(35.0),
        );
        assert!(
            s.contains("height: 30mm"),
            "the cap must not exceed the remainder: {s}"
        );

        // Width axis on a sheet slot: 40 - 5 = 35.
        let s = source_at(
            (40.0, 20.0),
            [5.0, 2.0],
            Size([SizeValue::fill(), SizeValue::fixed(8.0)]),
            None,
        );
        assert!(
            s.contains("width: 35mm"),
            "expected the remainder right of at.x: {s}"
        );
    }

    /// A refactor guard only. This PASSES against unfixed code, because the branch is already
    /// `(frame_width - left).max(0.0)`. It exists to catch a later rewrite that routes this branch
    /// through `resolve_size_value`, which rejects `<= 0` and would break a legitimate zero
    /// remainder. It is NOT a guard for #152.
    #[test]
    fn a_zero_remainder_container_renders_an_empty_box() {
        let source = dynamic_ctx_source(90.0, capped_container(90.0, Some(30.0), vec![]));
        assert!(
            source.contains("width: 0mm"),
            "a container with no room left renders an empty box rather than erroring: {source}"
        );
    }

    /// Spec §4.1. The Task 1 loosening admits this at load and it then fails at render, because the
    /// container has no width left for a divider. Like the test above, this PASSES after Task 1 and
    /// before Task 5 — it is not a per-step regression guard. It is here to pin *how* such a
    /// template fails: the standard explained error, not a panic and not a corrupt page.
    #[test]
    fn a_container_with_no_room_left_fails_cleanly_at_render() {
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [90.0, 0.0]\n    size: [fill, 12.0]\n    max_w: 30.0\n    items:\n      - type: line\n        at: [0.0, 6.0]\n        to: [-0.0, 6.0]\n        stroke:\n          thickness: 0.2\n";
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml).expect("parses");
        let template = crate::templates::TemplateContent::try_from(raw).expect("converts");
        assert_eq!(
            template.validate(),
            Ok(()),
            "the cap loosening admits this at load"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("a divider in a zero-width container cannot render");
        assert_eq!(
            err.reason(),
            Some("line_degenerate"),
            "expected line_degenerate, got: {}",
            err.message_text()
        );
    }

    #[test]
    fn a_strokeless_line_still_fails_its_render_checks() {
        // Same template as a_container_with_no_room_left_fails_cleanly_at_render but with stroke removed.
        // A strokeless line draws nothing yet still runs endpoint resolution and bounds checks.
        let yaml = "name: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [90.0, 0.0]\n    size: [fill, 12.0]\n    max_w: 30.0\n    items:\n      - type: line\n        at: [0.0, 6.0]\n        to: [-0.0, 6.0]\n";
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml).expect("parses");
        let template = crate::templates::TemplateContent::try_from(raw).expect("converts");
        assert_eq!(
            template.validate(),
            Ok(()),
            "strokeless variant also admitted at load"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("a strokeless degenerate line in a zero-width container must still fail");
        assert_eq!(
            err.reason(),
            Some("line_degenerate"),
            "expected line_degenerate for strokeless line, got: {}",
            err.message_text()
        );
    }

    /// A cap below the container's own padding leaves no inner box at all. When child items are
    /// inactive, the inner dimensions clamp at zero rather than going negative, emitting no
    /// negative dimensions.
    #[test]
    fn a_cap_smaller_than_the_padding_clamps_the_inner_box() {
        let item = LayoutItem::Container {
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::fill(),
                    SizeValue::fixed(12.0),
                ])),
                max_w: Some(2.0),
                max_h: None,
                rotate: None,
            },
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: crate::models::Padding {
                top: 3.0,
                right: 3.0,
                bottom: 3.0,
                left: 3.0,
            },
            flow: None,
            repeat: None,
            items: vec![LayoutItem::Container {
                placement: Placement {
                    at: Some(Position([-0.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::fill(),
                        SizeValue::fixed(1.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                when: Some(BTreeMap::from([("show".to_string(), "yes".to_string())])),
                shape: Shape::Rect,
                stroke: None,
                background: None,
                rounded: None,
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
                items: vec![],
            }],
        };
        let source = dynamic_ctx_source(100.0, item);
        assert!(
            source.contains("width: 2mm"),
            "the container still renders at its cap: {source}"
        );
        assert!(
            !source.contains("width: -") && !source.contains("height: -"),
            "no negative dimension may reach the emitted source: {source}"
        );
    }

    /// The cap must be inert when no bound is set. One assertion per site the branch capped, so a
    /// leak names the site. These pass before and after this branch; they exist to stay green.
    #[test]
    fn no_max_w_means_no_cap_anywhere() {
        // Text: an uncapped auto-width text measures its natural width against the full budget.
        let long = "a string long enough to have a natural width worth measuring";
        let (text_extent, _) = measured_extent_of(
            LayoutItem::Text {
                value: long.to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::content(),
                        SizeValue::fixed(8.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            },
            200.0,
        );
        assert!(text_extent > 0.0 && text_extent < 200.0);

        // Qr: an uncapped fill qr reports its intrinsic size.
        let (qr_extent, _) = measured_extent_of(
            LayoutItem::Qr {
                value: "abc".to_string(),
                placement: Placement {
                    at: Some(Position([10.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::fill(),
                        SizeValue::fixed(20.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: Some(crate::models::QrParams {
                    error_correction: None,
                    module_size: Some(1.0),
                    quiet_zone: None,
                }),
                when: None,
            },
            100.0,
        );
        assert_eq!(
            qr_extent, 31.0,
            "fill qr reports anchor + intrinsic (10 + 21 = 31)"
        );

        // Container, measurement: the child must be something whose measured width depends on
        // the inner budget, or the assertion proves nothing. An empty container contributes `at_x`
        // whatever the budget was, including a budget wrongly capped to zero.
        let child = LayoutItem::Text {
            value: long.to_string(),
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::content(),
                    SizeValue::fixed(8.0),
                ])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let (c_extent, _) = measured_extent_of(capped_container(10.0, None, vec![child]), 200.0);
        assert!(
            (c_extent - (10.0 + text_extent)).abs() < 0.5,
            "an uncapped container is sized by its child ({text_extent}mm at x=10), got {c_extent}"
        );

        // Container, render: uncapped, fills the frame remainder.
        let source = dynamic_ctx_source(100.0, capped_container(10.0, None, vec![]));
        assert!(
            source.contains("width: 90mm"),
            "an uncapped container fills the frame remainder: {source}"
        );
    }

    fn test_defaults(template: &TemplateContent) -> crate::models::ResolvedDefaults {
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now: chrono::Local::now(),
        };
        super::resolve_declared_defaults(template, &BTreeMap::new(), &dt)
    }

    fn test_inputs_all(template: &TemplateContent) -> Vec<crate::models::InputSpec> {
        let defaults = test_defaults(template);
        template.inputs_all(&defaults)
    }

    fn test_placeholder_data(
        template: &TemplateContent,
        now: chrono::DateTime<chrono::Local>,
    ) -> HashMap<String, serde_json::Value> {
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let defaults = super::resolve_declared_defaults(template, &BTreeMap::new(), &dt);
        template.placeholder_data(&defaults, now)
    }

    /// #152. `brother_24mm_weights.yaml` sets `max_w: 117` at `at.x: 1.5` on a `width.max: 120`
    /// tape, so the budget goes from 118.5 to 117 — a cap that binds by only 1.5mm. With the short
    /// placeholder data the suite uses, the render must be unchanged: this pins that a cap this
    /// close to the natural remainder does not perturb a real catalog/fixture template.
    #[test]
    fn brother_24mm_weights_render_is_unchanged_by_the_cap() {
        let registry = crate::templates::load_all_for_tests().0;
        let capped = registry.get("brother_24mm_weights").expect("template");
        let TemplateFormat::Single {
            width:
                DynamicDimension::Dynamic {
                    max: Some(DynamicValue::Literal(max_w)),
                    ..
                },
            ..
        } = &capped.format
        else {
            panic!("expected a dynamic-width single format");
        };
        assert_eq!(*max_w, 120.0, "budget math below assumes width.max: 120");
        let data = test_placeholder_data(capped, chrono::Local::now());
        let capped_png = render_thumbnail_png(capped, &data, &no_settings(), &no_datetime())
            .expect("render capped");

        // Same template, `max_w` stripped from both text items: the fallback remainder is
        // `width.max - at.x` = 118.5mm, so 117mm binds by only 1.5mm. With the short placeholder
        // text this suite uses, neither budget is the constraint that decides the fitted font
        // size or width, so the two renders must be pixel-identical.
        let mut uncapped = capped.clone();
        let Layout::Items(items) = &mut uncapped.layout;
        for item in items {
            if let LayoutItem::Text { placement, .. } = item {
                placement.max_w = None;
            }
        }
        let uncapped_png = render_thumbnail_png(&uncapped, &data, &no_settings(), &no_datetime())
            .expect("render uncapped");

        assert_eq!(
            capped_png, uncapped_png,
            "a 117mm cap on a 118.5mm remainder must not change the render with short placeholder text"
        );
    }

    fn to_text(at: [f32; 2], to: [f32; 2], value: &str) -> LayoutItem {
        LayoutItem::Text {
            value: value.to_string(),
            placement: Placement {
                at: Some(Position(at)),
                extent: crate::models::Extent::To(Position(to)),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        }
    }

    /// The whole point of #147: a text box that spans the label still sizes the label to its own
    /// content, so several full-width centered lines produce a label as wide as the longest one.
    #[test]
    fn an_edge_relative_to_text_contributes_its_natural_width() {
        let (extent, pushed) =
            measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42"), 80.0);
        assert!(
            extent > 0.0 && extent < 80.0,
            "expected a content-sized extent, got {extent}"
        );
        assert_eq!(
            pushed, 1,
            "an edge-relative to text is measured like an auto one"
        );
    }

    /// A right margin has to be paid for out of the label width, or the text is clipped by its own box.
    #[test]
    fn an_inset_to_text_contributes_its_natural_width_plus_the_inset() {
        let (plain, _) = measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42"), 80.0);
        let (inset, _) = measured_extent_of(to_text([0.0, 0.0], [-2.0, 8.0], "Widget A-42"), 80.0);
        assert!(
            (inset - (plain + 2.0)).abs() < 0.2,
            "expected {} + 2, got {inset}",
            plain
        );
    }

    /// A numeric `to` is a fixed width: known before the frame is, so it measures like `size:` and is
    /// rendered by fit_text_to_box, not replayed from a MeasuredText.
    #[test]
    fn a_numeric_to_text_measures_as_a_fixed_width() {
        let (extent, pushed) = measured_extent_of(
            to_text([0.0, 0.0], [30.0, 8.0], "text far too long for 30mm"),
            100.0,
        );
        assert_eq!(extent, 30.0);
        assert_eq!(
            pushed, 1,
            "an active text item creates a Measured node with text fit"
        );
    }

    /// A cap on an auto-width text must bind during measurement, since that is what sizes the label.
    /// Under left alignment (this test's `Alignment::default()`) the rendered box is also exactly
    /// what the measure pass recorded; under `center`/`right` the render pass applies the cap to the
    /// alignment slot itself (#180).
    #[test]
    fn max_w_caps_an_auto_width_text_during_measurement() {
        let long = "a string far too long to fit inside twenty millimetres of tape";
        fn text(max_w: Option<f32>, value: &str) -> LayoutItem {
            LayoutItem::Text {
                value: value.to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::content(),
                        SizeValue::fixed(8.0),
                    ])),
                    max_w,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }
        }
        let (uncapped, _) = measured_extent_of(text(None, long), 100.0);
        let (capped, pushed) = measured_extent_of(text(Some(20.0), long), 100.0);
        assert_eq!(pushed, 1);
        assert!(
            uncapped > 20.0,
            "the fixture must be long enough to exceed the cap, got {uncapped}"
        );
        assert!(
            capped <= 20.0 + 1.0e-3,
            "max_w must bind during measurement: measured {capped} against a 20mm cap"
        );
    }

    /// A capped qr sizes the label to its cap, not to `width.max`.
    #[test]
    fn max_w_caps_an_auto_width_qr_during_measurement() {
        let qr = |max_w: Option<f32>| LayoutItem::Qr {
            value: "abc".to_string(),
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::fill(),
                    SizeValue::fixed(20.0),
                ])),
                max_w,
                max_h: None,
                rotate: None,
            },
            params: Some(crate::models::QrParams {
                error_correction: None,
                module_size: Some(2.0),
                quiet_zone: None,
            }),
            when: None,
        };
        let (capped, pushed) = measured_extent_of(qr(Some(30.0)), 100.0);
        assert_eq!(pushed, 0, "a qr never records a text fit");
        assert_eq!(
            capped, 30.0,
            "a capped qr must contribute its cap, not the whole {}mm budget",
            100.0
        );
    }

    /// A slot expressed with edge-relative corners must measure the same as the identical slot
    /// expressed with plain ones. `at.y: -32` in a 40mm frame is y=8, so the slot is 32mm tall; mixing
    /// a resolved `to.y` with a raw `at.y` would compute 72mm, and the fitter would choose a font size
    /// the render-time box cannot fit.
    #[test]
    fn an_edge_relative_at_y_is_resolved_before_the_measure_height() {
        fn wrapped(at: [f32; 2], to: [f32; 2]) -> LayoutItem {
            LayoutItem::Text {
                value: "Some words that will wrap across several lines".to_string(),
                placement: Placement {
                    at: Some(Position(at)),
                    extent: crate::models::Extent::To(Position(to)),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Range {
                    min: 6.0,
                    max: 28.0,
                },
                font_weight: None,
                color: None,
                wrap: true,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }
        }
        // The frame is 40mm tall (see `measured_extent_of`), so these two describe the same 32mm slot.
        let (edge, _) = measured_extent_of(wrapped([0.0, -32.0], [-0.0, -0.0]), 60.0);
        let (plain, _) = measured_extent_of(wrapped([0.0, 8.0], [-0.0, 40.0]), 60.0);
        assert!(
            (edge - plain).abs() < 0.01,
            "the same slot measured {edge} with edge-relative corners and {plain} with plain ones"
        );
    }

    /// A container spanning to the right edge is measured by its children, like an auto-width one.
    /// Measuring it at its resolved width instead would peg every such label to its maximum.
    #[test]
    fn an_edge_relative_to_container_is_measured_by_its_children() {
        let (bare, _) = measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42"), 80.0);

        let container = LayoutItem::Container {
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: crate::models::Extent::To(Position([-0.0, 10.0])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: crate::models::Padding {
                top: 0.0,
                right: 1.0,
                bottom: 0.0,
                left: 1.0,
            },
            flow: None,
            repeat: None,
            items: vec![to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42")],
        };
        let (wrapped, pushed) = measured_extent_of(container, 80.0);
        assert_eq!(pushed, 1, "the child text is still measured exactly once");
        assert!(
            wrapped < 80.0,
            "the container was measured at its resolved width ({wrapped}), not by its children"
        );
        // 1mm of padding a side, and the child's budget shrinks by the same 2mm, so allow some slack.
        assert!(
            (wrapped - (bare + 2.0)).abs() < 0.5,
            "expected roughly the child width {bare} plus 2mm of padding, got {wrapped}"
        );
    }

    /// A qr spanning to the right edge contributes its intrinsic size.
    #[test]
    fn an_edge_relative_to_qr_contributes_its_intrinsic_size() {
        let (extent, pushed) = measured_extent_of(
            LayoutItem::Qr {
                value: "payload".to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::To(Position([-0.0, 8.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: Some(crate::models::QrParams {
                    error_correction: None,
                    module_size: Some(1.0),
                    quiet_zone: None,
                }),
                when: None,
            },
            80.0,
        );
        assert_eq!(extent, 21.0);
        assert_eq!(pushed, 0);
    }

    /// Carried over from Task 4's review: no unit test covered an edge-relative `at.x` on a `Qr`
    /// specifically (only `Text` had one). Clause 1 must skip it the same way regardless of item kind.
    #[test]
    fn an_edge_relative_at_x_on_a_qr_contributes_only_its_inset() {
        let (extent, pushed) = measured_extent_of(
            LayoutItem::Qr {
                value: "payload".to_string(),
                placement: Placement {
                    at: Some(Position([-5.0, 0.0])),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::fixed(10.0),
                        SizeValue::fixed(10.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
                when: None,
            },
            80.0,
        );
        assert_eq!(
            extent, 5.0,
            "an edge-relative at.x contributes only its inset"
        );
        assert_eq!(pushed, 0, "a qr never pushes a MeasuredText");
    }

    /// Measuring against the un-inset budget lets a text whose natural width reaches the budget
    /// contribute budget + inset. The page clamps back to `max`, and the text is then fitted into a
    /// box `inset` narrower than the width it was measured at, clipping it. The contribution must
    /// never exceed the budget it was measured against.
    #[test]
    fn an_inset_to_text_never_measures_wider_than_its_own_box() {
        let long = "a very long string that will not fit in forty millimetres at all";
        let (extent, pushed) = measured_extent_of(to_text([0.0, 0.0], [-2.0, 8.0], long), 40.0);
        assert_eq!(pushed, 1);
        assert!(
            extent <= 40.0 + 1.0e-3,
            "an inset item contributed {extent} against a 40mm budget: the inset was not subtracted \
             from the measure budget, so the label clamps to 40 and the text is clipped by 2mm"
        );
        // The measured text itself has to fit the box it will get: budget minus the inset.
        let (plain, _) = measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], long), 38.0);
        assert!(
            (extent - (plain + 2.0)).abs() < 0.2,
            "expected the inset contribution to be the 38mm-budget width plus 2, got {extent} vs {plain}"
        );
    }

    /// Review finding (code-reviewer, post-Task-8): clause 1 used to skip a right-anchored
    /// container's subtree entirely, so a frame-dependent child inside it (here, a `to`-spanned
    /// text) never got a `MeasuredText` pushed. `render_container_item` has no such skip and
    /// recurses unconditionally, so `render_text_item` then consumed a cursor entry that was never
    /// pushed and failed with "auto-length cursor overrun". The container's own width (`size:
    /// [8, 8]`) is fixed, so `validate_placement_position` allows pairing it with an edge-relative
    /// `at.x`; clause 1 must still measure the children against that known inner width.
    #[test]
    fn a_frame_dependent_child_inside_a_right_anchored_container_does_not_mismatch_the_cursor() {
        let template = TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(5.0),
                    max: Some(100.0),
                }
                .into(),
                height: Dimension::Fixed(8.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: Placement {
                    at: Some(Position([-10.0, 0.0])),
                    extent: Extent::Size(Size([SizeValue::fixed(8.0), SizeValue::fixed(8.0)])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                when: None,
                shape: Shape::Rect,
                stroke: None,
                background: None,
                rounded: None,
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
                items: vec![to_text([0.0, 0.0], [-0.0, 6.0], "x")],
            }]),
            version: None,
        };
        assert_eq!(
            template.validate(),
            Ok(()),
            "a fixed-width container paired with an edge-relative at.x is a legal shape"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        render_single_label(&template, &data, &no_settings(), &no_datetime()).expect(
            "a right-anchored container's frame-dependent child must still be measured, not \
             skipped along with the container",
        );
    }

    /// Review finding (code-reviewer, post-Task-8): Step 4 of the task brief routed the container's
    /// fixed-branch height through `resolve_size(..., allow_auto_fill: false)`, which has no
    /// fallback for an auto height with no `max_h`. `size: [40, auto]` is a documented container
    /// idiom (SPEC §4: "auto size resolves to `max_w`/`max_h` if present; for `container` it falls
    /// back to the parent frame"), accepted by `validate()` and rendered fine by
    /// `render_container_item` (which passes `allow_auto_fill: true`); only the measure pre-pass had
    /// been tightened, so every such container on a dynamic-width label started failing measurement
    /// with "size height is auto but no max_height provided".
    #[test]
    fn an_auto_height_fixed_width_container_measures_without_erroring() {
        let template = TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                }
                .into(),
                height: Dimension::Fixed(30.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: Extent::Size(Size([SizeValue::fixed(40.0), SizeValue::fill()])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
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
        assert_eq!(
            template.validate(),
            Ok(()),
            "`size: [40, auto]` is a documented container idiom"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        render_single_label(&template, &data, &no_settings(), &no_datetime()).expect(
            "an auto height with no max_h must fall back to the remaining frame height during \
             measurement, not error",
        );
    }

    #[test]
    fn r0_container_source_unchanged() {
        let container = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(80.0), SizeValue::fixed(40.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.3,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: None,
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[container], (80.0, 40.0)).expect("render r0 container");
        assert!(
            !src.contains("#rotate"),
            "R0 container must not emit #rotate"
        );
        assert!(
            src.contains("clip: true"),
            "R0 container keeps its single clipped box"
        );
    }

    fn rotated_container_template(rotate: f32, items: Vec<LayoutItem>) -> TemplateContent {
        TemplateContent {
            name: "Rot".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(80.0).into(),
                height: Dimension::Fixed(40.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: Extent::Size(Size([SizeValue::fixed(80.0), SizeValue::fixed(40.0)])),
                    max_w: None,
                    max_h: None,
                    rotate: Some(rotate),
                },
                when: None,
                shape: Shape::Rect,
                stroke: Some(Stroke {
                    thickness: 0.3,
                    color: DynamicValue::Literal(Color::black()),
                }),
                background: None,
                rounded: None,
                padding: Padding::ZERO,
                flow: None,
                repeat: None,
                items,
            }]),
            version: None,
        }
    }

    #[test]
    fn rotated_container_renders_to_png() {
        let template = rotated_container_template(
            90.0,
            vec![LayoutItem::Text {
                value: "VERTICAL".to_string(),
                placement: Placement::sized(
                    Position([2.0, 2.0]),
                    Size([SizeValue::fixed(30.0), SizeValue::fixed(8.0)]),
                ),
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }],
        );
        let data = HashMap::new();
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render rotated container");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    // Returns the dark-pixel fraction of each image quadrant: [TL, TR, BL, BR].
    fn quadrant_dark_fraction(png: &[u8]) -> [f32; 4] {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let (mw, mh) = (w / 2, h / 2);
        let mut dark = [0u32; 4];
        let mut total = [0u32; 4];
        for y in 0..h {
            for x in 0..w {
                let q = match (x < mw, y < mh) {
                    (true, true) => 0,
                    (false, true) => 1,
                    (true, false) => 2,
                    (false, false) => 3,
                };
                total[q] += 1;
                if img.get_pixel(x, y).0[0] < 128 {
                    dark[q] += 1;
                }
            }
        }
        [
            dark[0] as f32 / total[0] as f32,
            dark[1] as f32 / total[1] as f32,
            dark[2] as f32 / total[2] as f32,
            dark[3] as f32 / total[3] as f32,
        ]
    }

    #[test]
    fn rotation_ccw_corner_mapping_r90() {
        // A QR marker at the author canvas bottom-left (40x80 portrait); under CCW 90 it must land
        // in the physical bottom-right of the 80x40 label (spec table R90: BL -> BR).
        let template = rotated_container_template(
            90.0,
            vec![LayoutItem::Qr {
                value: "X".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(14.0), SizeValue::fixed(14.0)]),
                ),
                params: None,
                when: None,
            }],
        );
        let data = HashMap::new();
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render corner marker");
        let q = quadrant_dark_fraction(&png);
        assert!(
            q[3] > q[0] && q[3] > q[1] && q[3] > q[2],
            "QR at author BL must land physical BR under CCW 90; dark [TL,TR,BL,BR]={q:?}"
        );
    }

    #[test]
    fn rotation_ccw_corner_mapping_r180_and_r270() {
        let qr = || {
            vec![LayoutItem::Qr {
                value: "X".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(14.0), SizeValue::fixed(14.0)]),
                ),
                params: None,
                when: None,
            }]
        };
        let data = HashMap::new();

        // R180: author BL -> physical TR.
        let png = render_single_label(
            &rotated_container_template(180.0, qr()),
            &data,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render r180");
        let q = quadrant_dark_fraction(&png);
        assert!(
            q[1] > q[0] && q[1] > q[2] && q[1] > q[3],
            "R180 BL->TR; dark [TL,TR,BL,BR]={q:?}"
        );

        // R270: author BL -> physical TL.
        let png = render_single_label(
            &rotated_container_template(270.0, qr()),
            &data,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render r270");
        let q = quadrant_dark_fraction(&png);
        assert!(
            q[0] > q[1] && q[0] > q[2] && q[0] > q[3],
            "R270 BL->TL; dark [TL,TR,BL,BR]={q:?}"
        );
    }

    #[test]
    fn nested_rotated_containers_render() {
        // Outer R90 (frame + asymmetric author-space padding) containing an inner R90, frame-less
        // container with a text child. Proves nested rotation emits valid, compilable Typst.
        let inner = LayoutItem::Container {
            placement: Placement {
                at: Some(Position([2.0, 2.0])),
                extent: Extent::Size(Size([SizeValue::fixed(24.0), SizeValue::fixed(24.0)])),
                max_w: None,
                max_h: None,
                rotate: Some(90.0),
            },
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![LayoutItem::Text {
                value: "inner".to_string(),
                placement: Placement::sized(
                    Position([1.0, 1.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(8.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }],
        };
        let outer = LayoutItem::Container {
            placement: Placement {
                at: Some(Position([0.0, 0.0])),
                extent: Extent::Size(Size([SizeValue::fixed(80.0), SizeValue::fixed(40.0)])),
                max_w: None,
                max_h: None,
                rotate: Some(90.0),
            },
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.3,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: None,
            rounded: None,
            padding: Padding {
                top: 2.0,
                right: 4.0,
                bottom: 6.0,
                left: 8.0,
            },
            flow: None,
            repeat: None,
            items: vec![inner],
        };
        let template = TemplateContent {
            name: "Nest".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(80.0).into(),
                height: Dimension::Fixed(40.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![outer]),
            version: None,
        };
        let png = render_single_label(&template, &HashMap::new(), &no_settings(), &no_datetime())
            .expect("render nested rotated containers");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    /// An auto-length (dynamic-width) tape template whose single text item owns the whole
    /// `height_mm`-tall label, so the item's slot is exactly the rendered image. 180 dpi keeps the
    /// pixel geometry the same as the bundled brother tapes.
    fn autolength_tape(
        text: &str,
        wrap: bool,
        vertical: VerticalAlign,
        font_pt: f32,
    ) -> TemplateContent {
        const HEIGHT_MM: f32 = 20.0;
        TemplateContent {
            name: "Tape".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: Dimension::Fixed(HEIGHT_MM).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: text.to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::content(), SizeValue::fixed(HEIGHT_MM)]),
                ),
                font_size: FontSize::Fixed(font_pt),
                font_weight: None,
                color: None,
                wrap,
                line_spacing: None,
                alignment: Alignment {
                    horizontal: HorizontalAlign::Center,
                    vertical,
                },
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        }
    }

    /// A tape whose slot height is the caller's choice, so the same string can be rendered with room
    /// to spare and then in a slot tight enough that the old cap-height/baseline box would clip.
    fn tape_of_height(
        text: &str,
        vertical: VerticalAlign,
        font_pt: f32,
        height_mm: f32,
    ) -> TemplateContent {
        let mut t = autolength_tape(text, false, vertical, font_pt);
        t.format = TemplateFormat::Single {
            width: DynamicDimension::Dynamic {
                min: Some(DynamicValue::Literal(10.0)),
                max: Some(DynamicValue::Literal(200.0)),
            },
            height: Dimension::Fixed(height_mm).into(),
            media_width: None,
        };
        let Layout::Items(items) = &mut t.layout;
        if let Some(LayoutItem::Text { placement, .. }) = items.first_mut() {
            placement.extent =
                Extent::Size(Size([SizeValue::content(), SizeValue::fixed(height_mm)]));
        }
        t
    }

    fn ink_pixels(png: &[u8]) -> u64 {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        img.pixels().map(|p| (255 - p.0[0]) as u64).sum()
    }

    /// Clipping removes ink, so the same string at the same size must put the same ink on the page in
    /// a generous slot and in a tight one. Fixed `font_size` throughout: that bypasses the fitter, so
    /// this tests the *placement* pad alone — which is the half that fixes #124's reported defect.
    /// The reservation half is covered by the fitting tests in helpers.rs.
    #[test]
    fn ink_survives_a_tight_slot_at_top_and_bottom_alignment() {
        for (text, vertical) in [
            ("Édgy", VerticalAlign::Top),
            ("gjpqy", VerticalAlign::Bottom),
            ("Édgy", VerticalAlign::Bottom),
            ("gjpqy", VerticalAlign::Top),
        ] {
            // The control is a *centered* render in a roomy slot, not a taller slot at the same
            // alignment: bottom alignment puts the baseline on the slot floor however tall the slot
            // is, so an aligned control would clip exactly as much as the subject and the comparison
            // would be blind. Centered in 30mm, nothing can be cut.
            let generous = ink_pixels(&render_tape(&tape_of_height(
                text,
                VerticalAlign::Center,
                12.0,
                30.0,
            )));
            // 5.3mm just holds the 12pt ink band (1.21em = 5.12mm): enough room for the glyphs, but
            // only if the block is inset. Unpadded, the baseline sits on the slot edge and the
            // descenders or accents fall outside. A slot smaller than the band cannot be saved by
            // placement at a fixed font size, which is what the fitter reservation is for.
            let tight = ink_pixels(&render_tape(&tape_of_height(text, vertical, 12.0, 5.3)));
            let loss = (generous as f64 - tight as f64) / generous as f64;
            assert!(
                loss < 0.005,
                "{text} {vertical:?}: the tight slot lost {:.1}% of the ink",
                loss * 100.0
            );
        }
    }

    /// Count bands of inked rows separated by at least one blank row — i.e. how many lines of text
    /// actually landed on the page.
    fn ink_bands(png: &[u8]) -> usize {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let mut bands = 0;
        let mut inside = false;
        for y in 0..h {
            let inked = (0..w).any(|x| img.get_pixel(x, y).0[0] < 128);
            if inked && !inside {
                bands += 1;
            }
            inside = inked;
        }
        bands
    }

    /// #148 / #251: a newline in the data becomes a line on the label, whether wrap is true or false.
    #[test]
    fn a_newline_in_a_text_field_renders_as_two_lines_even_without_wrap() {
        let wrapped = render_tape(&autolength_tape(
            "one\ntwo",
            true,
            VerticalAlign::Center,
            12.0,
        ));
        assert_eq!(
            ink_bands(&wrapped),
            2,
            "a two-line value with wrap: true must put two lines of ink on the label"
        );

        let unwrapped = render_tape(&autolength_tape(
            "one\ntwo",
            false,
            VerticalAlign::Center,
            12.0,
        ));
        assert_eq!(
            ink_bands(&unwrapped),
            2,
            "a two-line value with wrap: false must still render both lines"
        );
    }

    /// First and last image rows carrying ink, plus the image height.
    fn ink_rows(png: &[u8]) -> (u32, u32, u32) {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let inked: Vec<u32> = (0..h)
            .filter(|&y| (0..w).any(|x| img.get_pixel(x, y).0[0] < 128))
            .collect();
        assert!(!inked.is_empty(), "rendered label has no ink");
        (inked[0], inked[inked.len() - 1], h)
    }

    fn render_tape(template: &TemplateContent) -> Vec<u8> {
        render_single_label(template, &HashMap::new(), &no_settings(), &no_datetime())
            .expect("render tape label")
    }

    /// #123: auto-length text placed its own box using fontdue's full line height (~1.21 em) while
    /// Typst lays the line out cap-height-to-baseline (~0.73 em) at the box top, so centered text
    /// floated ~0.24 em high. "test" has no descender, so its ink box is cap-height-to-baseline and
    /// centering it must put the ink centre on the slot centre. Two font sizes: the old error scaled
    /// with the em (~6.5 px at 12 pt, ~13 px at 24 pt), so any re-introduced metric-derived offset
    /// blows the tolerance at 24 pt even if it hid at 12 pt.
    #[test]
    fn autolength_text_centers_vertically() {
        for (label, wrap, text) in [
            ("single line", false, "test"),
            ("multiline", true, "test\ntest"),
        ] {
            for font_pt in [12.0, 24.0] {
                let png = render_tape(&autolength_tape(text, wrap, VerticalAlign::Center, font_pt));
                let (top, bottom, height) = ink_rows(&png);
                let offset = (top + bottom) as f32 / 2.0 - (height - 1) as f32 / 2.0;
                assert!(
                    offset.abs() <= 2.0,
                    "{label} at {font_pt}pt: ink rows {top}..{bottom} in {height}px label are off-centre by {offset:+.1}px"
                );
            }
        }
    }

    /// #133: alignment is baseline-relative, the industry norm. A fixed metric box (Typst's default
    /// cap-height→baseline, the same box CSS `text-box-trim` and Figma's vertical trim use) means the
    /// baseline lands in the same place no matter which glyphs a string contains — so `test`,
    /// `testj` and `es` sit on one line. #127 briefly centred the per-string ink box instead, which
    /// centred each label perfectly but let `j` and `t` move the baseline between labels.
    #[test]
    fn baseline_is_stable_across_glyph_classes() {
        // Strings with no descender end their ink ON the baseline, so the last inked row is a direct
        // read of where the baseline sits.
        let baseline_of = |text: &str| {
            let (_, bottom, _) = ink_rows(&render_tape(&autolength_tape(
                text,
                false,
                VerticalAlign::Center,
                18.0,
            )));
            bottom
        };
        let reference = baseline_of("test");
        for text in ["es", "MESSAGE", "Ml", "123"] {
            let got = baseline_of(text);
            assert!(
                got.abs_diff(reference) <= 1,
                "{text:?} put its baseline at row {got}, but \"test\" is at {reference}: \
                 alignment must not depend on which glyphs the string contains"
            );
        }

        // Descenders hang below that same baseline rather than moving it: the ink runs lower, by
        // about the descender depth, and by the SAME amount for every descender string.
        let with_desc: Vec<u32> = ["testj", "message", "typogy"]
            .iter()
            .map(|t| baseline_of(t))
            .collect();
        for (text, got) in ["testj", "message", "typogy"].iter().zip(&with_desc) {
            assert!(
                *got > reference,
                "{text:?} has a descender, so its ink must extend below the baseline ({got} vs {reference})"
            );
        }
        let spread = with_desc.iter().max().unwrap() - with_desc.iter().min().unwrap();
        assert!(
            spread <= 1,
            "descender strings must all hang the same distance below the baseline, spread was {spread}px"
        );
    }

    /// #251: blank edge lines are rendered, not trimmed (#127 superseded).
    /// A leading blank line shifts the visible text down by one line box.
    #[test]
    fn blank_edge_line_is_rendered_and_shifts_centering() {
        let plain = render_tape(&autolength_tape(
            "message",
            true,
            VerticalAlign::Center,
            18.0,
        ));
        let leading = render_tape(&autolength_tape(
            "\nmessage",
            true,
            VerticalAlign::Center,
            18.0,
        ));
        let (t1, _, _) = ink_rows(&plain);
        let (t2, _, _) = ink_rows(&leading);
        assert!(
            t2 > t1,
            "a leading blank line must shift text downwards ({t2} vs {t1})"
        );
    }

    /// Task 3.5: Render tests at a font size well away from the default (28pt vs default 10/12pt):
    /// a leading blank, an interior blank, a trailing blank and an empty value each produce a rendered
    /// block height matching what the fitter measured for the same value.
    ///
    /// Verifies at the Typst layout/render level: compiles the emitted Typst and asserts that
    /// the rendered height matches the fitter's block height, ensuring that:
    /// 1. The whole block is wrapped in `#text(size: {font_pt}pt...)`, so blank lines and fallbacks
    ///    inherit the fitted font size rather than ambient default (11pt).
    /// 2. Trailing blank lines emit a trailing `#linebreak()` so Typst allocates a box for them.
    #[test]
    fn rendered_block_height_matches_fitter_at_non_default_font_size() {
        use std::cell::RefCell;
        let font_pt = 28.0;
        let weight = 400;

        for (label, text, expected_lines) in [
            ("leading blank", "\nhello", 2),
            ("interior blank", "hello\n\nworld", 3),
            ("trailing blank", "hello\n", 2),
            ("empty value", "", 1),
        ] {
            let data: HashMap<String, super::JsonValue> = HashMap::new();
            let settings = no_settings();
            let datetime = no_datetime();
            let env = super::RenderEnv {
                settings: &settings,
                datetime: &datetime,
            };
            let images = RefCell::new(super::ImageCollector::default());
            let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);

            let item = LayoutItem::Text {
                value: text.to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::content(), SizeValue::content()]),
                ),
                font_size: FontSize::Fixed(font_pt),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            };

            let geometry_values = HashMap::new();
            let (measured, _) = ctx
                .measure_items(
                    std::slice::from_ref(&item),
                    (200.0, 100.0),
                    [true, true],
                    &geometry_values,
                    "layout",
                )
                .unwrap();

            let mut typst_rendered = String::new();
            let pbox = super::PlacedBox {
                x: 0.0,
                y: 0.0,
                w: 200.0,
                h: 100.0,
                frame: (200.0, 100.0),
            };
            let placement = match &item {
                LayoutItem::Text { placement, .. } => placement,
                _ => unreachable!(),
            };
            ctx.render_text_item(
                &mut typst_rendered,
                super::TextRenderArgs {
                    placement,
                    font_weight: None,
                    color: None,
                    alignment: &Alignment::default(),
                    line_spacing: None,
                    pbox,
                    text_fit: measured[0].text.as_ref().unwrap(),
                },
            )
            .unwrap();

            // 1. Assert emitted Typst structure: outer block wrapper carries font size and weight,
            // inner individual text nodes do not carry their own size.
            assert!(
                typst_rendered.contains(&format!("#text(size: {font_pt}pt")),
                "{label}: emitted Typst must wrap the block in #text(size: {font_pt}pt...): got {typst_rendered}"
            );

            // 2. Extract the emitted #text(size: ...) block and compile it in Typst on an auto-height page
            let start = typst_rendered
                .find(&format!("#text(size: {font_pt}pt"))
                .expect("found text wrapper");
            let mut depth = 0;
            let mut end = start;
            for (i, c) in typst_rendered[start..].char_indices() {
                if c == '[' {
                    depth += 1;
                } else if c == ']' {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i + 1;
                        break;
                    }
                }
            }
            let text_block = &typst_rendered[start..end];

            let probe_source = format!(
                "#set page(width: 200mm, height: auto, margin: 0mm)\n#set text(font: \"Inter\")\n{text_block}"
            );
            let rendered_h_pt = compile_probe(&probe_source).pages()[0]
                .frame
                .height()
                .to_pt() as f32;
            let predicted_h_pt =
                super::helpers::block_height_for_test(weight, font_pt, expected_lines);

            let drift = (rendered_h_pt - predicted_h_pt).abs() / predicted_h_pt;
            assert!(
                drift < 0.01,
                "{label}: Typst compiled height {rendered_h_pt:.2}pt vs predicted {predicted_h_pt:.2}pt ({:.1}% drift)",
                drift * 100.0
            );

            // 3. Verify intrinsic measurement matches predicted height
            let measured_h_mm = measured[0].intrinsic[1].expect("text measured height");
            let expected_h_pt = super::helpers::block_height_with_align_for_test(
                weight,
                font_pt,
                expected_lines,
                VerticalAlign::Top,
            );
            let expected_h_mm = super::helpers::pt_to_units_for_test(expected_h_pt, "mm");
            assert!(
                (measured_h_mm - expected_h_mm).abs() < 0.01,
                "{label}: measured {measured_h_mm}mm, expected {expected_h_mm}mm"
            );
        }
    }

    /// Guards the other two `alignment.vertical` values (ADR-0030 honours them literally), so a
    /// centering fix cannot hardcode centre.
    ///
    /// #124 turned "pinned to the edge" into "inset by the font's ink overflow", so this asserts the
    /// *metric* inset rather than contact. It deliberately does not require the ink to touch the
    /// slot edge: the pad is `ascender − cap_height` / `|descender|`, which overshoots `test`'s
    /// actual glyphs, so demanding contact would fail a correct implementation and push it toward
    /// glyph-dependent placement — the thing ADR-0050 rejects.
    #[test]
    fn autolength_text_top_and_bottom_pin_to_slot_edges() {
        let (top_first, top_last, height) = ink_rows(&render_tape(&autolength_tape(
            "test",
            false,
            VerticalAlign::Top,
            12.0,
        )));
        let (bottom_first, bottom_last, _) = ink_rows(&render_tape(&autolength_tape(
            "test",
            false,
            VerticalAlign::Bottom,
            12.0,
        )));
        // The pad is 0.2412em at 12pt = 2.89pt of an 18mm-tall tape rendered at `height` rows, plus
        // the cap-height gap `test` leaves under the ascender line. Bound it generously above and
        // require it to be non-zero below: zero would mean the pad never reached this path.
        let px_per_pt = height as f32 / super::helpers::units_to_pt_for_test(20.0, "mm");
        let pad_px = 0.2412 * 12.0 * px_per_pt;
        assert!(
            top_first as f32 >= pad_px * 0.5,
            "top-aligned ink must be inset by the pad, got row {top_first} (pad ≈ {pad_px:.1}px)"
        );
        assert!(
            (top_first as f32) < pad_px * 3.0,
            "top-aligned ink is far below the pad, got row {top_first} (pad ≈ {pad_px:.1}px)"
        );
        let bottom_gap = (height - 1 - bottom_last) as f32;
        assert!(
            bottom_gap >= pad_px * 0.5 && bottom_gap < pad_px * 3.0,
            "bottom-aligned ink must be inset by the pad, got gap {bottom_gap} (pad ≈ {pad_px:.1}px)"
        );
        assert!(
            bottom_first > top_last,
            "bottom alignment must sit below top alignment ({bottom_first} vs {top_last})"
        );
    }

    /// #137: the catalog index lists the request fields a template needs. `{vars.*}` and
    /// `{datetime.*}` resolve from the variables store and the datetime resolver, not the caller, so
    /// they must not appear — `homebox-qr` would otherwise advertise `vars.qr_base_url` as something
    /// the user has to supply.
    #[test]
    fn template_fields_lists_request_keys_only() {
        let registry = crate::templates::load_all_for_tests().0;
        let t = registry.get("homebox-qr").expect("homebox-qr");
        let fields: Vec<String> = test_inputs_all(t)
            .into_iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert_eq!(fields, vec!["id".to_string(), "message".to_string()]);
    }

    fn no_settings() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn no_datetime() -> crate::datetime_fmt::DateTimeResolver<'static> {
        use std::sync::OnceLock;
        static EMPTY: OnceLock<std::collections::BTreeMap<String, String>> = OnceLock::new();
        let formats = EMPTY.get_or_init(std::collections::BTreeMap::new);
        crate::datetime_fmt::DateTimeResolver {
            formats,
            now: chrono::Local::now(),
        }
    }

    fn two_slot_sheet() -> TemplateDefinition {
        TemplateDefinition {
            id: "sheet2".to_string(),
            group: None,
            content: TemplateContent {
                name: "Sheet2".to_string(),
                description: String::new(),
                unit: "mm".to_string(),
                dpi: 200,
                format: TemplateFormat::Sheet {
                    paper_width: 20.0,
                    paper_height: 10.0,
                    label_width: 10.0,
                    label_height: 10.0,
                    positions: vec![SheetPosition([0.0, 0.0]), SheetPosition([10.0, 0.0])],
                },
                params: IndexMap::from([(
                    "message".to_string(),
                    ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                )]),
                layout: Layout::Items(vec![LayoutItem::Text {
                    value: "{message}".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(10.0), SizeValue::fixed(10.0)]),
                    ),
                    font_size: FontSize::Fixed(8.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                }]),
                version: None,
            },
        }
    }

    fn sheet_label(msg: &str) -> LabelInput {
        LabelInput {
            data: HashMap::from([("message".to_string(), json!(msg))]),
        }
    }

    #[test]
    fn sheet_pages_paginate_overflow() {
        let labels = vec![sheet_label("a"), sheet_label("b"), sheet_label("c")];
        let pdf = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            0,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render");
        assert!(pdf.starts_with(b"%PDF"));
        assert_eq!(count_pdf_pages(&pdf), 2);
    }

    #[test]
    fn sheet_pages_respect_start_slot() {
        let labels = vec![sheet_label("a"), sheet_label("b")];
        let pdf = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            1,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render");
        assert!(pdf.starts_with(b"%PDF"));
        assert_eq!(count_pdf_pages(&pdf), 2);
    }

    #[test]
    fn sheet_pages_collect_bad_label_index() {
        let labels = vec![
            sheet_label("a"),
            LabelInput {
                data: HashMap::new(),
            },
        ];
        let err = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            0,
            &no_settings(),
            &no_datetime(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
    }

    #[test]
    fn render_single_label_produces_png() {
        let template = TemplateContent {
            name: "Test".to_string(),
            description: "Test template".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(10.0).into(),
                media_width: None,
            },
            params: IndexMap::from([(
                "variant".to_string(),
                ParamSpec {
                    param_type: ParamType::Enum {
                        values: vec!["default".to_string()],
                    },
                    description: None,
                    default: None,
                    min: None,
                    max: None,
                },
            )]),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "{message}".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };

        let data = HashMap::from([("message".to_string(), json!("Hello"))]);
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render label");

        assert!(!png.is_empty(), "rendered PNG is empty");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_single_label_with_qr_produces_png() {
        let template = TemplateContent {
            name: "Test QR".to_string(),
            description: "Test template with qr".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(30.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: IndexMap::from([(
                "variant".to_string(),
                ParamSpec {
                    param_type: ParamType::Enum {
                        values: vec!["default".to_string()],
                    },
                    description: None,
                    default: None,
                    min: None,
                    max: None,
                },
            )]),
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    value: "{message}".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                    ),
                    font_size: FontSize::Fixed(10.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                },
                LayoutItem::Qr {
                    value: "{code}".to_string(),
                    placement: Placement::sized(
                        Position([20.0, 0.0]),
                        Size([SizeValue::fixed(10.0), SizeValue::fixed(10.0)]),
                    ),
                    params: None,
                    when: None,
                },
                LayoutItem::Line {
                    at: Position([0.0, 1.0]),
                    to: Position([30.0, 1.0]),
                    stroke: Some(Stroke {
                        thickness: 0.2,
                        color: DynamicValue::Literal(Color::black()),
                    }),
                    when: None,
                },
                LayoutItem::Container {
                    placement: Placement::sized(
                        Position([0.5, 1.5]),
                        Size([SizeValue::fixed(29.0), SizeValue::fixed(18.0)]),
                    ),
                    when: None,
                    shape: Shape::Rect,
                    stroke: Some(Stroke {
                        thickness: 0.2,
                        color: DynamicValue::Literal(Color::black()),
                    }),
                    background: None,
                    rounded: Some(0.4),
                    padding: Padding::ZERO,
                    flow: None,
                    repeat: None,
                    items: Vec::new(),
                },
            ]),
            version: None,
        };

        let data = HashMap::from([
            ("message".to_string(), json!("Hello")),
            ("code".to_string(), json!("QR-123")),
        ]);
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render label with qr");

        assert!(!png.is_empty(), "rendered PNG is empty");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_sheet_labels_produces_pdf() {
        let template = TemplateDefinition {
            id: "sheet".to_string(),
            group: None,
            content: TemplateContent {
                name: "Sheet".to_string(),
                description: "Sheet template".to_string(),
                unit: "mm".to_string(),
                dpi: 200,
                format: TemplateFormat::Sheet {
                    paper_width: 10.0,
                    paper_height: 5.0,
                    label_width: 10.0,
                    label_height: 5.0,
                    positions: vec![SheetPosition([0.0, 0.0])],
                },
                params: IndexMap::from([(
                    "message".to_string(),
                    ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                )]),
                layout: Layout::Items(vec![LayoutItem::Text {
                    value: "{message}".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(10.0), SizeValue::fixed(5.0)]),
                    ),
                    font_size: FontSize::Fixed(10.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                }]),
                version: None,
            },
        };

        let labels = vec![LabelInput {
            data: HashMap::from([("message".to_string(), json!("Hello"))]),
        }];

        let pdf = render_sheet_pages(&template, &labels, 0, &no_settings(), &no_datetime())
            .expect("render sheet");

        assert!(!pdf.is_empty(), "rendered PDF is empty");
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
    }

    const PNG_1X1_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

    fn image_single_template() -> TemplateContent {
        TemplateContent {
            name: "Img".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Image {
                name: Some("logo".to_string()),
                src: None,
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                ),
                fit: Fit::Contain,
                when: None,
            }]),
            version: None,
        }
    }

    #[test]
    fn render_single_label_with_image_produces_png() {
        let template = image_single_template();
        let data = HashMap::from([(
            "logo".to_string(),
            json!(format!("data:image/png;base64,{PNG_1X1_B64}")),
        )]);
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render image");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_image_missing_data_errors() {
        let template = image_single_template();
        let data = HashMap::new();
        assert!(render_single_label(&template, &data, &no_settings(), &no_datetime()).is_err());
    }

    #[test]
    fn render_image_invalid_base64_errors() {
        let template = image_single_template();
        let data = HashMap::from([(
            "logo".to_string(),
            json!("data:image/png;base64,@@@not-base64@@@"),
        )]);
        assert!(render_single_label(&template, &data, &no_settings(), &no_datetime()).is_err());
    }

    #[test]
    fn render_sheet_labels_with_image_produces_pdf() {
        let template = TemplateDefinition {
            id: "sheet".to_string(),
            group: None,
            content: TemplateContent {
                name: "Sheet".to_string(),
                description: String::new(),
                unit: "mm".to_string(),
                dpi: 200,
                format: TemplateFormat::Sheet {
                    paper_width: 20.0,
                    paper_height: 20.0,
                    label_width: 20.0,
                    label_height: 20.0,
                    positions: vec![SheetPosition([0.0, 0.0])],
                },
                params: IndexMap::from([(
                    "logo".to_string(),
                    ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                )]),
                layout: Layout::Items(vec![LayoutItem::Image {
                    name: Some("logo".to_string()),
                    src: None,
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                    ),
                    fit: Fit::Contain,
                    when: None,
                }]),
                version: None,
            },
        };
        let labels = vec![LabelInput {
            data: HashMap::from([(
                "logo".to_string(),
                json!(format!("data:image/png;base64,{PNG_1X1_B64}")),
            )]),
        }];
        let pdf = render_sheet_pages(&template, &labels, 0, &no_settings(), &no_datetime())
            .expect("render sheet image");
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
    }

    /// The motivating case for #151. Two unrelated failures share `UnsupportedLayoutItem`: a bad
    /// image payload and a geometry violation. Before `details.reason` the only way to tell them
    /// apart was the prose, so a client could not act on either, and a test asserting one could pass
    /// against the other. This is the test that would have failed before the change.
    #[test]
    fn one_code_two_reasons_for_unrelated_failures() {
        let template = image_single_template();
        let data = HashMap::from([("logo".to_string(), json!("not a data uri"))]);
        let image_err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("a non-data-URI image payload must not render");

        let geometry_err = render_test_items(
            &[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([30.0, 6.0]),
                stroke: Some(Stroke {
                    thickness: 0.2,
                    color: DynamicValue::Literal(Color::black()),
                }),
                when: None,
            }],
            (10.0, 12.0),
        )
        .expect_err("a 30mm endpoint on a 10mm frame must not render");

        assert_eq!(image_err.code(), geometry_err.code());
        assert_eq!(image_err.code(), "UnsupportedLayoutItem");
        assert_eq!(image_err.reason(), Some("image_data_invalid"));
        assert_eq!(geometry_err.reason(), Some("line_endpoint_out_of_frame"));
    }

    fn image_single_template_with_src(src: &str) -> TemplateContent {
        let mut template = image_single_template();
        template.layout = Layout::Items(vec![LayoutItem::Image {
            name: None,
            src: Some(src.to_string()),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
            ),
            fit: Fit::Contain,
            when: None,
        }]);
        template
    }

    #[test]
    fn render_single_label_with_svg_data_uri_produces_png() {
        use base64::Engine as _;
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>";
        let uri = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        );
        let template = image_single_template();
        let data = HashMap::from([("logo".to_string(), json!(uri))]);
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render svg");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_static_image_src() {
        use base64::Engine as _;
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cfg = std::env::temp_dir().join(format!("labeler_render_cfg_{n}"));
        let assets_dir = cfg.join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(PNG_1X1_B64)
            .unwrap();
        std::fs::write(assets_dir.join("logo.png"), &bytes).unwrap();
        std::env::set_var("LABELER_CONFIG_DIR", &cfg);

        let data = HashMap::new();
        let png = render_single_label(
            &image_single_template_with_src("logo.png"),
            &data,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render static src");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // A missing asset is rejected at render time.
        assert!(render_single_label(
            &image_single_template_with_src("missing.png"),
            &data,
            &no_settings(),
            &no_datetime(),
        )
        .is_err());

        std::env::remove_var("LABELER_CONFIG_DIR");
        std::fs::remove_dir_all(&cfg).ok();
    }

    #[test]
    fn render_single_label_produces_pdf() {
        let template = TemplateContent {
            name: "Pdf".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: Dimension::Fixed(10.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "{message}".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        let data = HashMap::from([("message".to_string(), json!("Hello"))]);
        let pdf = render_single_label_pdf(&template, &data, &no_settings(), &no_datetime())
            .expect("render pdf");
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
    }

    /// #135: every template in either root must parse, validate and render — the five catalog
    /// entries and the five fixtures alike. Deliberately wider than the catalog: `brother_18mm_qr`,
    /// `brother_9mm` and `brother_18mm` have zero references anywhere in `src/`, so this gate is the
    /// only thing proving they work. An exact set, not a floor: "render whatever the loader found"
    /// passes vacuously the moment a root is misconfigured and the loader quietly returns fewer.
    #[test]
    fn every_template_renders() {
        let registry = crate::templates::load_all_for_tests().0;
        // Bind the Vec: `summaries()` returns by value, so borrowing `&str` straight out of the
        // call expression drops the temporary while the set still holds references (E0716).
        let summaries = registry.summaries();
        let found: BTreeSet<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
        let expected: BTreeSet<&str> = BTreeSet::from([
            "avery5163",
            "avery5163_asset_tag",
            "brother_12mm",
            "brother_18mm",
            "brother_18mm_qr",
            "brother_24mm",
            "brother_24mm_lines_divider",
            "brother_24mm_max_w_cap",
            "brother_24mm_multiline",
            "brother_24mm_printed_on",
            "brother_24mm_qr",
            "brother_24mm_weights",
            "brother_9mm",
            "container_circle_content",
            "container_circle_gated",
            "container_circle_param",
            "container_default_rect",
            "container_ellipse_padded",
            "container_ellipse_square",
            "container_ellipse_stroked_cross",
            "container_rect_rounded_corner",
            "container_rect_stroked_edge",
            "homebox-qr",
        ]);
        assert_eq!(
            found, expected,
            "template roots do not hold the expected set"
        );
        // homebox-qr interpolates {vars.qr_base_url} and {sys.now:iso_date}; brother_24mm_printed_on
        // interpolates {printed_on:short_date} off a `datetime` parameter. Supply all of them so the
        // demo entries are covered rather than skipped.
        let settings =
            BTreeMap::from([("qr_base_url".to_string(), "https://example.com".to_string())]);
        let formats = BTreeMap::from([
            ("iso_date".to_string(), "%Y-%m-%d".to_string()),
            ("short_date".to_string(), "%m/%d/%Y".to_string()),
        ]);
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now: chrono::Local::now(),
        };
        for summary in registry.summaries() {
            let template = registry.get(&summary.id).expect("template");
            let data = test_placeholder_data(template, dt.now);
            let png = render_thumbnail_png(template, &data, &settings, &dt)
                .unwrap_or_else(|e| panic!("render {}: {e:?}", summary.id));
            assert_eq!(
                &png[..8],
                b"\x89PNG\r\n\x1a\n",
                "{} did not render a PNG",
                summary.id
            );
        }
    }

    fn walk_templates(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
            let path = entry.expect("entry").path();
            let meta = std::fs::symlink_metadata(&path).expect("stat template entry");
            if meta.is_dir() {
                walk_templates(&path, out);
            } else if path.extension().is_some_and(|x| x == "yaml" || x == "yml") {
                out.push(path);
            }
        }
    }

    /// #135: the catalog is the product surface and is designed, not accreted. An exact set rather
    /// than a count: it names what ships, and it fails on a silent rename as well as an addition.
    /// Fixtures live in `tests/fixtures/templates/` and must never appear here.
    #[test]
    fn catalog_is_exactly_the_starter_set() {
        let mut files = Vec::new();
        walk_templates(std::path::Path::new("catalog"), &mut files);
        let found: BTreeSet<String> = files
            .iter()
            .map(|p| p.file_stem().expect("stem").to_string_lossy().to_string())
            .collect();
        let expected: BTreeSet<String> = [
            "avery5163",
            "brother_12mm",
            "brother_18mm",
            "brother_24mm",
            "brother_9mm",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            found, expected,
            "catalog contents changed; update this gate deliberately. ADR-0047 recorded the original set and is frozen, so it is not updated with it"
        );
    }

    /// Ids are the API key, the `/print/{id}` route and what print webhooks hardcode, and installs
    /// land flat in `{config}/templates` — so a duplicate id anywhere in the nested catalog would
    /// collide on install, and an id that differs from its filename would install under a name the
    /// catalog does not know (#137).
    #[test]
    fn template_ids_are_unique_and_match_filenames() {
        let mut files = Vec::new();
        walk_templates(std::path::Path::new("catalog"), &mut files);
        // Both roots flatten into one dir at test time, so a cross-root duplicate would overwrite a
        // file before `load_from_dir` could ever see two ids (#135).
        walk_templates(std::path::Path::new("tests/fixtures/templates"), &mut files);
        assert!(!files.is_empty(), "no templates found");

        let mut seen: HashMap<String, std::path::PathBuf> = HashMap::new();
        for path in files {
            let stem = path
                .file_stem()
                .expect("stem")
                .to_string_lossy()
                .to_string();
            assert!(
                crate::templates::validate_template_id_stem(&stem),
                "{path:?}: stem must be a valid template id stem"
            );
            if let Some(prev) = seen.insert(stem.clone(), path.clone()) {
                panic!("duplicate catalog id {stem}: {prev:?} and {path:?}");
            }
        }
    }

    #[test]
    fn render_value_text_and_qr_interpolate() {
        let template = TemplateContent {
            name: "Interp".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    value: "Item {id}".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 10.0]),
                        Size([SizeValue::fixed(40.0), SizeValue::fixed(8.0)]),
                    ),
                    font_size: FontSize::Fixed(8.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                },
                LayoutItem::Qr {
                    value: "{vars.qr_base_url}/{id}".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(10.0), SizeValue::fixed(10.0)]),
                    ),
                    params: None,
                    when: None,
                },
            ]),
            version: None,
        };
        let data = HashMap::from([("id".to_string(), json!("A1"))]);
        let settings = BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())]);
        let png = render_single_label(&template, &data, &settings, &no_datetime())
            .expect("render interp");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // Missing setting is an error.
        assert!(render_single_label(&template, &data, &no_settings(), &no_datetime()).is_err());
    }

    #[test]
    fn interpolated_data_cannot_inject_typst() {
        let template = TemplateContent {
            name: "Inject".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(60.0).into(),
                height: Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "{x}".to_string(),
                placement: Placement::sized(
                    Position([0.0, 6.0]),
                    Size([SizeValue::fixed(60.0), SizeValue::fixed(8.0)]),
                ),
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        // Typst-hostile payload: markup that would call into the system if not escaped.
        let data = HashMap::from([("x".to_string(), json!(r#""]#sys.version[ \ end"#))]);
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("render escaped");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn homebox_qr_template_renders() {
        let registry = crate::templates::load_all_for_tests().0;
        let template = registry.get("homebox-qr").expect("template homebox-qr");
        let data = HashMap::from([
            ("id".to_string(), json!("A1")),
            ("message".to_string(), json!("Widget")),
        ]);
        let settings = BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())]);
        let dt_formats = crate::settings::default_datetime_formats();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now: chrono::Local::now(),
        };
        let png = render_single_label(template, &data, &settings, &dt).expect("render homebox-qr");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // Missing qr_base_url setting is an error.
        assert!(render_single_label(template, &data, &no_settings(), &dt).is_err());
    }

    #[test]
    fn render_thumbnail_of_sheet_is_label_sized() {
        let template = sheet_template_10x5_on_100x100();
        let data = HashMap::new();
        let settings = BTreeMap::new();
        let png = render_thumbnail_png(&template, &data, &settings, &no_datetime()).expect("png");
        let img = image::load_from_memory(&png).expect("decode png");
        // label 10x5 mm at 96 dpi ≈ 37.8 x 18.9 px; paper would be ~378 px. Assert it is the label box.
        assert!(
            img.width() > 20 && img.width() < 60,
            "width {} should be ~38px (label 10mm@96dpi), not paper-sized",
            img.width()
        );
        assert!(
            img.height() > 10 && img.height() < 30,
            "height {} should be ~19px (label 5mm@96dpi), not paper-sized",
            img.height()
        );
    }

    fn sheet_template_10x5_on_100x100() -> TemplateContent {
        use crate::models::{Alignment, FontSize, Position, SheetPosition, Size, SizeValue};
        TemplateContent {
            name: "s".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Sheet {
                paper_width: 100.0,
                paper_height: 100.0,
                label_width: 10.0,
                label_height: 5.0,
                positions: vec![SheetPosition([0.0, 0.0])],
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "hi".into(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(10.0), SizeValue::fixed(5.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        }
    }

    #[test]
    fn placeholder_data_includes_interpolated_keys_only() {
        use crate::models::{Alignment, Fit, FontSize, Position, Size, SizeValue};
        let template = TemplateContent {
            name: "t".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(40.0).into(),
                height: crate::models::Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: IndexMap::from([
                (
                    "title".into(),
                    crate::models::ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                ),
                (
                    "url".into(),
                    crate::models::ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                ),
                (
                    "logo".into(),
                    crate::models::ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                    },
                ),
            ]),
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    value: "{title} {url} {vars.base} {sys.now} {sys.now:short_date}".into(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(40.0), SizeValue::fixed(10.0)]),
                    ),
                    font_size: FontSize::Fixed(6.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                },
                LayoutItem::Image {
                    name: Some("logo".into()),
                    src: None,
                    placement: Placement::sized(
                        Position([0.0, 10.0]),
                        Size([SizeValue::fixed(5.0), SizeValue::fixed(5.0)]),
                    ),
                    fit: Fit::default(),
                    when: None,
                },
            ]),
            version: None,
        };
        let data = test_placeholder_data(&template, chrono::Local::now());
        assert_eq!(data.get("title").and_then(|v| v.as_str()), Some("title"));
        assert_eq!(data.get("url").and_then(|v| v.as_str()), Some("url"));
        assert!(!data.contains_key("base"), "vars.* must be excluded");
        assert!(!data.contains_key("vars.base"), "vars.* must be excluded");
        assert!(
            !data.contains_key("sys.now"),
            "sys namespace must be excluded"
        );
        assert!(
            !data.contains_key("sys.now:short_date"),
            "sys namespace must be excluded"
        );
        assert!(!data.contains_key("now"), "sys namespace must be excluded");
        assert_eq!(
            data.get("logo").and_then(|v| v.as_str()),
            Some(SAMPLE_PNG_DATA_URI)
        );
    }

    #[test]
    fn placeholder_data_skips_empty_token() {
        use crate::models::{Alignment, FontSize, Position, Size, SizeValue};
        let template = TemplateContent {
            name: "t".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(40.0).into(),
                height: crate::models::Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: IndexMap::from([(
                "real".into(),
                crate::models::ParamSpec {
                    param_type: crate::models::ParamType::String { multiline: false },
                    default: None,
                    min: None,
                    max: None,
                    description: None,
                },
            )]),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "{} {real}".into(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(40.0), SizeValue::fixed(20.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        let data = test_placeholder_data(&template, chrono::Local::now());
        assert!(
            !data.contains_key(""),
            "empty token must not produce an empty-string key"
        );
        assert_eq!(
            data.get("real").and_then(|v| v.as_str()),
            Some("real"),
            "real token must be collected"
        );
    }

    #[test]
    fn interpolate_datetime_tokens() {
        use crate::datetime_fmt::DateTimeResolver;
        use chrono::TimeZone;
        use std::collections::{BTreeMap, HashMap};

        let now = chrono::Local
            .with_ymd_and_hms(2026, 6, 25, 14, 30, 0)
            .single()
            .unwrap();
        let formats = BTreeMap::from([("short_date".to_string(), "%m/%d/%Y".to_string())]);
        let dt = DateTimeResolver {
            formats: &formats,
            now,
        };
        let vars = BTreeMap::from([("site".to_string(), "https://example.com".to_string())]);
        let mut data: HashMap<String, serde_json::Value> = HashMap::new();
        data.insert(
            "datetime".to_string(),
            serde_json::json!("my-datetime-data"),
        );
        data.insert("vars".to_string(), serde_json::json!("my-vars-data"));

        // bare sys.now => ISO date
        assert_eq!(
            super::helpers::interpolate("d={sys.now}", &data, &vars, &dt, None).unwrap(),
            "d=2026-06-25"
        );
        // named format on sys.now
        assert_eq!(
            super::helpers::interpolate("{sys.now:short_date}", &data, &vars, &dt, None).unwrap(),
            "06/25/2026"
        );
        // unknown named format on sys.now => error
        assert!(super::helpers::interpolate("{sys.now:nope}", &data, &vars, &dt, None).is_err());

        // bare name `datetime` resolves data field
        assert_eq!(
            super::helpers::interpolate("{datetime}", &data, &vars, &dt, None).unwrap(),
            "my-datetime-data"
        );
        // bare name `vars` resolves data field
        assert_eq!(
            super::helpers::interpolate("{vars}", &data, &vars, &dt, None).unwrap(),
            "my-vars-data"
        );
        // vars.<key> resolves variables
        assert_eq!(
            super::helpers::interpolate("{vars.site}", &data, &vars, &dt, None).unwrap(),
            "https://example.com"
        );

        // literal braces unaffected
        assert_eq!(
            super::helpers::interpolate("{{sys.now}}", &data, &vars, &dt, None).unwrap(),
            "{sys.now}"
        );
    }

    /// Engine-upgrade visual harness (#101): dumps a label-sized PNG for every bundled template
    /// (both avery orientations) into $LABELER_RENDER_DUMP_DIR. Run explicitly:
    /// LABELER_RENDER_DUMP_DIR=.render-scratch/pre-015 cargo test --lib dump_all_template_renders -- --ignored
    #[test]
    #[ignore = "env-gated render dump for engine-upgrade visual comparison"]
    fn dump_all_template_renders() {
        let Some(dir) = std::env::var_os("LABELER_RENDER_DUMP_DIR") else {
            panic!("set LABELER_RENDER_DUMP_DIR");
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("create dump dir");
        let registry = crate::templates::load_all_for_tests().0;
        // homebox-qr interpolates {vars.qr_base_url}; placeholder_data excludes variables by design.
        let settings =
            BTreeMap::from([("qr_base_url".to_string(), "https://example.com".to_string())]);
        // homebox-qr also references {sys.now:iso_date}, a named format; supply it so the harness
        // resolves it (no_datetime carries no named formats).
        let datetime_formats = BTreeMap::from([
            ("iso_date".to_string(), "%Y-%m-%d".to_string()),
            ("short_date".to_string(), "%m/%d/%Y".to_string()),
        ]);
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &datetime_formats,
            now: chrono::Local::now(),
        };
        for summary in registry.summaries() {
            let template = registry.get(&summary.id).expect("template");
            let mut base_data = test_placeholder_data(template, datetime.now);
            // Engine-upgrade visual baseline, not thumbnail-spec: keep the avery outline
            // container covered even though thumbnails no longer draw it (undefaulted gate
            // via `placeholder_data`'s `interpolated && required` rule). The deleted option
            // map supplied `outline: yes` for every enum; this restores that one branch for
            // the dump only, and only when the param is an enum whose values contain "yes".
            if let Some(spec) = template.params.get("outline") {
                if let crate::models::ParamType::Enum { values } = &spec.param_type {
                    if values.contains(&"yes".to_string()) {
                        base_data.insert(
                            "outline".to_string(),
                            serde_json::Value::String("yes".to_string()),
                        );
                    }
                }
            }
            let variants: Vec<(String, HashMap<String, serde_json::Value>)> = match template
                .params
                .get("orientation")
                .and_then(|spec| match &spec.param_type {
                    crate::models::ParamType::Enum { values } => Some(values.clone()),
                    _ => None,
                }) {
                Some(orientations) => orientations
                    .into_iter()
                    .map(|o| {
                        let mut data = base_data.clone();
                        data.insert(
                            "orientation".to_string(),
                            serde_json::Value::String(o.clone()),
                        );
                        (format!("{}-{o}", summary.id), data)
                    })
                    .collect(),
                None => vec![(summary.id.clone(), base_data.clone())],
            };
            for (name, data) in variants {
                let png = render_thumbnail_png(template, &data, &settings, &datetime)
                    .unwrap_or_else(|e| panic!("render {name}: {e:?}"));
                std::fs::write(dir.join(format!("{name}.png")), png).expect("write png");
            }
        }
    }

    /// Compile a probe source and hand back the document. Shares `weight_probe_ink`'s
    /// zero-warnings assertion: a missing bundled Inter would otherwise resolve through the
    /// embedded fallback and quietly calibrate the fitter against the wrong font.
    fn compile_probe(source: &str) -> super::PagedDocument {
        let engine = super::TypstEngine::builder()
            .main_file(source.to_string())
            .search_fonts_with(super::typst_font_options())
            .build();
        let warned = engine.compile::<super::PagedDocument>();
        assert!(
            warned.warnings.is_empty(),
            "probe must compile without warnings: {:?}",
            warned.warnings
        );
        warned.output.expect("compile probe")
    }

    /// Lay out `lines` lines at `size` on an auto-height page with no margin: the page height Typst
    /// produces *is* the block height the fitter has to predict. Ink extents are the wrong quantity
    /// here — they include descenders and exclude leading.
    fn typst_block_height_pt(lines: usize, size: f32, line_spacing: Option<f32>) -> f32 {
        let body = (0..lines)
            .map(|_| "Hxy")
            .collect::<Vec<_>>()
            .join("#linebreak()");
        let leading =
            super::helpers::derived_leading_pt(400, size, line_spacing).expect("derived leading");
        let source = format!(
            "#set page(width: 200mm, height: auto, margin: 0mm)\n#set text(font: \"Inter\", size: {size}pt)\n#set par(leading: {leading}pt)\n{body}"
        );
        compile_probe(&source).pages()[0].frame.height().to_pt() as f32
    }

    fn typst_line_width_pt(text: &str, size: f32) -> f32 {
        let source = format!(
            "#set page(width: auto, height: auto, margin: 0mm)\n#set text(font: \"Inter\", size: {size}pt)\n{text}"
        );
        compile_probe(&source).pages()[0].frame.width().to_pt() as f32
    }

    /// The fitter's block model must match what Typst lays out, or auto-shrink is guessing. One, two
    /// and three lines at authored leading values (0.5, 0.99, 1.2, 1.5): a per-line constant that folds
    /// leading in is right at n=1 and wrong by one leading per line after that (#96).
    #[test]
    fn block_height_matches_typst_layout() {
        for spacing in [0.5, 0.99, 1.2, 1.5] {
            for lines in 1..=3usize {
                let rendered = typst_block_height_pt(lines, 20.0, Some(spacing));
                let predicted = super::helpers::block_height_with_spacing_for_test(
                    400,
                    20.0,
                    lines,
                    Some(spacing),
                );
                let drift = (rendered - predicted).abs() / rendered;
                assert!(
                    drift < 0.01,
                    "{lines} line(s) at pitch {spacing}: predicted {predicted:.2}pt, Typst laid out {rendered:.2}pt ({:.1}% off)",
                    drift * 100.0
                );
            }
        }
    }

    /// The per-character advance sum must match a real shaped line. Not a claim of shaping parity —
    /// the string has no kerning pairs or ligatures — but a units-per-em or scaling mistake would
    /// otherwise pass every unit test that compares text_width against itself (#96). Two sizes,
    /// because opsz differs between them.
    #[test]
    fn text_width_matches_typst_layout() {
        let text = "HIHIHI 123";
        for size in [10.0f32, 24.0] {
            let rendered = typst_line_width_pt(text, size);
            let predicted = super::helpers::text_width_for_test(400, size, text);
            let drift = (rendered - predicted).abs() / rendered;
            assert!(
                drift < 0.01,
                "{size}pt: predicted {predicted:.2}pt, Typst laid out {rendered:.2}pt ({:.1}% off)",
                drift * 100.0
            );
        }
    }

    fn weight_probe_ink(weight: u32) -> u64 {
        // Typst 0.15 strips the "Variable" suffix from stored family names (typst-library
        // `typographic_family`), so the bundled InterVariable.ttf registers as "Inter"; requesting
        // "Inter Variable" is now an unknown family and warns. Probe the name that actually resolves
        // so the zero-warnings guard still fails loudly if the bundled Inter is missing.
        let source = format!(
            "#set page(width: 60mm, height: 20mm, margin: 0mm)\n#set text(font: \"Inter\", size: 14pt, weight: {weight})\nWeight Probe 123"
        );
        let engine = super::TypstEngine::builder()
            .main_file(source)
            .search_fonts_with(super::typst_font_options())
            .build();
        let warned = engine.compile::<super::PagedDocument>();
        assert!(
            warned.warnings.is_empty(),
            "font must resolve without warnings (else the embedded fallback could fake a real bold): {:?}",
            warned.warnings
        );
        let doc = warned.output.expect("compile weight probe");
        let pixmap = typst_render::render(&doc.pages()[0], &super::render_options(200.0 / 72.0));
        let png = pixmap.encode_png().expect("png");
        let img = image::load_from_memory(&png).expect("decode").to_luma8();
        img.pixels().map(|p| (255 - p.0[0]) as u64).sum()
    }

    /// #101 acceptance: Typst 0.15 drives the wght axis of the bundled variable Inter.
    /// On 0.14 the axis is ignored (ratio ~1.0) and this fails; on 0.15 bold has ≥10% more ink.
    #[test]
    fn variable_font_weight_is_honored() {
        let regular = weight_probe_ink(400);
        let bold = weight_probe_ink(700);
        assert!(
            bold as f64 >= regular as f64 * 1.10,
            "weight 700 must add ≥10% ink over 400 (got {regular} vs {bold}, ratio {:.3})",
            bold as f64 / regular as f64
        );
    }

    /// Task 2.3: Render-measured tests on repeated lines ("Hxy\nHxy") proving band distances of
    /// 0.99, 0.5, 1.5 and the 1.2 default, plus absent renders identically to explicit 1.2.
    #[test]
    fn render_measured_line_pitch_band_distances_and_default_equivalence() {
        let render_hxy = |text: &str, spacing: Option<f32>| -> Vec<u8> {
            let template = TemplateContent {
                name: "Hxy".to_string(),
                description: String::new(),
                unit: "mm".to_string(),
                dpi: 180,
                format: TemplateFormat::Single {
                    width: Dimension::Fixed(100.0).into(),
                    height: Dimension::Fixed(60.0).into(),
                    media_width: None,
                },
                params: IndexMap::new(),
                layout: Layout::Items(vec![LayoutItem::Text {
                    value: text.to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(100.0), SizeValue::fixed(60.0)]),
                    ),
                    font_size: FontSize::Fixed(20.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: spacing,
                    alignment: crate::models::Alignment {
                        horizontal: HorizontalAlign::Left,
                        vertical: VerticalAlign::Top,
                    },
                    overflow: Overflow::Ellipsis,
                    when: None,
                }]),
                version: None,
            };
            render_single_label(
                &template,
                &HashMap::new(),
                None,
                &no_settings(),
                &no_datetime(),
            )
            .expect("render hxy")
        };

        // 1. Absent line_spacing renders byte-identically to explicit 1.2
        let png_absent = render_hxy("Hxy\nHxy", None);
        let png_default = render_hxy("Hxy\nHxy", Some(1.2));
        assert_eq!(
            png_absent, png_default,
            "absent line_spacing must render byte-identically to explicit 1.2"
        );

        // 2. Measure vertical distance between the two lines across spacing values
        // At 180 dpi, 1 pt = 2.5 px. For 20pt font: expected pitch px = spacing * 20.0 * 2.5 = spacing * 50.0 px.
        for spacing in [0.5, 0.99, 1.2, 1.5] {
            let png_1line = render_hxy("Hxy", Some(spacing));
            let png_2line = render_hxy("Hxy\nHxy", Some(spacing));

            let (_, bottom1, _) = ink_rows(&png_1line);
            let (_, bottom2, _) = ink_rows(&png_2line);

            let expected_pitch_px = spacing * 50.0;
            let measured_pitch = (bottom2 - bottom1) as f32;
            let drift = (measured_pitch - expected_pitch_px).abs();
            assert!(
                drift <= 1.0,
                "spacing {spacing}: measured pitch {measured_pitch}px, expected {expected_pitch_px}px (drift {drift}px)"
            );
        }
    }

    /// Task 2.4: Render-measured tests proving tighter pitch settles a height-bound range item
    /// at a larger size than a looser one, and that a single-line item renders byte-identically.
    #[test]
    fn tighter_pitch_allows_larger_font_size_and_single_line_is_invariant() {
        // 1. Height-bound 3-line text item with range font_size
        let make_range_template = |spacing: Option<f32>| TemplateContent {
            name: "RangePitch".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(100.0).into(),
                height: Dimension::Fixed(16.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "First line of text\nSecond line of text\nThird line of text".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(100.0), SizeValue::fixed(16.0)]),
                ),
                font_size: FontSize::Range {
                    min: 8.0,
                    max: 24.0,
                },
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: spacing,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };

        let env = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
        };
        let tpl_tight = make_range_template(Some(0.8));
        let compiled_tight = super::compile_label_source(&tpl_tight, &HashMap::new(), None, &env)
            .expect("compile tight");
        let size_tight = fitted_pt(&compiled_tight.source);

        let tpl_loose = make_range_template(Some(1.5));
        let compiled_loose = super::compile_label_source(&tpl_loose, &HashMap::new(), None, &env)
            .expect("compile loose");
        let size_loose = fitted_pt(&compiled_loose.source);

        assert!(
            size_tight > size_loose,
            "tighter pitch (0.8) must fit at larger font size than looser pitch (1.5): got {size_tight}pt vs {size_loose}pt"
        );

        // 2. Single-line text item renders byte-identically with and without line_spacing
        let make_single_line = |spacing: Option<f32>| -> Vec<u8> {
            let template = TemplateContent {
                name: "SingleLine".to_string(),
                description: String::new(),
                unit: "mm".to_string(),
                dpi: 180,
                format: TemplateFormat::Single {
                    width: Dimension::Fixed(60.0).into(),
                    height: Dimension::Fixed(20.0).into(),
                    media_width: None,
                },
                params: IndexMap::new(),
                layout: Layout::Items(vec![LayoutItem::Text {
                    value: "Single Line Invariant".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(60.0), SizeValue::fixed(20.0)]),
                    ),
                    font_size: FontSize::Fixed(12.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: spacing,
                    alignment: crate::models::Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                }]),
                version: None,
            };
            render_single_label(
                &template,
                &HashMap::new(),
                None,
                &no_settings(),
                &no_datetime(),
            )
            .expect("render single line")
        };

        let png_absent = make_single_line(None);
        let png_explicit_default = make_single_line(Some(1.2));
        let png_tight = make_single_line(Some(0.5));
        let png_loose = make_single_line(Some(1.5));

        assert_eq!(
            png_absent, png_explicit_default,
            "single line absent line_spacing must match explicit 1.2"
        );
        assert_eq!(
            png_absent, png_tight,
            "single line absent line_spacing must match tight 0.5"
        );
        assert_eq!(
            png_absent, png_loose,
            "single line absent line_spacing must match loose 1.5"
        );
    }

    /// Dynamic-width mode is a property of the template format, not of whether any text needed
    /// measuring: a label can be sized by a line or a non-text container alone. An auto-width
    /// container at x=5 on a 25mm dynamic label must be 20mm wide, not the full 25mm, or it overruns
    /// the page by exactly its own offset.
    #[test]
    fn dynamic_width_mode_is_independent_of_measured_text() {
        // `at_x` differs per mode: the render-time bounds check (Task 5) now rejects a container
        // that resolves past the frame edge, and the fixed-mode auto-width fallback fills the whole
        // frame regardless of offset, so it needs `at_x = 0.0` to stay in bounds. Compile-time
        // `validate_bounds` already forbids the x=5 fixed-mode combination on any real template
        // (5 + 25 > 25), so this keeps the fixture reachable through the real pipeline.
        fn container_width(at_x: f32) -> String {
            render_test_items(
                &[LayoutItem::Container {
                    placement: Placement::sized(
                        Position([at_x, 0.0]),
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
                    items: vec![LayoutItem::Line {
                        at: Position([0.0, 6.0]),
                        to: Position([20.0, 6.0]),
                        stroke: Some(Stroke {
                            thickness: 0.2,
                            color: DynamicValue::Literal(Color::black()),
                        }),
                        when: None,
                    }],
                }],
                (25.0, 12.0),
            )
            .expect("render")
        }

        let dynamic = container_width(5.0);
        assert!(
            dynamic.contains("width: 20mm"),
            "a dynamic label with no measured text must still size the container to the remaining \
             width, got: {dynamic}"
        );

        let fixed = container_width(0.0);
        assert!(
            fixed.contains("width: 25mm"),
            "on a fixed label an auto container fills the frame, got: {fixed}"
        );
    }

    /// An edge-relative line endpoint contributes its inset, exactly as a right-anchored box does:
    /// it cannot define the width it is measured against, but the label still has to be at least as
    /// wide as the inset or the endpoint has nowhere to sit. Here the wider endpoint is 5mm in.
    #[test]
    fn an_edge_relative_line_endpoint_contributes_its_inset() {
        let item = LayoutItem::Line {
            at: Position([-5.0, 6.0]),
            to: Position([-3.0, 6.0]),
            stroke: Some(Stroke {
                thickness: 0.2,
                color: DynamicValue::Literal(Color::black()),
            }),
            when: None,
        };
        let (extent, text_count) = measured_extent_of(item, 80.0);
        assert_eq!(extent, 5.0);
        assert_eq!(text_count, 0);
    }

    /// A right-anchored item cannot define the width it is anchored to, but the label still has to
    /// be at least as wide as the inset or the item has nowhere to sit. That inset is its
    /// contribution.
    #[test]
    fn an_edge_relative_at_x_contributes_its_inset() {
        let item = LayoutItem::Text {
            value: "x".to_string(),
            placement: Placement::sized(
                Position([-20.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(8.0)]),
            ),
            font_size: FontSize::Fixed(6.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let (extent, text_count) = measured_extent_of(item, 80.0);
        assert_eq!(extent, 20.0);
        assert_eq!(text_count, 1);
    }

    /// The divider spans to the frame's right edge, not back to x=0.
    #[test]
    fn an_edge_relative_line_renders_to_the_right_edge() {
        let source = render_test_items(
            &[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([-0.0, 6.0]),
                stroke: Some(Stroke {
                    thickness: 0.2,
                    color: DynamicValue::Literal(Color::black()),
                }),
                when: None,
            }],
            (40.0, 12.0),
        )
        .expect("render");
        assert!(
            source.contains("end: (40mm, 0mm)"),
            "expected a 40mm-long line, got: {source}"
        );
    }

    /// Builds a dynamic-width label whose text measures to roughly 10mm, plus one line.
    fn dynamic_label_with_line(at: Position, to: Position) -> TemplateContent {
        TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(5.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    value: "hi".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::content(), SizeValue::fixed(6.0)]),
                    ),
                    font_size: FontSize::Fixed(6.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: crate::models::Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                },
                LayoutItem::Line {
                    at,
                    to,
                    stroke: Some(Stroke {
                        thickness: 0.2,
                        color: DynamicValue::Literal(Color::black()),
                    }),
                    when: None,
                },
            ]),
            version: None,
        }
    }

    /// A right-anchored line beside content-sized text: the label must grow to hold the line's own
    /// inset, the same way it grows to hold a right-anchored box. Before the line rule matched the
    /// box rule this rendered a `a coordinate resolves outside the frame` error, because the label
    /// resolved to the ~10mm of text and the 20mm inset then landed left of x=0.
    #[test]
    fn a_right_anchored_line_widens_the_label_to_its_inset() {
        let template = dynamic_label_with_line(Position([-20.0, 8.0]), Position([-0.0, 8.0]));
        assert_eq!(template.validate(), Ok(()));
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let png = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect("a right-anchored line must render beside auto-width text");
        let width_px = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        // 20mm at 180dpi is ~142px; the ~10mm of text alone would be about half that.
        assert!(
            width_px >= 138,
            "the label must be at least as wide as the line's 20mm inset, got {width_px}px"
        );
    }

    /// The render-time endpoint bound (`check_line`) is the mirror of the load-time one, per SPEC §7's
    /// compile-time/render-time duplication. Load-time validation now rejects every template that
    /// could reach it (a plain endpoint past `width.max` is rejected outright, and an edge-relative
    /// one sizes the label to its own inset), so it is exercised here at the context level.
    #[test]
    fn a_line_endpoint_outside_the_frame_errors_at_render() {
        let err = render_test_items(
            &[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([30.0, 6.0]),
                stroke: Some(Stroke {
                    thickness: 0.2,
                    color: DynamicValue::Literal(Color::black()),
                }),
                when: None,
            }],
            (10.0, 12.0),
        )
        .expect_err("a 30mm endpoint on a 10mm frame must not render");
        // Not `coord_out_of_frame`: this is a Line, so it trips the endpoint check. The prose
        // assertion this replaces could not tell the two apart, which is the point of #151.
        assert_eq!(
            err.reason(),
            Some("line_endpoint_out_of_frame"),
            "unexpected error: {}",
            err.message_text()
        );
    }

    /// Compile time could not compare these endpoints: one is edge-relative and one is not, and the
    /// final width was unknown. The content measures well under `min`, so the clamp pins the label to
    /// exactly 20mm, where the two endpoints coincide and the line is degenerate after all.
    #[test]
    fn a_line_that_becomes_degenerate_at_the_final_width_errors_at_render() {
        let mut template = dynamic_label_with_line(Position([20.0, 8.0]), Position([-0.0, 8.0]));
        template.format = TemplateFormat::Single {
            width: DynamicDimension::Dynamic {
                min: Some(DynamicValue::Literal(20.0)),
                max: Some(DynamicValue::Literal(100.0)),
            },
            height: Dimension::Fixed(12.0).into(),
            media_width: None,
        };
        assert_eq!(template.validate(), Ok(()), "not comparable at load time");
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("a zero-length line must not render");
        assert_eq!(
            err.reason(),
            Some("line_degenerate"),
            "unexpected error: {}",
            err.message_text()
        );
    }

    /// The #146/#147 acceptance template renders, and its width tracks its content. Visual
    /// correctness was verified by looking at the PNG; this guards the mechanics.
    #[test]
    fn the_lines_divider_template_is_content_sized() {
        let (registry, _dir) = crate::templates::load_all_for_tests();
        let template = registry
            .get("brother_24mm_lines_divider")
            .expect("fixture template is loaded");
        let render = |l1: &str, l2: &str| {
            let mut data: HashMap<String, super::JsonValue> = HashMap::new();
            data.insert("line1".to_string(), json!(l1));
            data.insert("line2".to_string(), json!(l2));
            let png = render_single_label(template, &data, &no_settings(), &no_datetime())
                .expect("render");
            u32::from_be_bytes([png[16], png[17], png[18], png[19]])
        };
        let short = render("Bin 7", "Shed");
        let long = render("Storage Bin A-42", "Workshop / North Wall");
        assert!(
            long > short,
            "an auto-length label must track its content: {long}px vs {short}px"
        );
    }

    /// Task 3.6: Update expectations for fixtures affected by centered ink reservation.
    #[test]
    fn fixture_renders_reflect_new_centered_ink_reservation_numbers() {
        let (registry, _dir) = crate::templates::load_all_for_tests();

        // 1. brother_24mm_printed_on: line 1 in 8.0mm box (max 24pt) fits at 18.5pt (down from 24.0pt)
        let printed_on = registry
            .get("brother_24mm_printed_on")
            .expect("printed_on template");
        let mut data1 = HashMap::new();
        data1.insert("message".to_string(), json!("Warehouse Section B"));
        data1.insert("printed_on".to_string(), json!("2026-08-19"));
        let mut dt_formats = BTreeMap::new();
        dt_formats.insert("short_date".to_string(), "%Y-%m-%d".to_string());
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now: chrono::Local::now(),
        };
        let env1 = super::RenderEnv {
            settings: &no_settings(),
            datetime: &dt,
        };
        let compiled1 =
            super::compile_label_source(printed_on, &data1, &env1).expect("compile printed_on");
        let size1 = fitted_pt(&compiled1.source);
        assert_eq!(
            size1, 18.5,
            "brother_24mm_printed_on line 1 must fit at 18.5pt (down from 24pt)"
        );

        // 2. brother_24mm_lines_divider: line 1 in 7.5mm box (max 20pt) fits at 17.5pt (down from 20.0pt)
        let lines_divider = registry
            .get("brother_24mm_lines_divider")
            .expect("lines_divider template");
        let mut data2 = HashMap::new();
        data2.insert("line1".to_string(), json!("Storage Bin A-42"));
        data2.insert("line2".to_string(), json!("Workshop / North Wall"));
        let env2 = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
        };
        let compiled2 = super::compile_label_source(lines_divider, &data2, &env2)
            .expect("compile lines_divider");
        let size2 = fitted_pt(&compiled2.source);
        assert_eq!(
            size2, 17.5,
            "brother_24mm_lines_divider line 1 must fit at 17.5pt (down from 20pt)"
        );

        // 3. brother_24mm_multiline: 2-line wrapped text in 16.1mm box (max 32pt) fits at 18.5pt (down from 21.5pt with old leading)
        let multiline = registry
            .get("brother_24mm_multiline")
            .expect("multiline template");
        let mut data3 = HashMap::new();
        data3.insert(
            "message".to_string(),
            json!("Long label that should wrap onto two lines on the tape"),
        );
        let env3 = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
        };
        let compiled3 =
            super::compile_label_source(multiline, &data3, &env3).expect("compile multiline");
        let size3 = fitted_pt(&compiled3.source);
        assert_eq!(
            size3, 18.5,
            "brother_24mm_multiline 2-line text must fit at 18.5pt (down from 21.5pt)"
        );

        // 4. avery5163_asset_tag:
        let avery = registry
            .get("avery5163_asset_tag")
            .expect("avery5163_asset_tag template");
        let mut data4 = HashMap::new();
        data4.insert("id".to_string(), json!("A1"));
        data4.insert("url".to_string(), json!("https://example.com"));
        data4.insert("name".to_string(), json!("Floor Grinder"));
        data4.insert(
            "tags".to_string(),
            json!("Angle grinder with floor grinding attachment and heavy dust shroud"),
        );
        data4.insert(
            "description".to_string(),
            json!("Angle grinder with floor grinding attachment and heavy dust shroud"),
        );
        let mut opt4 = BTreeMap::new();
        opt4.insert("orientation".to_string(), "horizontal".to_string());
        let env4 = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
        };
        let compiled4 =
            super::compile_label_source(avery, &data4, &env4).expect("compile avery5163");
        let src4 = &compiled4.source;

        // {id} in horizontal orientation (0.35in box, max 22pt) fits at 20.5pt (down from 22.0pt)
        let id_idx = src4.find("\"A1\"").expect("id text in source");
        let size4_id = fitted_pt_at(src4, id_idx);
        assert_eq!(
            size4_id, 20.5,
            "avery5163_asset_tag {{id}} must fit at 20.5pt (down from 22pt)"
        );

        // {name} in horizontal orientation (0.4in box, max 24pt) fits at 23.5pt (down from 24.0pt)
        let name_idx = src4.find("Floor").expect("name text in source");
        let size4_name = fitted_pt_at(src4, name_idx);
        assert_eq!(
            size4_name, 23.5,
            "avery5163_asset_tag {{name}} must fit at 23.5pt (down from 24pt)"
        );

        // {tags} / {description} in 0.65in box at fixed 12pt fits all 3 lines without ellipsizing under 1.2 pitch
        let desc_chunk = &src4[name_idx..];
        let linebreaks = desc_chunk.matches("#linebreak()").count();
        assert_eq!(
            linebreaks, 4,
            "avery5163_asset_tag tags and description must each wrap to 3 lines (2 linebreaks each, 4 total)"
        );
    }

    /// A blank optional field is ordinary in CSV-driven printing. The empty value measures to
    /// nothing, the label clamps to the item's own `at.x`, and the `to`-spanning box collapses to
    /// zero width — a legitimate render-time outcome of empty data, not an authoring error, so it
    /// must render rather than 422. The same shape with a value still renders.
    #[test]
    fn an_empty_value_collapses_a_to_spanned_box_instead_of_erroring() {
        let template = TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(60.0)),
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: IndexMap::from([(
                "v".to_string(),
                crate::models::ParamSpec {
                    param_type: crate::models::ParamType::String { multiline: false },
                    default: None,
                    min: None,
                    max: None,
                    description: None,
                },
            )]),
            layout: Layout::Items(vec![LayoutItem::Text {
                value: "{v}".to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::To(Position([-0.0, 12.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        assert_eq!(template.validate(), Ok(()));
        for value in ["hello", ""] {
            let mut data: HashMap<String, super::JsonValue> = HashMap::new();
            data.insert("v".to_string(), json!(value));
            render_single_label(&template, &data, &no_settings(), &no_datetime()).unwrap_or_else(
                |err| panic!("value {value:?} must render, got: {}", err.message_text()),
            );
        }
    }

    /// A `to`-sized qr contributes nothing to the measured extent (it has no intrinsic content
    /// width, ADR-0050 decision 11), so the label falls back to `width.min`. That only leaves room
    /// for the item when its own `at.x` fits inside the fallback: anchored at x=30 on a 10mm label
    /// there is no box left to draw, and it errors rather than silently disappearing. Pins the §6
    /// wording.
    #[test]
    fn a_to_sized_qr_anchored_past_the_fallback_width_errors() {
        let qr_at = |x: f32| TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Ref("target_width".to_string())),
                },
                height: Dimension::Fixed(12.0).into(),
                media_width: None,
            },
            params: IndexMap::from([(
                "target_width".to_string(),
                ParamSpec {
                    param_type: ParamType::Length,
                    description: None,
                    default: Some(crate::models::ParamValue::Float(100.0)),
                    min: Some(10.0),
                    max: Some(100.0),
                },
            )]),
            layout: Layout::Items(vec![LayoutItem::Qr {
                value: "payload".to_string(),
                placement: Placement {
                    at: Some(Position([x, 0.0])),
                    extent: crate::models::Extent::To(Position([-0.0, 12.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: Some(crate::models::QrParams {
                    error_correction: None,
                    module_size: Some(0.5),
                    quiet_zone: None,
                }),
                when: None,
            }]),
            version: None,
        };
        let mut data: HashMap<String, super::JsonValue> = HashMap::new();

        let flush_left = qr_at(0.0);
        assert_eq!(flush_left.validate(), Ok(()));
        render_single_label(&flush_left, &data, &no_settings(), &no_datetime())
            .expect("from x=0 the fallback width is the whole box");

        let template = qr_at(30.0);
        assert_eq!(template.validate(), Ok(()), "valid against the 100mm max");
        data.insert("target_width".to_string(), json!(20.0));
        let err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("a 30mm anchor on a 20mm label leaves no box");
        assert_eq!(
            err.reason(),
            Some("edge_rect_inverted"),
            "unexpected error: {}",
            err.message_text()
        );
    }

    /// The box spans from x=0 to the frame's right edge, so a centered line centers on the label.
    #[test]
    fn a_to_box_renders_at_the_full_frame_width() {
        let source = render_test_items(
            &[LayoutItem::Text {
                value: "x".to_string(),
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: crate::models::Extent::To(Position([-0.0, 12.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }],
            (40.0, 12.0),
        )
        .expect("render");
        assert!(
            source.contains("width: 40mm"),
            "expected a full-width box, got: {source}"
        );
    }

    fn parse_and_validate(body: &str) -> Result<TemplateContent, AppError> {
        let content = crate::parse::parse_template(body).map_err(|err| {
            AppError::template_invalid(Reason::TemplateParseFailed, err.to_string())
        })?;
        content
            .validate()
            .map_err(|err| AppError::template_invalid(Reason::TemplateValidationFailed, err))?;
        Ok(content)
    }

    fn resolver() -> crate::datetime_fmt::DateTimeResolver<'static> {
        no_datetime()
    }

    #[test]
    fn render_continuous_tape_with_dynamic_target_width() {
        let yaml = r#"
name: Dynamic Width
unit: mm
dpi: 200
params:
  - name: message
    type: string
  - name: target_width
    type: length
    default: 60
format:
  type: single
  height: 18
  width:
    min: 25
    max: "{target_width}"
layout:
  - type: text
    value: "{message}"
    at: [0, 0]
    size: [content, 18]
    font_size: { min: 8, max: 24 }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert(
            "message".to_string(),
            json!("Hello World this is a very long text that will overflow the target width"),
        );
        data.insert("target_width".to_string(), json!(90.0));

        let png = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        let expected_px = (90.0_f32 / 25.4 * template.dpi as f32).round() as u32;
        let min_px = (25.0_f32 / 25.4 * template.dpi as f32).round() as u32;
        assert!(
            img.width() <= expected_px && img.width() > min_px,
            "tape width {}px must be in (min: {min_px}, max: {expected_px})",
            img.width()
        );
    }

    #[test]
    fn render_with_dynamic_font_weight() {
        let yaml = r#"
name: Dynamic Weight
unit: mm
dpi: 200
params:
  - name: message
    type: string
  - name: weight
    type: integer
    default: 400
format:
  type: single
  height: 18
  width: 60
layout:
  - type: text
    value: "{message}"
    at: [0, 0]
    size: [60, 18]
    font_size: 10
    font_weight: "{weight}"
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("message".to_string(), json!("Bold Text"));
        data.insert("weight".to_string(), json!(700));

        let png = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn inactive_when_branch_does_not_require_missing_fields_during_measure_or_render() {
        let yaml = r#"
name: When Lazy Test
unit: mm
dpi: 200
params:
  - name: orientation
    type: enum
    values: [h, v]
    default: h
  - name: h_text
    type: string
  - name: v_text
    type: string
format:
  type: single
  height: 18
  width:
    min: 20
    max: 100
layout:
  - type: text
    value: "{h_text}"
    at: [0, 0]
    size: [content, 18]
    font_size: { min: 8, max: 24 }
    when: { orientation: h }
  - type: text
    value: "{v_text}"
    at: [0, 0]
    size: [content, 18]
    font_size: { min: 8, max: 24 }
    when: { orientation: v }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("orientation".to_string(), json!("h"));
        data.insert("h_text".to_string(), json!("Horizontal only"));
        // v_text is omitted; must succeed without MissingField

        let res = render_single_label(&template, &data, &BTreeMap::new(), &resolver());
        assert!(
            res.is_ok(),
            "should succeed because v_text is in inactive branch"
        );
    }

    #[test]
    fn active_branch_missing_field_returns_422_missing_field() {
        let yaml = r#"
name: Active Missing Test
unit: mm
dpi: 200
params:
  - name: message
    type: string
format:
  type: single
  height: 18
  width: 60
layout:
  - type: text
    value: "{message}"
    at: [0, 0]
    size: [60, 18]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let data = HashMap::new(); // message omitted

        let res = render_single_label(&template, &data, &BTreeMap::new(), &resolver());
        assert!(matches!(res, Err(err) if err.code() == "MissingField"));
    }

    #[test]
    fn dimension_exceeding_max_label_dimension_returns_422() {
        let yaml = r#"
name: Dim Limit Test
unit: mm
dpi: 200
params:
  - name: target_width
    type: length
    default: 60
format:
  type: single
  height: 18
  width:
    min: 25
    max: "{target_width}"
layout:
  - type: text
    value: "Test"
    at: [0, 0]
    size: [content, 18]
    font_size: { min: 8, max: 24 }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("target_width".to_string(), json!(1500.0)); // exceeds default 1000mm

        let res = render_single_label(&template, &data, &BTreeMap::new(), &resolver());
        assert!(matches!(res, Err(err) if err.code() == "UnsupportedLayoutItem"));
    }

    #[test]
    fn dynamic_container_padding_overflow_at_runtime_returns_container_padding_no_room() {
        let yaml = r#"
name: Dynamic Container Padding Overflow
unit: mm
dpi: 200
params:
  - name: target_width
    type: length
    default: 60
format:
  type: single
  height: 18
  width:
    min: 10
    max: "{target_width}"
layout:
  - type: container
    at: [0, 0]
    size: [fill, 18]
    padding: [0, 10, 0, 10]
    items:
      - type: text
        value: "Active text"
        at: [0, 0]
        size: [content, 18]
        font_size: { min: 8, max: 24 }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        // target_width shrinks to 15mm, but container padding is left 10 + right 10 = 20mm.
        data.insert("target_width".to_string(), json!(15.0));

        let res = render_single_label(&template, &data, &BTreeMap::new(), &resolver());
        assert!(
            res.is_err(),
            "expected render to fail due to padding overflow"
        );
        let err = res.unwrap_err();
        assert_eq!(err.code(), "UnsupportedLayoutItem");
        assert_eq!(err.reason(), Some("text_does_not_fit"));
    }

    #[test]
    fn dynamic_container_padding_overflow_with_inactive_when_children_renders_ok() {
        let yaml = r#"
name: Dynamic Container Inactive Padding
unit: mm
dpi: 200
params:
  - name: target_width
    type: length
    default: 60
  - name: show_extra
    type: boolean
    default: false
format:
  type: single
  height: 18
  width:
    min: 10
    max: "{target_width}"
layout:
  - type: container
    at: [0, 0]
    size: [fill, 18]
    padding: [0, 10, 0, 10]
    items:
      - type: text
        value: "Conditional text"
        at: [0, 0]
        size: [content, 18]
        font_size: { min: 8, max: 24 }
        when: { show_extra: "true" }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("target_width".to_string(), json!(15.0));
        data.insert("show_extra".to_string(), json!(false));

        let res = render_single_label(&template, &data, &BTreeMap::new(), &resolver());
        assert!(
            res.is_ok(),
            "inactive child item should not trigger container_padding_no_room"
        );
    }

    #[test]
    fn datetime_param_render_and_override() {
        use chrono::TimeZone;

        let yaml = r#"
name: Test DateTime Param
unit: mm
dpi: 200
params:
  - name: printed_on
    type: datetime
    default: "{sys.now}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{printed_on} / {printed_on:short_date} / {sys.now}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let now = chrono::Local
            .with_ymd_and_hms(2026, 6, 25, 14, 30, 0)
            .single()
            .unwrap();
        let formats = BTreeMap::from([("short_date".to_string(), "%m/%d/%Y".to_string())]);
        let resolver = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now,
        };

        // 1. Without override: every token resolves against the request instant.
        assert_eq!(
            interpolated(&template, &HashMap::new(), &resolver).unwrap(),
            "2026-06-25 / 06/25/2026 / 2026-06-25"
        );

        // 2. With an override: the parameter's tokens move, `{sys.now}` does not.
        let mut data = HashMap::new();
        data.insert("printed_on".to_string(), json!("2026-08-19"));
        assert_eq!(
            interpolated(&template, &data, &resolver).unwrap(),
            "2026-08-19 / 08/19/2026 / 2026-06-25"
        );

        // 3. The same template still compiles all the way to a PNG.
        assert!(
            !render_single_label(&template, &data, &BTreeMap::new(), &resolver)
                .unwrap()
                .is_empty()
        );

        // 4. A blank string and an explicit null both mean "use the request instant".
        for omitted in [json!(""), json!("   "), json!(null)] {
            let mut blank = HashMap::new();
            blank.insert("printed_on".to_string(), omitted.clone());
            assert_eq!(
                interpolated(&template, &blank, &resolver).unwrap(),
                "2026-06-25 / 06/25/2026 / 2026-06-25",
                "{omitted} should resolve to the request instant"
            );
        }

        // 5. An unparseable string and a non-string value are both refused.
        for bad in [
            json!("not-a-date"),
            json!("yesterday"),
            json!(20260819),
            json!(true),
        ] {
            let mut bad_data = HashMap::new();
            bad_data.insert("printed_on".to_string(), bad.clone());
            let err = interpolated(&template, &bad_data, &resolver).unwrap_err();
            assert_eq!(
                err.reason(),
                Some("datetime_param_invalid"),
                "{bad} should be refused"
            );
            assert!(err.message_text().contains("printed_on"));
        }
    }

    /// Resolve a label's parameters and interpolate the template's first text item through the
    /// real chain: `resolve_parameters` builds the instants, `interpolate` reads them. Everything
    /// below `interpolate` is Typst, which a byte-length assertion cannot inspect.
    fn interpolated(
        template: &TemplateContent,
        data: &HashMap<String, serde_json::Value>,
        resolver: &crate::datetime_fmt::DateTimeResolver,
    ) -> Result<String, AppError> {
        let Layout::Items(items) = &template.layout;
        let value = items
            .iter()
            .find_map(|i| match i {
                LayoutItem::Text { value, .. } => Some(value.clone()),
                _ => None,
            })
            .expect("template needs a text item");
        let empty_vars = BTreeMap::new();
        let resolved =
            super::resolve_parameters(template, data, Some(&empty_vars), Some(resolver))?;
        super::helpers::interpolate(
            &value,
            &resolved.data,
            &empty_vars,
            resolver,
            Some(&resolved.instants),
        )
    }

    #[test]
    fn datetime_param_unknown_format_errors_at_render() {
        let yaml = r#"
name: Test DateTime Unknown Format
unit: mm
dpi: 200
params:
  - name: printed_on
    type: datetime
    default: "{sys.now}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{printed_on:no_such_format}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.code(), "MissingField");
        assert!(err.message_text().contains("printed_on:no_such_format"));
    }

    #[test]
    fn datetime_param_dynamic_width_auto_length_renders() {
        use chrono::TimeZone;

        let yaml = r#"
name: Test DateTime Dynamic Width
unit: mm
dpi: 200
params:
  - name: printed_on
    type: datetime
    default: "{sys.now}"
format:
  type: single
  height: 20
  width:
    min: 20
    max: 100
layout:
  - type: text
    value: "Date: {printed_on:short_date}"
    at: [0, 0]
    size: [content, 20]
    font_size: { min: 8, max: 14 }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let now = chrono::Local
            .with_ymd_and_hms(2026, 6, 25, 14, 30, 0)
            .single()
            .unwrap();
        let formats = BTreeMap::from([("short_date".to_string(), "%m/%d/%Y".to_string())]);
        let resolver = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now,
        };

        let doc =
            render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver).unwrap();
        assert!(!doc.is_empty());
    }

    #[test]
    fn datetime_param_included_in_fields_and_placeholders() {
        let yaml = r#"
name: Test DateTime Fields
unit: mm
dpi: 200
params:
  - name: title
    type: string
  - name: printed_on
    type: datetime
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{title} {printed_on} {printed_on:short_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut fields: Vec<String> = test_inputs_all(&template)
            .into_iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        fields.sort();
        assert_eq!(fields, vec!["printed_on".to_string(), "title".to_string()]);

        let ph = test_placeholder_data(&template, chrono::Local::now());
        assert!(ph.contains_key("title"));
        assert!(ph.contains_key("printed_on"));
        assert!(!ph.contains_key("printed_on:short_date"));
    }

    fn dt_param_template(value: &str) -> TemplateContent {
        let yaml = format!(
            r#"
name: Test DateTime
unit: mm
dpi: 200
params:
  - name: printed_on
    type: datetime
    default: "{{sys.now}}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{value}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#
        );
        parse_and_validate(&yaml).unwrap()
    }

    fn dt_resolver(
        formats: &BTreeMap<String, String>,
        now: chrono::DateTime<chrono::Local>,
    ) -> crate::datetime_fmt::DateTimeResolver<'_> {
        crate::datetime_fmt::DateTimeResolver { formats, now }
    }

    fn fixed_instant() -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(2026, 6, 25, 14, 30, 0)
            .single()
            .unwrap()
    }

    fn short_date_formats() -> BTreeMap<String, String> {
        BTreeMap::from([("short_date".to_string(), "%m/%d/%Y".to_string())])
    }

    /// A request key spelled like a namespace token is data, and data never reaches a declared
    /// `datetime` parameter's namespace.
    #[test]
    fn datetime_param_namespace_cannot_be_shadowed_by_request_data() {
        let template = dt_param_template("{printed_on} {printed_on:short_date}");
        let formats = short_date_formats();
        let resolver = dt_resolver(&formats, fixed_instant());

        let mut data = HashMap::new();
        data.insert(
            "printed_on:short_date".to_string(),
            json!("SHADOWED BY REQUEST"),
        );
        assert_eq!(
            interpolated(&template, &data, &resolver).unwrap(),
            "2026-06-25 06/25/2026"
        );
    }

    /// `resolve_parameters` must resolve against the instant it is handed and never read the clock
    /// itself: a second clock read is what makes a sheet that crosses midnight print two dates.
    /// The instant here is years in the past, so any hidden `Local::now()` shows up immediately.
    #[test]
    fn datetime_param_uses_the_passed_instant_not_the_clock() {
        use chrono::TimeZone;
        let template = dt_param_template("{printed_on}");
        let long_ago = chrono::Local
            .with_ymd_and_hms(2020, 1, 2, 3, 4, 5)
            .single()
            .unwrap();
        let empty_formats = BTreeMap::new();
        let empty_vars = BTreeMap::new();
        let resolver = dt_resolver(&empty_formats, long_ago);

        // Two labels of one batch, resolved separately, sharing the request's instant.
        let first = super::resolve_parameters(
            &template,
            &HashMap::new(),
            Some(&empty_vars),
            Some(&resolver),
        )
        .unwrap();
        let second = super::resolve_parameters(
            &template,
            &HashMap::new(),
            Some(&empty_vars),
            Some(&resolver),
        )
        .unwrap();

        let long_ago_midnight = chrono::Local
            .with_ymd_and_hms(2020, 1, 2, 0, 0, 0)
            .single()
            .unwrap();
        assert_eq!(first.instants["printed_on"], long_ago_midnight);
        assert_eq!(second.instants["printed_on"], long_ago_midnight);
        assert_eq!(first.data["printed_on"], json!("2020-01-02"));
        assert_eq!(second.data["printed_on"], json!("2020-01-02"));
    }

    /// A thumbnail substitutes placeholder text for request fields. A `datetime` parameter is not
    /// one, so it prints a real date rather than its own name.
    #[test]
    fn datetime_param_renders_a_real_date_in_a_thumbnail() {
        let template = dt_param_template("{printed_on:short_date}");
        let formats = short_date_formats();
        let resolver = dt_resolver(&formats, fixed_instant());

        let data = test_placeholder_data(&template, resolver.now);
        assert_eq!(
            interpolated(&template, &data, &resolver).unwrap(),
            "06/25/2026"
        );

        assert!(
            !render_thumbnail_png(&template, &data, &BTreeMap::new(), &resolver)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn advertised_fields_token_grammar_test() {
        // {datetime} is an advertised data field; {sys.now} and {sys.now:<fmt>} produce nothing
        // {vars} is an advertised data field; {vars.<key>} produces nothing
        // Declared parameter of type datetime (e.g. printed_on) is excluded from advertised data fields
        let yaml = r#"
name: Adv Test
unit: mm
dpi: 200
params:
  - name: datetime
    type: string
  - name: vars
    type: string
  - name: printed_on
    type: datetime
    default: "{sys.now}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{datetime} {sys.now} {sys.now:iso_date} {vars} {vars.site} {printed_on} {printed_on:short_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let fields: Vec<String> = test_inputs_all(&template)
            .into_iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert_eq!(fields, vec!["datetime".to_string(), "vars".to_string()]);
    }

    /// `when:` sees the parameter through the resolved data map, where the instant is written as
    /// the bare ISO date. That is what a predicate compares against.
    #[test]
    fn datetime_param_when_compares_the_bare_iso_date() {
        let yaml = r#"
name: Test DateTime When
unit: mm
dpi: 200
params:
  - name: printed_on
    type: datetime
format:
  type: single
  height: 20
  width: 50
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    when:
      printed_on: "2026-08-19"
    items: []
"#;
        let template = parse_and_validate(yaml).unwrap();
        let formats = short_date_formats();
        let resolver = dt_resolver(&formats, fixed_instant());
        let Layout::Items(items) = &template.layout;
        let images = std::cell::RefCell::new(super::ImageCollector::default());

        let active_for = |data: HashMap<String, serde_json::Value>| {
            let resolved =
                super::resolve_parameters(&template, &data, None, Some(&resolver)).unwrap();
            let empty_settings = BTreeMap::new();
            let env = super::RenderEnv {
                settings: &empty_settings,
                datetime: &resolver,
            };
            super::RenderContext::new("mm", 180, &resolved.data, &env, &images)
                .with_instants(&resolved.instants)
                .is_item_active(&items[0])
        };

        let mut matching = HashMap::new();
        matching.insert("printed_on".to_string(), json!("2026-08-19"));
        assert!(active_for(matching));

        // A different instant, and the request instant, both fail the predicate.
        let mut other = HashMap::new();
        other.insert("printed_on".to_string(), json!("2026-08-20"));
        assert!(!active_for(other));
        assert!(!active_for(HashMap::new()));

        // An RFC 3339 override on the same day still compares as that day's bare ISO date.
        let mut rfc = HashMap::new();
        rfc.insert("printed_on".to_string(), json!("2026-08-19T23:15:00Z"));
        assert_eq!(
            super::resolve_parameters(&template, &rfc, None, Some(&resolver))
                .unwrap()
                .data["printed_on"],
            json!("2026-08-19")
        );
    }

    #[test]
    fn avery5163_asset_tag_thumbnail_renders_horizontal_branch() {
        let registry = crate::templates::load_all_for_tests().0;
        let template = registry
            .get("avery5163_asset_tag")
            .expect("avery5163_asset_tag template");
        let dt_formats = BTreeMap::new();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now: chrono::Local::now(),
        };
        let data = test_placeholder_data(template, dt.now);
        assert!(!data.contains_key("orientation"));
        assert!(!data.contains_key("outline"));
        // outline declares no default and is only a gate key, so it must be absent and its
        // container inactive in the thumbnail (the fixture's thumbnail no longer draws the
        // outline). Horizontal branch must be active via its default.
        let settings = BTreeMap::new();
        let dt_resolved = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now: dt.now,
        };
        let resolved =
            super::resolve_parameters(template, &data, None, Some(&dt_resolved)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &dt_resolved,
        };
        let ctx =
            super::RenderContext::new(&template.unit, template.dpi, &resolved.data, &env, &images)
                .with_instants(&resolved.instants);
        let Layout::Items(items) = &template.layout;
        assert!(
            !ctx.is_item_active(&items[0]),
            "outline container must be inactive when outline declares no default"
        );
        assert!(
            ctx.is_item_active(&items[1]),
            "horizontal container must be active via default"
        );
        assert!(
            !ctx.is_item_active(&items[2]),
            "vertical container must be inactive"
        );
        let png = render_thumbnail_png(template, &data, &settings, &dt_resolved)
            .expect("render thumbnail");
        assert!(!png.is_empty());
    }

    #[test]
    fn rotated_container_measurement_applies_swapped_padding() {
        let yaml = r#"
name: Rotated Padding
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 40]
    rotate: 90
    padding: [5, 10, 5, 10]
    items:
      - type: text
        value: "Long text that must fit within inner width"
        at: [0, 0]
        size: [fill, fill]
        font_size: { min: 8, max: 24 }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let png =
            render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn text_with_shrinking_to_laid_out_against_resolved_box() {
        let yaml = r#"
name: Shrinking To Overflow
unit: mm
dpi: 200
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: "A long text that cannot fit within 10mm"
    at: [-20, 0]
    to: [90, 20]
    overflow: fail
    font_size: 14
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("text_does_not_fit"));
    }

    #[test]
    fn parameter_resolved_authored_size_zero_errors_with_size_invalid() {
        let yaml = r#"
name: Zero Size
unit: mm
dpi: 200
params:
  - name: w
    type: length
    default: 10
format: { type: single, width: 100, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: ["{w}", 20]
    items: []
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("w".to_string(), json!(0.0));
        let err = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap_err();
        assert_eq!(err.reason(), Some("size_invalid"));
    }

    /// A request that collapses an authored width to zero is refused as an invalid size wherever
    /// the item is a `text`, too. The layout pass runs before placement, so without the
    /// intrinsic-independent checks holding first this reported what the text then failed to do
    /// inside the zero box instead of the box being wrong.
    #[test]
    fn parameter_resolved_authored_size_zero_on_a_text_errors_with_size_invalid() {
        let yaml = r#"
name: Zero Size Text
unit: mm
dpi: 200
params:
  - name: w
    type: length
    default: 10
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: hello
    at: [0, 0]
    size: ["{w}", 10]
    font_size: 6
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("w".to_string(), json!(0.0));
        let err = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap_err();
        assert_eq!(err.reason(), Some("size_invalid"));
    }

    #[test]
    fn runtime_inverted_to_returns_edge_rect_inverted() {
        let yaml = r#"
name: Inverted To
unit: mm
dpi: 200
params:
  - name: target_width
    type: length
    default: 100
format:
  type: single
  width:
    min: 20
    max: "{target_width}"
  height: 20
layout:
  - type: container
    at: [30, 0]
    to: [-0.0, 20]
    items: []
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("target_width".to_string(), json!(20.0));
        let err = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap_err();
        assert_eq!(err.reason(), Some("edge_rect_inverted"));
    }

    /// A `to` that inverts only for this request is inverted whatever sits inside the box. The
    /// negative box must never reach the content, or a `text` reports what it then fails to do in
    /// it instead of the box being wrong: `edge_rect_inverted` takes priority over
    /// `text_does_not_fit`.
    #[test]
    fn runtime_inverted_to_on_a_text_returns_edge_rect_inverted() {
        let yaml = r#"
name: Inverted To Text
unit: mm
dpi: 200
params:
  - name: target_width
    type: length
    default: 100
format:
  type: single
  width:
    min: 20
    max: "{target_width}"
  height: 20
layout:
  - type: text
    value: hello
    at: [30, 0]
    to: [-0.0, 20]
    font_size: 6
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("target_width".to_string(), json!(20.0));
        let err = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap_err();
        assert_eq!(err.reason(), Some("edge_rect_inverted"));
    }

    #[test]
    fn rotated_container_frame_rect_outline_is_not_rotated() {
        let source = render_test_items(
            &[LayoutItem::Container {
                placement: Placement {
                    at: Some(Position([0.0, 0.0])),
                    extent: Extent::Size(Size([SizeValue::fixed(30.0), SizeValue::fixed(10.0)])),
                    max_w: None,
                    max_h: None,
                    rotate: Some(90.0),
                },
                shape: Shape::Rect,
                stroke: Some(Stroke {
                    thickness: 1.0,
                    color: DynamicValue::Literal(Color::black()),
                }),
                background: None,
                rounded: None,
                padding: crate::models::Padding::ZERO,
                flow: None,
                repeat: None,
                items: vec![],
                when: None,
            }],
            (100.0, 100.0),
        )
        .expect("render");
        assert!(source.contains(
            "#box(width: 30mm, height: 10mm, fill: none, stroke: 1mm + rgb(\"#000000ff\"), radius: 0mm, clip: true)"
        ));
        assert!(!source.contains("#rotate(90deg, origin: center)[#box(width: 30mm"));
    }

    #[test]
    fn shape_paint_source_emission() {
        // Container with stroke only (emits fill: none)
        let stroke_only = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.5,
                color: DynamicValue::Literal(Color::from_rgba(255, 0, 0, 255)),
            }),
            background: None,
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[stroke_only], (20.0, 10.0)).expect("render stroke only");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: none, stroke: 0.5mm + rgb(\"#ff0000ff\"), radius: 0mm, clip: true)"),
            "got: {src}"
        );

        // Container with background only (emits stroke: none)
        let bg_only = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: Some(DynamicValue::Literal(Color::from_rgba(0, 0, 128, 255))),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[bg_only], (20.0, 10.0)).expect("render bg only");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#000080ff\"), stroke: none, radius: 0mm, clip: true)"),
            "got: {src}"
        );

        // Container with both
        let both = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.2,
                color: DynamicValue::Literal(Color::from_rgba(0, 255, 0, 255)),
            }),
            background: Some(DynamicValue::Literal(Color::from_rgba(255, 255, 0, 255))),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[both], (20.0, 10.0)).expect("render both");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#ffff00ff\"), stroke: 0.2mm + rgb(\"#00ff00ff\"), radius: 0mm, clip: true)"),
            "got: {src}"
        );

        // Container with rounded clamped to min(w, h)/2
        // w=20, h=10 -> max radius is 5.0. Requested radius is 8.0 -> clamped to 5.0
        let rounded_clamped = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.2,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: None,
            rounded: Some(8.0),
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src =
            render_test_items(&[rounded_clamped], (20.0, 10.0)).expect("render rounded clamped");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: none, stroke: 0.2mm + rgb(\"#000000ff\"), radius: 5mm, clip: true)"),
            "got: {src}"
        );

        // Container with rounded fill and no stroke (stroke: none, radius: 1.5mm)
        let rounded_fill_no_stroke = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: Some(DynamicValue::Literal(Color::from_rgba(0, 0, 0, 255))),
            rounded: Some(1.5),
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[rounded_fill_no_stroke], (20.0, 10.0))
            .expect("render rounded fill no stroke");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#000000ff\"), stroke: none, radius: 1.5mm, clip: true)"),
            "got: {src}"
        );

        // Container with neither
        let neither = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: None,
            background: None,
            rounded: Some(2.0),
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[neither], (20.0, 10.0)).expect("render neither");
        assert!(!src.contains("#rect"), "got: {src}");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: none, stroke: none, radius: 2mm, clip: true)"),
            "got: {src}"
        );

        // Line with custom colour
        let line = LayoutItem::Line {
            at: Position([0.0, 0.0]),
            to: Position([10.0, 5.0]),
            stroke: Some(Stroke {
                thickness: 0.4,
                color: DynamicValue::Literal(Color::from_rgba(0x80, 0, 0x80, 0xff)),
            }),
            when: None,
        };
        let src = render_test_items(&[line], (20.0, 10.0)).expect("render line");
        assert!(
            src.contains(
                "#line(start: (0mm, 0mm), end: (10mm, -5mm), stroke: 0.4mm + rgb(\"#800080ff\"))"
            ),
            "got: {src}"
        );

        // Line with omitted stroke emits no #line
        let strokeless_line = LayoutItem::Line {
            at: Position([0.0, 0.0]),
            to: Position([10.0, 5.0]),
            stroke: None,
            when: None,
        };
        let src_strokeless =
            render_test_items(&[strokeless_line], (20.0, 10.0)).expect("render strokeless line");
        assert!(!src_strokeless.contains("#line"), "got: {src_strokeless}");

        // Container with child holds child in single box
        let container_with_child = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.5,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: Some(DynamicValue::Literal(Color::from_rgba(255, 0, 0, 255))),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![LayoutItem::Text {
                value: "child_text".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(10.0), SizeValue::fixed(5.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                color: None,
                wrap: false,
                line_spacing: None,
                alignment: crate::models::Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            }],
        };
        let src = render_test_items(&[container_with_child], (20.0, 10.0))
            .expect("render container with child");
        assert!(src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#ff0000ff\"), stroke: 0.5mm + rgb(\"#000000ff\"), radius: 0mm, clip: true)["));
        assert!(src.contains("child_text"));
    }

    #[test]
    fn circle_render_time_squareness_check() {
        // 1. Param-dependent circle that resolves non-square fails with circle_box_not_square
        let yaml = r#"
name: CircleParamTest
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: w
    type: length
    default: 12
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: ["{w}", 12]
    items: []
"#;
        let template = crate::parse::parse_template(yaml).unwrap();
        assert!(template.validate().is_ok());

        // When w=12, square -> success
        let mut data_ok = HashMap::new();
        data_ok.insert("w".to_string(), serde_json::json!(12.0));
        assert!(render_single_label(&template, &data_ok, &no_settings(), &no_datetime()).is_ok());

        // When w=14, not square -> 422 circle_box_not_square
        let mut data_bad = HashMap::new();
        data_bad.insert("w".to_string(), serde_json::json!(14.0));
        let err =
            render_single_label(&template, &data_bad, &no_settings(), &no_datetime()).unwrap_err();
        assert_eq!(err.reason(), Some("circle_box_not_square"));
        assert_eq!(err.status(), 422);
        assert!(err.message_text().contains("layout[0]"));

        // 2. Inactive non-square circle (when is false) succeeds
        let yaml_when = r#"
name: CircleWhenTest
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: w
    type: length
    default: 14
  - name: show
    type: enum
    values: [yes, no]
    default: no
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: ["{w}", 12]
    when: { show: yes }
    items: []
"#;
        let template_when = crate::parse::parse_template(yaml_when).unwrap();
        let mut data_inactive = HashMap::new();
        data_inactive.insert("w".to_string(), serde_json::json!(14.0));
        data_inactive.insert("show".to_string(), serde_json::json!("no"));
        assert!(render_single_label(
            &template_when,
            &data_inactive,
            &no_settings(),
            &no_datetime()
        )
        .is_ok());

        // 3. Content-derived circle that resolves non-square fails with circle_box_not_square
        let yaml_content = r#"
name: CircleContentTest
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: [content, content]
    items:
      - type: text
        value: "Long text item"
        at: [0, 0]
        size: [30, 10]
        font_size: 10
"#;
        let template_content = crate::parse::parse_template(yaml_content).unwrap();
        let err = render_single_label(
            &template_content,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
        )
        .unwrap_err();
        assert_eq!(err.reason(), Some("circle_box_not_square"));
        assert!(err.message_text().contains("layout[0]"));

        // 4. Batch render reports circle_box_not_square in failures for failing row while rendering valid row
        let settings = no_settings();
        let datetime = no_datetime();
        let batch_env = crate::batch::BatchEnv {
            settings: &settings,
            datetime: &datetime,
            render_opts: super::ImageRenderOptions::default(),
        };
        let template_def = crate::templates::TemplateDefinition {
            id: "CircleParamTest".to_string(),
            group: None,
            content: template,
        };
        let labels = vec![
            crate::models::LabelInput { data: data_ok },
            crate::models::LabelInput { data: data_bad },
        ];
        let err = crate::batch::render_batch(
            &template_def,
            &labels,
            crate::batch::BatchMode::Download,
            None,
            0,
            &batch_env,
            500,
        )
        .unwrap_err();
        assert_eq!(err.status(), 422);
        assert_eq!(err.code(), "BatchInvalid");
        let failures = &err.details().as_ref().unwrap()["failures"];
        assert_eq!(failures[0]["index"], 1);
        assert_eq!(failures[0]["code"], "UnsupportedLayoutItem");
        assert_eq!(failures[0]["reason"], "circle_box_not_square");
        assert!(failures[0]["message"]
            .as_str()
            .unwrap()
            .contains("layout[0]"));

        // 5. Epsilon boundary: <= 0.0001 succeeds, > 0.0001 fails
        let yaml_eps = r#"
name: CircleEpsTest
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: w
    type: length
    default: 10
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: ["{w}", 10]
    items: []
"#;
        let template_eps = crate::parse::parse_template(yaml_eps).unwrap();

        let mut data_eps_ok = HashMap::new();
        data_eps_ok.insert("w".to_string(), serde_json::json!(10.00009));
        assert!(
            render_single_label(&template_eps, &data_eps_ok, &no_settings(), &no_datetime())
                .is_ok()
        );

        let mut data_eps_bad = HashMap::new();
        data_eps_bad.insert("w".to_string(), serde_json::json!(10.00011));
        let err = render_single_label(&template_eps, &data_eps_bad, &no_settings(), &no_datetime())
            .unwrap_err();
        assert_eq!(err.reason(), Some("circle_box_not_square"));
        assert!(err.message_text().contains("layout[0]"));
    }

    #[test]
    fn container_geometry_emission_and_nesting() {
        // 1. shape: rect emits single #box with fill, stroke, radius, clip: true
        let rect_item = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
            ),
            when: None,
            shape: Shape::Rect,
            stroke: Some(Stroke {
                thickness: 0.5,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: Some(DynamicValue::Literal(Color::from_rgba(255, 0, 0, 255))),
            rounded: Some(2.0),
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[rect_item], (20.0, 10.0)).expect("render rect");
        assert!(src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#ff0000ff\"), stroke: 0.5mm + rgb(\"#000000ff\"), radius: 2mm, clip: true)[]"));
        assert!(!src.contains("#rect"));

        // 2. shape: ellipse emits #ellipse then #box with clip: true (unstroked and unrounded)
        let ellipse_item = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(30.0), SizeValue::fixed(20.0)]),
            ),
            when: None,
            shape: Shape::Ellipse,
            stroke: Some(Stroke {
                thickness: 0.5,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: Some(DynamicValue::Literal(Color::from_rgba(0, 255, 0, 255))),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src_ellipse = render_test_items(&[ellipse_item], (30.0, 20.0)).expect("render ellipse");
        assert!(src_ellipse.contains("#ellipse(width: 30mm, height: 20mm, fill: rgb(\"#00ff00ff\"), stroke: 0.5mm + rgb(\"#000000ff\"))"));
        assert!(src_ellipse.contains("#box(width: 30mm, height: 20mm, clip: true)[]"));
        assert!(
            src_ellipse.find("#ellipse").unwrap()
                < src_ellipse
                    .find("#box(width: 30mm, height: 20mm, clip: true)")
                    .unwrap(),
            "ellipse paint must precede child box"
        );

        // 3. shape: circle emits #ellipse on a square box
        let circle_item = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
            ),
            when: None,
            shape: Shape::Circle,
            stroke: Some(Stroke {
                thickness: 0.5,
                color: DynamicValue::Literal(Color::black()),
            }),
            background: Some(DynamicValue::Literal(Color::from_rgba(0, 0, 255, 255))),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src_circle = render_test_items(&[circle_item], (20.0, 20.0)).expect("render circle");
        assert!(src_circle.contains("#ellipse(width: 20mm, height: 20mm, fill: rgb(\"#0000ffff\"), stroke: 0.5mm + rgb(\"#000000ff\"))"));
        assert!(src_circle.contains("#box(width: 20mm, height: 20mm, clip: true)[]"));
        assert!(!src_circle.contains("#circle"));
        assert!(
            src_circle.find("#ellipse").unwrap()
                < src_circle
                    .find("#box(width: 20mm, height: 20mm, clip: true)")
                    .unwrap(),
            "circle ellipse paint must precede child box"
        );

        // 4. Strokeless and fill-less ellipse emits no #ellipse, just the clip box
        let strokeless_ellipse = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(30.0), SizeValue::fixed(20.0)]),
            ),
            when: None,
            shape: Shape::Ellipse,
            stroke: None,
            background: None,
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src_strokeless_el = render_test_items(&[strokeless_ellipse], (30.0, 20.0))
            .expect("render strokeless ellipse");
        assert!(!src_strokeless_el.contains("#ellipse"));
        assert!(src_strokeless_el.contains("#box(width: 30mm, height: 20mm, clip: true)[]"));

        // 5. Nested containers of mixed shapes compile and render to PNG
        let yaml_nested = r#"
name: MixedNestedShapes
unit: mm
dpi: 200
format: { type: single, width: 60, height: 60 }
layout:
  - type: container
    at: [0, 0]
    shape: rect
    size: [60, 60]
    stroke: { thickness: 0.5, color: black }
    background: '#f0f0f0'
    items:
      - type: container
        at: [5, 5]
        shape: circle
        size: [50, 50]
        stroke: { thickness: 0.5, color: blue }
        background: '#e0e0ff'
        items:
          - type: container
            at: [5, 10]
            shape: ellipse
            size: [40, 30]
            stroke: { thickness: 0.5, color: red }
            background: '#ffe0e0'
            items:
              - type: text
                value: "Nested"
                at: [5, 5]
                size: [30, 20]
                font_size: 8
"#;
        let template_nested = crate::parse::parse_template(yaml_nested).unwrap();
        let png = render_single_label_image(
            &template_nested,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
            super::ImageRenderOptions::default(),
        )
        .expect("render nested shapes png");
        assert!(!png.is_empty());
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn container_fixtures_emit_expected_typst_and_pdf() {
        fn source_for_yaml(yaml: &str) -> String {
            let template = crate::parse::parse_template(yaml).unwrap();
            let data: HashMap<String, serde_json::Value> = HashMap::new();
            let settings = no_settings();
            let datetime = no_datetime();
            let env = super::RenderEnv {
                settings: &settings,
                datetime: &datetime,
            };
            let compiled = super::compile_label_source(&template, &data, &env).expect("compile");
            compiled.source
        }
        fn assert_source_contains(yaml: &str, needle: &str) {
            let src = source_for_yaml(yaml);
            assert!(src.contains(needle), "expected {needle} in {src}");
        }
        // 1. Ellipse touching all four sides (46x26 in 50x30)
        let yaml_padded =
            std::fs::read_to_string("tests/fixtures/templates/container_ellipse_padded.yaml")
                .unwrap();
        assert_source_contains(&yaml_padded, "#ellipse(width: 46mm, height: 26mm");
        // also check padded inner box present
        let template_padded = crate::parse::parse_template(&yaml_padded).unwrap();
        let png = render_single_label_image(
            &template_padded,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
            super::ImageRenderOptions::default(),
        )
        .expect("png padded");
        assert_eq!(&png[1..4], b"PNG");
        let pdf = render_single_label_pdf(
            &template_padded,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
        )
        .expect("pdf padded");
        assert_eq!(&pdf[0..4], b"%PDF");

        // 2. Square box makes ellipse a circle (30x30)
        let yaml_square =
            std::fs::read_to_string("tests/fixtures/templates/container_ellipse_square.yaml")
                .unwrap();
        assert_source_contains(&yaml_square, "#ellipse(width: 30mm, height: 30mm");
        let template_square = crate::parse::parse_template(&yaml_square).unwrap();
        let pdf2 = render_single_label_pdf(
            &template_square,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
        )
        .expect("pdf square");
        assert_eq!(&pdf2[0..4], b"%PDF");

        // 3. Ellipse stroked cross – source has ellipse before box
        let yaml_cross = std::fs::read_to_string(
            "tests/fixtures/templates/container_ellipse_stroked_cross.yaml",
        )
        .unwrap();
        let src_cross = source_for_yaml(&yaml_cross);
        assert!(src_cross.contains("#ellipse(width: 46mm, height: 26mm"));
        assert!(src_cross.contains("#box(width: 46mm, height: 26mm, clip: true)"));
        assert!(
            src_cross.find("#ellipse").unwrap()
                < src_cross
                    .find("#box(width: 46mm, height: 26mm, clip: true)")
                    .unwrap()
        );

        // 4. Rect rounded corner – single box with radius and clip
        let yaml_rounded =
            std::fs::read_to_string("tests/fixtures/templates/container_rect_rounded_corner.yaml")
                .unwrap();
        assert_source_contains(&yaml_rounded, "radius: 6mm, clip: true");
        assert_source_contains(&yaml_rounded, "#box(width: 46mm, height: 26mm");
        assert!(!source_for_yaml(&yaml_rounded).contains("#ellipse"));

        // 5. Rect stroked edge – single box with stroke, clip true
        let yaml_edge =
            std::fs::read_to_string("tests/fixtures/templates/container_rect_stroked_edge.yaml")
                .unwrap();
        assert_source_contains(&yaml_edge, "stroke: 1mm");
        assert_source_contains(&yaml_edge, "clip: true");
    }

    #[test]
    fn circle_dynamic_width_frame_sourced_extent_checked_at_final_frame() {
        // Fill width under width: {min:10,max:60} – the frame follows label sizing, which is
        // decided per render. Probing at max_w (60) would misclassify.
        let yaml_oval = r#"
name: DynOval
unit: mm
dpi: 200
format:
  type: single
  width: { min: 10, max: 60 }
  height: 60
layout:
  - type: text
    value: hi
    at: [0, 0]
    size: [20, 5]
    font_size: 8
  - type: container
    at: [0, 0]
    shape: circle
    size: [fill, 60]
    items: []
"#;
        let template_oval = crate::parse::parse_template(yaml_oval).unwrap();
        // load succeeds – frame source defers to render
        assert!(template_oval.validate().is_ok());
        let err = render_single_label_image(
            &template_oval,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
            super::ImageRenderOptions::default(),
        )
        .unwrap_err();
        assert_eq!(err.reason(), Some("circle_box_not_square"));
        assert!(err.message_text().contains("layout[1]"));

        // Square counterpart – fill 20 in final 20-wide label is square and must render.
        let yaml_circle = r#"
name: DynCircle
unit: mm
dpi: 200
format:
  type: single
  width: { min: 10, max: 60 }
  height: 20
layout:
  - type: text
    value: hi
    at: [0, 0]
    size: [20, 5]
    font_size: 8
  - type: container
    at: [0, 0]
    shape: circle
    size: [fill, 20]
    items: []
"#;
        let template_circle = crate::parse::parse_template(yaml_circle).unwrap();
        assert!(template_circle.validate().is_ok());
        let png = render_single_label_image(
            &template_circle,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
            super::ImageRenderOptions::default(),
        )
        .expect("square circle must render");
        assert_eq!(&png[1..4], b"PNG");
        let pdf = render_single_label_pdf(
            &template_circle,
            &HashMap::new(),
            &no_settings(),
            &no_datetime(),
        )
        .expect("square circle pdf");
        assert_eq!(&pdf[0..4], b"%PDF");
    }

    #[test]
    fn shape_paint_renders_png_and_pdf() {
        let yaml = r#"
name: Shape Paint Test
unit: mm
dpi: 200
format:
  type: single
  width: 40
  height: 30
layout:
  - type: container
    at: [2, 2]
    size: [36, 26]
    stroke:
      thickness: 0.5
      color: '#ff0000'
    background: '#00ff0080'
    rounded: 3.0
    items:
      - type: line
        at: [1, 1]
        to: [30, 20]
        stroke:
          thickness: 0.3
          color: blue
      - type: text
        value: "Test"
        at: [2, 2]
        size: [20, 10]
        font_size: 8
"#;
        let template = crate::parse::parse_template(yaml).unwrap();
        let data = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();

        let png = render_single_label_image(
            &template,
            &data,
            &settings,
            &datetime,
            super::ImageRenderOptions::default(),
        )
        .expect("render png");
        assert!(!png.is_empty());
        assert_eq!(&png[1..4], b"PNG");

        let pdf =
            render_single_label_pdf(&template, &data, &settings, &datetime).expect("render pdf");
        assert!(!pdf.is_empty());
        assert_eq!(&pdf[0..4], b"%PDF");
    }

    #[test]
    fn raster_image_dimensions_rejects_mismatched_mime_format() {
        let jpeg_bytes = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x01,
            0x00, 0x48, 0x00, 0x48, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d,
            0x0c, 0x0b, 0x0b, 0x0c, 0x19, 0x12, 0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d,
            0x1a, 0x1c, 0x1c, 0x20, 0x24, 0x2e, 0x27, 0x20, 0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28,
            0x37, 0x29, 0x2c, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1f, 0x27, 0x39, 0x3d, 0x38, 0x32,
            0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01, 0x00, 0x01,
            0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xda, 0x00, 0x08, 0x01,
            0x01, 0x00, 0x00, 0x3f, 0x00, 0xbf, 0x00, 0xff, 0xd9,
        ];
        let res = super::helpers::raster_image_dimensions(
            &jpeg_bytes,
            super::helpers::ImageFmt::Png,
            200,
            "mm",
            "layout[0]",
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().reason(), Some("intrinsic_size_undefined"));
    }

    #[test]
    fn shrinking_to_extent_is_cap_inert() {
        let yaml = r#"
name: Shrinking To Cap Inert
unit: mm
dpi: 200
format: { type: single, width: 100, height: 20 }
layout:
  - type: container
    at: [-20, 0]
    to: [90, 20]
    max_w: 5
    items: []
"#;
        let template = parse_and_validate(yaml).unwrap();
        let source = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver());
        assert!(source.is_ok());
    }

    #[test]
    fn capped_content_container_bounds_child_frame() {
        let yaml = r#"
name: Capped Content Container
unit: mm
dpi: 200
format: { type: single, width: 100, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    max_w: 20
    items:
      - type: text
        value: "A long message that would wrap beyond 20mm"
        at: [0, 0]
        size: [fill, 20]
        overflow: fail
        font_size: 14
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("text_does_not_fit"));
    }

    #[test]
    fn empty_text_with_overflow_fail_in_zero_box_renders_empty() {
        let yaml = r#"
name: Empty Text Zero Box
unit: mm
dpi: 200
params:
  - name: msg
    type: string
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: "{msg}"
    at: [0, 0]
    size: [content, content]
    overflow: fail
    font_size: 12
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("msg".to_string(), serde_json::json!(""));
        let png = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn svg_absolute_units_pc_and_q_parse_correctly() {
        let svg_pc = r#"<svg xmlns="http://www.w3.org/2000/svg" width="6pc" height="12pc" viewBox="0 0 100 200"></svg>"#;
        let w = super::helpers::svg_axis_intrinsic(svg_pc, 0, "mm", 200, "layout[0]").unwrap();
        assert!((w - 25.4).abs() < 1e-3);

        let svg_q = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100q" height="200q" viewBox="0 0 100 200"></svg>"#;
        let w_q = super::helpers::svg_axis_intrinsic(svg_q, 0, "mm", 200, "layout[0]").unwrap();
        assert!((w_q - 25.0).abs() < 1e-3);
    }

    /// Proves that flow container primary and secondary overruns fail with `item_out_of_frame`
    /// and identify the offending child index in the error path.
    #[test]
    fn flow_row_overflow_errors_with_item_out_of_frame() {
        let yaml = r#"
name: Row Overflow
unit: mm
dpi: 200
format: { type: single, width: 30, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [30, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "A"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "B"
        size: [15, 10]
        font_size: 8
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("item_out_of_frame"));
        assert!(
            err.message_text().contains("items[1]"),
            "expected error at child index 1, got: {}",
            err.message_text()
        );
    }

    #[test]
    fn flow_column_overflow_errors_with_item_out_of_frame() {
        let yaml = r#"
name: Column Overflow
unit: mm
dpi: 200
format: { type: single, width: 30, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [30, 20]
    flow: { direction: column, gap: 5 }
    items:
      - type: text
        value: "A"
        size: [10, 15]
        font_size: 8
      - type: text
        value: "B"
        size: [10, 10]
        font_size: 8
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("item_out_of_frame"));
        assert!(
            err.message_text().contains("items[1]"),
            "expected error at child index 1, got: {}",
            err.message_text()
        );
    }

    #[test]
    fn flow_secondary_axis_overflow_errors_with_item_out_of_frame() {
        let yaml = r#"
name: Too Tall Child
unit: mm
dpi: 200
params:
  - name: h
    type: number
    default: 15
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    flow: { direction: row }
    items:
      - type: text
        value: "A"
        size: [20, "{h}"]
        font_size: 8
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("h".to_string(), serde_json::json!(25));
        let err = render_single_label(&template, &data, &BTreeMap::new(), &resolver()).unwrap_err();
        assert_eq!(err.reason(), Some("item_out_of_frame"));
    }

    #[test]
    fn flow_overflow_in_measurement_with_gated_sibling_names_correct_child_index() {
        let yaml = r#"
name: Measurement Gated Overflow
unit: mm
dpi: 200
params:
  - name: show_first
    type: enum
    values: ["yes", "no"]
    default: "no"
format: { type: single, width: 40, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [fill, fill]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        when: { show_first: "yes" }
        value: "Gated Off"
        size: [10, 10]
        font_size: 8
      - type: text
        value: "First Active"
        size: [25, 10]
        font_size: 8
      - type: text
        value: "Second Active Overrun"
        size: [25, 10]
        font_size: 8
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("item_out_of_frame"));
        assert!(
            err.message_text().contains("items[2]"),
            "expected error to name items[2], got: {}",
            err.message_text()
        );
    }

    /// Proves that packed children inside flow containers size consistently against their padded
    /// inner box and interact with dynamic and fixed layouts identically to anchored children at origin.
    #[test]
    fn packed_child_sized_identically_to_unpacked_at_origin() {
        let yaml_abs = r#"
name: Abs Container
unit: mm
dpi: 200
format: { type: single, width: 60, height: 30 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 30]
    items:
      - type: text
        value: "Hello"
        at: [0, 0]
        size: [fill, fill]
        font_size: 8
"#;
        let yaml_flow = r#"
name: Flow Container
unit: mm
dpi: 200
format: { type: single, width: 60, height: 30 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 30]
    flow: { direction: row }
    items:
      - type: text
        value: "Hello"
        size: [fill, fill]
        font_size: 8
"#;
        let t_abs = parse_and_validate(yaml_abs).unwrap();
        let t_flow = parse_and_validate(yaml_flow).unwrap();
        let src_abs =
            render_single_label(&t_abs, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap();
        let src_flow =
            render_single_label(&t_flow, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap();
        assert_eq!(src_abs, src_flow);
    }

    #[test]
    fn uncapped_and_capped_fill_child_in_flow() {
        // Uncapped fill child alone: gets full width (80mm)
        let yaml_alone = r#"
name: Fill Alone
unit: mm
dpi: 200
format: { type: single, width: 80, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [80, 20]
    flow: { direction: row }
    items:
      - type: text
        value: "Alone"
        size: [fill, 10]
        font_size: 8
"#;
        let t_alone = parse_and_validate(yaml_alone).unwrap();
        let res_alone =
            render_single_label(&t_alone, &HashMap::new(), &BTreeMap::new(), &resolver());
        assert!(res_alone.is_ok());

        // Uncapped fill child beside sibling: overruns because fill claims full 80mm
        let yaml_overrun = r#"
name: Fill Overrun
unit: mm
dpi: 200
format: { type: single, width: 80, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [80, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "First"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "Second"
        size: [fill, 10]
        font_size: 8
"#;
        let t_overrun = parse_and_validate(yaml_overrun).unwrap();
        let err_overrun =
            render_single_label(&t_overrun, &HashMap::new(), &BTreeMap::new(), &resolver())
                .unwrap_err();
        assert_eq!(err_overrun.reason(), Some("item_out_of_frame"));

        // Capped fill child sharing line: fits within 80mm
        let yaml_capped = r#"
name: Capped Fill
unit: mm
dpi: 200
format: { type: single, width: 80, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [80, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "First"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "Second"
        size: [fill, 10]
        max_w: 55
        font_size: 8
"#;
        let t_capped = parse_and_validate(yaml_capped).unwrap();
        let res_capped =
            render_single_label(&t_capped, &HashMap::new(), &BTreeMap::new(), &resolver());
        assert!(res_capped.is_ok());
    }

    #[test]
    fn content_flow_container_hugs_children_in_both_directions() {
        let yaml_row = r#"
name: Content Row
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [content, content]
    padding: 2
    flow: { direction: row, gap: 4 }
    items:
      - type: text
        value: "A"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "B"
        size: [30, 15]
        font_size: 8
"#;
        let t_row = parse_and_validate(yaml_row).unwrap();
        let source_row = render_test_items(
            match &t_row.layout {
                Layout::Items(items) => items,
            },
            (100.0, 100.0),
        )
        .expect("render");
        // w: 20 + 30 + 4 + 4(pad) = 58mm; h: 15 + 4(pad) = 19mm
        assert!(
            source_row.contains("width: 58mm"),
            "expected 58mm width in: {source_row}"
        );
        assert!(
            source_row.contains("height: 19mm"),
            "expected 19mm height in: {source_row}"
        );

        let yaml_col = r#"
name: Content Col
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
layout:
  - type: container
    at: [0, 0]
    size: [content, content]
    padding: 2
    flow: { direction: column, gap: 4 }
    items:
      - type: text
        value: "A"
        size: [20, 10]
        font_size: 8
      - type: text
        value: "B"
        size: [30, 15]
        font_size: 8
"#;
        let t_col = parse_and_validate(yaml_col).unwrap();
        let source_col = render_test_items(
            match &t_col.layout {
                Layout::Items(items) => items,
            },
            (100.0, 100.0),
        )
        .expect("render");
        // w: 30 + 4(pad) = 34mm; h: 10 + 15 + 4 + 4(pad) = 33mm
        assert!(
            source_col.contains("width: 34mm"),
            "expected 34mm width in: {source_col}"
        );
        assert!(
            source_col.contains("height: 33mm"),
            "expected 33mm height in: {source_col}"
        );
    }

    #[test]
    fn flow_container_sizes_dynamic_width_label() {
        let yaml = r#"
name: Dynamic Flow
unit: mm
dpi: 200
format:
  type: single
  width: { min: 10, max: 100 }
  height: 20
layout:
  - type: container
    at: [0, 0]
    size: [content, 20]
    flow: { direction: row, gap: 5 }
    items:
      - type: text
        value: "Hello"
        size: [content, 10]
        font_size: 8
      - type: text
        value: "World"
        size: [content, 10]
        font_size: 8
"#;
        let template = parse_and_validate(yaml).unwrap();
        let png =
            render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        // Sized to sum of content + gap, not min (10mm) or max (100mm)
        let min_px = (10.0_f32 / 25.4 * 200.0).round() as u32;
        let max_px = (100.0_f32 / 25.4 * 200.0).round() as u32;
        assert!(img.width() > min_px);
        assert!(img.width() < max_px);
    }

    #[test]
    fn nested_flow_containers_render_in_both_directions() {
        let yaml = r#"
name: Nested Flow
unit: mm
dpi: 200
format: { type: single, width: 80, height: 40 }
layout:
  - type: container
    at: [0, 0]
    size: [80, 40]
    flow: { direction: row, gap: 4 }
    padding: 2
    items:
      - type: container
        size: [30, fill]
        flow: { direction: column, gap: 2 }
        items:
          - type: text
            value: "Line 1"
            size: [fill, 10]
            font_size: 8
          - type: text
            value: "Line 2"
            size: [fill, 10]
            font_size: 8
      - type: qr
        value: "NESTED"
        size: [20, 20]
"#;
        let template = parse_and_validate(yaml).unwrap();
        let png =
            render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn flow_container_at_sheet_slot_root() {
        let yaml = r#"
name: Sheet Flow Root
unit: mm
dpi: 200
format:
  type: sheet
  paper_width: 210
  paper_height: 297
  label_width: 60
  label_height: 30
  positions: [[10, 10], [80, 10]]
layout:
  - type: container
    at: [0, 0]
    size: [60, 30]
    flow: { direction: row, gap: 4 }
    padding: 2
    items:
      - type: text
        value: "Slot Label"
        size: [20, 10]
        font_size: 8
      - type: qr
        value: "DATA"
        size: [10, 10]
"#;
        let template_content = parse_and_validate(yaml).unwrap();
        let template = TemplateDefinition {
            id: "sheet_flow".to_string(),
            group: None,
            content: template_content,
        };
        let labels = vec![LabelInput {
            data: HashMap::new(),
        }];
        let pdf =
            render_sheet_pages(&template, &labels, 0, &no_settings(), &no_datetime()).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    /// #245 acceptance: a centered multiline text item in an 18.1mm box reserves its ink
    /// and fits at 21.0pt rather than 24.0pt, keeping its descender closed and off the final raster row.
    #[test]
    fn center_aligned_multiline_auto_shrink_descender_fits_and_closes_stroke() {
        let item = LayoutItem::Text {
            value: "Kitchen Utensils and a much longer second line here".to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(120.0), SizeValue::fixed(18.1)]),
            ),
            font_size: FontSize::Range {
                min: 10.0,
                max: 32.0,
            },
            font_weight: None,
            color: None,
            wrap: true,
            line_spacing: None,
            alignment: crate::models::Alignment {
                horizontal: HorizontalAlign::Center,
                vertical: VerticalAlign::Center,
            },
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let src = render_test_items(&[item], (120.0, 18.1)).expect("render text item");
        let size = fitted_pt(&src);
        assert_eq!(size, 21.0, "fitted size after fix should be 21.0pt");

        let yaml = r#"
name: Issue 245 Repro
unit: mm
dpi: 180
format: { type: single, width: 120, height: 18.1 }
layout:
  - type: text
    value: "Kitchen Utensils and a much longer second line here"
    at: [0, 0]
    size: [120, 18.1]
    font_size:
      min: 10
      max: 32
    wrap: true
    alignment:
      horizontal: center
      vertical: center
"#;
        let template = parse_and_validate(yaml).unwrap();
        let png =
            render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap();
        let img = image::load_from_memory(&png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let last_row = h - 1;
        let inked_cols: Vec<u32> = (0..w)
            .filter(|&x| img.get_pixel(x, last_row).0[0] < 128)
            .collect();
        assert!(
            inked_cols.is_empty(),
            "expected no ink on the final raster row {last_row}, but found {} inked pixels",
            inked_cols.len()
        );
    }

    /// Task 5.2: Render the four catalog tapes and confirm they are unchanged from the baseline.
    #[test]
    fn catalog_brother_tapes_render_unchanged_from_baseline() {
        let (registry, _dir) = crate::templates::load_all_for_tests();
        let archive_dir = std::path::Path::new(
            "openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution/renders",
        );

        for tape_id in [
            "brother_9mm",
            "brother_12mm",
            "brother_18mm",
            "brother_24mm",
        ] {
            let template = registry.get(tape_id).expect("catalog template");
            let mut data = HashMap::new();
            data.insert("message".to_string(), json!("BOX.073 - Floor Grinder"));
            let png = render_single_label(template, &data, &no_settings(), &no_datetime())
                .unwrap_or_else(|e| panic!("render {tape_id}: {e:?}"));

            let baseline_path = archive_dir.join(format!("{tape_id}.png"));
            let baseline = std::fs::read(&baseline_path)
                .unwrap_or_else(|e| panic!("missing baseline PNG {baseline_path:?}: {e}"));
            assert_eq!(
                png, baseline,
                "rendered PNG for {tape_id} differs from baseline {baseline_path:?}"
            );
        }
    }

    #[test]
    fn when_predicate_with_omitted_param_evaluates_false() {
        let yaml = r#"
name: Test When Omitted
unit: mm
dpi: 200
params:
  - name: bold
    type: boolean
  - name: mode
    type: enum
    values: [draft, final]
format: { type: single, width: 100, height: 20 }
layout:
  - type: container
    when:
      bold: "false"
    at: [0, 0]
    size: [50, 10]
    items:
      - type: text
        value: "Bold is false branch"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
  - type: container
    when:
      mode: draft
    at: [0, 10]
    size: [50, 10]
    items:
      - type: text
        value: "Draft branch"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let res_dt = resolver();
        let empty_settings = BTreeMap::new();
        let env = super::RenderEnv {
            settings: &empty_settings,
            datetime: &res_dt,
        };

        // When bold and mode are omitted (no defaults), neither branch selects (both are inactive)
        let resolved_omitted =
            super::resolve_parameters(&template, &HashMap::new(), None, Some(&res_dt)).unwrap();
        let ctx = super::RenderContext::new("mm", 200, &resolved_omitted.data, &env, &images)
            .with_instants(&resolved_omitted.instants);
        assert!(
            !ctx.is_item_active(&items[0]),
            "when: {{ bold: 'false' }} must not select when bold is omitted"
        );
        assert!(
            !ctx.is_item_active(&items[1]),
            "when: {{ mode: draft }} must not select when mode is omitted"
        );

        // When bold: false is explicitly provided, bold branch selects
        let mut with_bold_false = HashMap::new();
        with_bold_false.insert("bold".to_string(), json!(false));
        let resolved_bf =
            super::resolve_parameters(&template, &with_bold_false, None, Some(&res_dt)).unwrap();
        let ctx_bf = super::RenderContext::new("mm", 200, &resolved_bf.data, &env, &images)
            .with_instants(&resolved_bf.instants);
        assert!(ctx_bf.is_item_active(&items[0]));
        assert!(!ctx_bf.is_item_active(&items[1]));
    }

    #[test]
    fn when_predicate_only_default_resolution() {
        // 1. Literal default only when reads -> resolves and selects branch
        let yaml1 = r#"
name: Test When Literal Default
unit: mm
dpi: 200
params:
  - name: mode
    type: enum
    values: [draft, final]
    default: draft
format: { type: single, width: 100, height: 20 }
layout:
  - type: container
    when:
      mode: draft
    at: [0, 0]
    size: [50, 10]
    items:
      - type: text
        value: "Rendered"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template1 = parse_and_validate(yaml1).unwrap();
        let Layout::Items(items1) = &template1.layout;
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let res_dt = resolver();
        let empty_settings = BTreeMap::new();
        let env = super::RenderEnv {
            settings: &empty_settings,
            datetime: &res_dt,
        };
        let resolved1 =
            super::resolve_parameters(&template1, &HashMap::new(), None, Some(&res_dt)).unwrap();
        let ctx1 = super::RenderContext::new("mm", 200, &resolved1.data, &env, &images)
            .with_instants(&resolved1.instants);
        assert!(
            ctx1.is_item_active(&items1[0]),
            "default: draft must select when: mode: draft"
        );

        // 2. Tokened default that fails resolution -> fails render with param_default_unresolvable
        let yaml2 = r#"
name: Test When Broken Default
unit: mm
dpi: 200
params:
  - name: mode
    type: enum
    values: [draft, final]
    default: "{vars.missing}"
format: { type: single, width: 100, height: 20 }
layout:
  - type: container
    when:
      mode: draft
    at: [0, 0]
    size: [50, 10]
    items:
      - type: text
        value: "Rendered"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
"#;
        let template2 = parse_and_validate(yaml2).unwrap();
        let err = render_single_label(&template2, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("param_default_unresolvable"));
    }

    #[test]
    fn unused_param_with_broken_default_fails_render() {
        let yaml = r#"
name: Test Unused Param Broken Default
unit: mm
dpi: 200
params:
  - name: unused
    type: string
    default: "{vars.missing}"
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: "Fixed Text"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let err = render_single_label(&template, &HashMap::new(), &BTreeMap::new(), &resolver())
            .unwrap_err();
        assert_eq!(err.reason(), Some("param_default_unresolvable"));
    }

    #[test]
    fn length_default_coercion_and_input_spec() {
        let yaml = r#"
name: Test Length Coercion
unit: mm
dpi: 200
params:
  - name: w
    type: length
    default: "80mm"
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: "{w}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let s = interpolated(&template, &HashMap::new(), &resolver()).unwrap();
        assert_eq!(s, "80.0");

        let inputs = test_inputs_all(&template);
        let input_w = inputs.iter().find(|i| i.name == "w").unwrap();
        assert_eq!(
            input_w.default,
            Some(crate::models::ParamValue::Float(80.0))
        );
    }

    #[test]
    fn string_param_null_value_stringifies_to_empty_string() {
        let yaml = r#"
name: Test String Null
unit: mm
dpi: 200
params:
  - name: title
    type: string
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: "Prefix:{title}:Suffix"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("title".to_string(), serde_json::Value::Null);
        let s = interpolated(&template, &data, &resolver()).unwrap();
        assert_eq!(s, "Prefix::Suffix");
    }

    #[test]
    fn csv_import_avery5163_without_outline_column() {
        let registry = crate::templates::load_all_for_tests().0;
        let template = registry
            .get("avery5163_asset_tag")
            .expect("avery5163_asset_tag template");

        let mut data = HashMap::new();
        data.insert("id".to_string(), json!("ITM-001"));
        data.insert("name".to_string(), json!("Asset 001"));
        data.insert("url".to_string(), json!("https://example.com/asset"));
        data.insert("tags".to_string(), json!("tools"));
        data.insert("description".to_string(), json!("Test asset"));
        data.insert("orientation".to_string(), json!("horizontal"));
        // outline is omitted (no default declared) -> outline container is inactive
        let labels = vec![crate::models::LabelInput { data: data.clone() }];
        let pdf = render_sheet_pages(template, &labels, 0, &BTreeMap::new(), &resolver()).unwrap();
        assert!(!pdf.is_empty());

        let Layout::Items(items) = &template.layout;
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let res_dt = resolver();
        let empty_settings = BTreeMap::new();
        let env = super::RenderEnv {
            settings: &empty_settings,
            datetime: &res_dt,
        };
        let resolved = super::resolve_parameters(template, &data, None, Some(&res_dt)).unwrap();
        let ctx = super::RenderContext::new("in", 300, &resolved.data, &env, &images)
            .with_instants(&resolved.instants);
        assert!(
            !ctx.is_item_active(&items[0]),
            "outline container must be inactive when outline is omitted"
        );
        assert!(
            ctx.is_item_active(&items[1]),
            "horizontal container must be active"
        );
    }

    #[test]
    fn emitted_typst_source_color_fill_and_omission() {
        use std::str::FromStr;
        // 1. Named color emits fill: rgb(...) with pinned components (CSS Level 1 red = 255, 0, 0, 255)
        let named_item = LayoutItem::Text {
            value: "Hello".to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(50.0), SizeValue::fixed(20.0)]),
            ),
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: Some(DynamicValue::Literal(
                crate::models::Color::from_str("red").unwrap(),
            )),
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let src_named = render_test_items(&[named_item], (50.0, 20.0)).expect("render named");
        assert!(
            src_named.contains("fill: rgb(\"#ff0000ff\")"),
            "red must emit rgb(\"#ff0000ff\"), got: {src_named}"
        );

        // 2. Hex color emits fill: rgb(...) with exact same components
        let hex_item = LayoutItem::Text {
            value: "Hello".to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(50.0), SizeValue::fixed(20.0)]),
            ),
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: Some(DynamicValue::Literal(
                crate::models::Color::from_str("#ff4136").unwrap(),
            )),
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let src_hex = render_test_items(&[hex_item], (50.0, 20.0)).expect("render hex");
        assert!(
            src_hex.contains("fill: rgb(\"#ff4136ff\")"),
            "#ff4136 must emit rgb(\"#ff4136ff\"), got: {src_hex}"
        );

        // 3. No color emits no fill: argument at all
        let no_color_item = LayoutItem::Text {
            value: "Hello".to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(50.0), SizeValue::fixed(20.0)]),
            ),
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let src_no_color =
            render_test_items(&[no_color_item], (50.0, 20.0)).expect("render no color");
        assert!(
            !src_no_color.contains("fill:"),
            "item with no color must emit no fill: argument, got: {src_no_color}"
        );
    }

    #[test]
    fn text_color_null_and_absent_render_black_e2e() {
        let yaml_null = r#"
name: ColorNullText
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "BLACK"
    at: [0, 0]
    size: [50, 20]
    font_size: 14
    color: null
"#;
        let template_null = crate::parse::parse_template(yaml_null).unwrap();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let data = HashMap::new();
        let compiled_null = super::compile_label_source(&template_null, &data, &env).unwrap();
        assert!(
            !compiled_null.source.contains("fill:"),
            "explicit color: null must emit no fill: in Typst, got: {}",
            compiled_null.source
        );

        let png_null = super::render_single_label(&template_null, &data, &settings, &datetime)
            .expect("render template with color: null");
        let img_null = image::load_from_memory(&png_null)
            .expect("decode png")
            .to_rgba8();
        let dark_pixels_null = img_null
            .pixels()
            .filter(|p| p[0] < 200 && p[0] == p[1] && p[1] == p[2])
            .count();
        assert!(
            dark_pixels_null > 0,
            "explicit color: null must render black text glyphs"
        );

        let yaml_absent = r#"
name: ColorAbsentText
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "BLACK"
    at: [0, 0]
    size: [50, 20]
    font_size: 14
"#;
        let template_absent = crate::parse::parse_template(yaml_absent).unwrap();
        let compiled_absent = super::compile_label_source(&template_absent, &data, &env).unwrap();
        assert!(
            !compiled_absent.source.contains("fill:"),
            "absent color must emit no fill: in Typst, got: {}",
            compiled_absent.source
        );

        let png_absent = super::render_single_label(&template_absent, &data, &settings, &datetime)
            .expect("render template with absent color");
        let img_absent = image::load_from_memory(&png_absent)
            .expect("decode png")
            .to_rgba8();
        let dark_pixels_absent = img_absent
            .pixels()
            .filter(|p| p[0] < 200 && p[0] == p[1] && p[1] == p[2])
            .count();
        assert!(
            dark_pixels_absent > 0,
            "absent color must render black text glyphs"
        );
    }

    #[test]
    fn emitted_typst_source_padded_color_literals() {
        let yaml = r#"
name: PaddedColorTest
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: " #F0F "
    stroke:
      thickness: 0.2
      color: " navy "
    items:
      - type: text
        value: "Hello"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
        color: " red "
"#;
        let template = crate::parse::parse_template(yaml).unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        let src = render_test_items(items, (50.0, 20.0)).expect("render padded template items");
        assert!(
            src.contains("fill: rgb(\"#ff00ffff\")"),
            "container background must emit rgb(\"#ff00ffff\"), got: {src}"
        );
        assert!(
            src.contains("rgb(\"#000080ff\")"),
            "container stroke must emit rgb(\"#000080ff\"), got: {src}"
        );
        assert!(
            src.contains("fill: rgb(\"#ff0000ff\")"),
            "text color must emit rgb(\"#ff0000ff\"), got: {src}"
        );
    }

    #[test]
    fn padded_color_reference_loads_and_renders() {
        let yaml = r#"
name: PaddedColorRef
unit: mm
dpi: 200
params:
  - name: brand
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: " {brand} "
"#;
        let template = crate::parse::parse_template(yaml).unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        match &items[0] {
            crate::models::LayoutItem::Text { color, .. } => {
                assert_eq!(
                    color,
                    &Some(crate::models::DynamicValue::Ref("brand".to_string()))
                );
            }
            _ => panic!("expected text"),
        }

        let mut data = HashMap::new();
        data.insert("brand".to_string(), serde_json::json!("red"));
        let settings = no_settings();
        let datetime = no_datetime();
        let res = super::render_single_label(&template, &data, &settings, &datetime);
        assert!(res.is_ok(), "padded reference must render successfully");
    }

    #[test]
    fn color_param_with_whitespace_renders_resolved_color() {
        let yaml = r#"
name: WhiteSpaceColorParam
unit: mm
dpi: 200
params:
  - name: brand
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
        let template = crate::parse::parse_template(yaml).unwrap();
        let settings = no_settings();
        let datetime = no_datetime();

        let mut data = HashMap::new();
        data.insert("brand".to_string(), serde_json::json!(" navy "));
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let compiled = super::compile_label_source(&template, &data, &env).unwrap();
        assert!(
            compiled.source.contains("fill: rgb(\"#000080ff\")"),
            "resolved ' navy ' must emit rgb(\"#000080ff\"), got: {}",
            compiled.source
        );
        let rendered = super::render_single_label(&template, &data, &settings, &datetime)
            .expect("padded brand must render");
        let img = image::load_from_memory(&rendered)
            .expect("valid png")
            .to_rgba8();
        let navy_pixels = img
            .pixels()
            .filter(|p| p[2] < 200 && p[0] < p[2] && p[1] < p[2])
            .count();
        assert!(
            navy_pixels > 0,
            "padded ' navy ' parameter must render navy text glyphs"
        );
    }

    #[test]
    fn color_param_with_whitespace_chained_ref_fails_with_chained_message() {
        let yaml = r#"
name: ChainedColorParam
unit: mm
dpi: 200
params:
  - name: brand
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
        let template = crate::parse::parse_template(yaml).unwrap();
        let settings = no_settings();
        let datetime = no_datetime();

        let mut data = HashMap::new();
        data.insert("brand".to_string(), serde_json::json!(" {other} "));
        let err = super::render_single_label(&template, &data, &settings, &datetime).unwrap_err();
        assert_eq!(err.reason(), Some(Reason::ColorParamInvalid.as_slug()));
        assert!(
            err.message_text().contains("references cannot be chained"),
            "expected chained-reference message, got: {}",
            err.message_text()
        );
    }

    #[test]
    fn color_changes_no_layout_metrics() {
        use std::str::FromStr;
        let make_item = |color: Option<&str>| LayoutItem::Text {
            value: "Some longer text that might wrap or size dynamically".to_string(),
            placement: Placement::sized(
                Position([2.0, 3.0]),
                Size([SizeValue::content(), SizeValue::fixed(15.0)]),
            ),
            font_size: FontSize::Range {
                min: 8.0,
                max: 24.0,
            },
            font_weight: None,
            color: color.map(|s| DynamicValue::Literal(crate::models::Color::from_str(s).unwrap())),
            wrap: true,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };

        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);
        let geometry_values = HashMap::new();

        let item_no_color = make_item(None);
        let item_red = make_item(Some("red"));
        let item_hex = make_item(Some("#0074d9"));

        let (measured_none, _) = ctx
            .measure_items(
                &[item_no_color],
                (100.0, 50.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .unwrap();
        let (measured_red, _) = ctx
            .measure_items(
                &[item_red],
                (100.0, 50.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .unwrap();
        let (measured_hex, _) = ctx
            .measure_items(
                &[item_hex],
                (100.0, 50.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .unwrap();

        assert_eq!(measured_none[0].intrinsic, measured_red[0].intrinsic);
        assert_eq!(measured_none[0].intrinsic, measured_hex[0].intrinsic);

        let fit_none = measured_none[0].text.as_ref().unwrap();
        let fit_red = measured_red[0].text.as_ref().unwrap();
        let fit_hex = measured_hex[0].text.as_ref().unwrap();

        assert_eq!(fit_none.font_size_pt, fit_red.font_size_pt);
        assert_eq!(fit_none.font_size_pt, fit_hex.font_size_pt);
        assert_eq!(fit_none.lines, fit_red.lines);
        assert_eq!(fit_none.lines, fit_hex.lines);
        assert_eq!(fit_none.width_units, fit_red.width_units);
        assert_eq!(fit_none.width_units, fit_hex.width_units);
        assert_eq!(fit_none.height_units, fit_red.height_units);
        assert_eq!(fit_none.height_units, fit_hex.height_units);
    }

    #[test]
    fn parameter_referenced_color_renders_on_background_and_stroke() {
        let yaml = r#"
name: ParamShapes
unit: mm
dpi: 200
params:
  - name: brand
    type: string
  - name: line_color
    type: string
  - name: palette
    type: enum
    values: [red, green, blue]
format:
  type: single
  width: 60
  height: 40
layout:
  - type: container
    at: [0, 0]
    size: [50, 30]
    background: "{brand}"
    stroke:
      thickness: 0.3
      color: "{brand}"
    items:
      - type: text
        value: "Inside"
        at: [5, 5]
        size: [40, 20]
        font_size: 10
        color: "{palette}"
  - type: line
    at: [0, 35]
    to: [50, 35]
    stroke:
      thickness: 0.5
      color: "{line_color}"
"#;
        let template = crate::parse::parse_template(yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };

        // 1. Scenario: A referenced colour renders on a shape and on a stroke (#c0392b and navy)
        let data = HashMap::from([
            ("brand".to_string(), serde_json::json!("#c0392b")),
            ("line_color".to_string(), serde_json::json!("navy")),
            ("palette".to_string(), serde_json::json!("green")),
        ]);
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(items, (60.0, 40.0), [true, true], &HashMap::new(), "layout")
            .unwrap();
        let src = ctx
            .render_items(items, &meas, (60.0, 40.0), &HashMap::new(), None, "layout")
            .unwrap();

        // Container background is #c0392b (#c0392bff), stroke is #c0392b (#c0392bff)
        assert!(
            src.contains("fill: rgb(\"#c0392bff\")"),
            "container fill must be #c0392b, got: {src}"
        );
        assert!(
            src.contains("stroke: 0.3mm + rgb(\"#c0392bff\")"),
            "container stroke must be #c0392b, got: {src}"
        );
        // Line stroke is navy (#000080ff)
        assert!(
            src.contains("stroke: 0.5mm + rgb(\"#000080ff\")"),
            "line stroke must be navy, got: {src}"
        );
        // Child text is green (#008000ff)
        assert!(
            src.contains("fill: rgb(\"#008000ff\")"),
            "text fill must be green, got: {src}"
        );

        // 2. Scenario: An enum parameter drives the colour on a container background (red, green, blue)
        let enum_yaml = r#"
name: EnumBg
unit: mm
dpi: 200
params:
  - name: palette
    type: enum
    values: [red, green, blue]
format: { type: single, width: 50, height: 30 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 30]
    background: "{palette}"
    items: []
"#;
        let template_enum = crate::parse::parse_template(enum_yaml).unwrap();
        let Layout::Items(items_enum) = &template_enum.layout;
        for (enum_val, expected_hex) in [
            ("red", "#ff0000ff"),
            ("green", "#008000ff"),
            ("blue", "#0000ffff"),
        ] {
            let data_enum = HashMap::from([("palette".to_string(), serde_json::json!(enum_val))]);
            let resolved_enum = super::resolve_parameters(
                &template_enum,
                &data_enum,
                Some(&settings),
                Some(&datetime),
            )
            .unwrap();
            let images_enum = std::cell::RefCell::new(super::ImageCollector::default());
            let ctx_enum =
                super::RenderContext::new("mm", 200, &resolved_enum.data, &env, &images_enum);
            let (meas_enum, _) = ctx_enum
                .measure_items(
                    items_enum,
                    (50.0, 30.0),
                    [true, true],
                    &HashMap::new(),
                    "layout",
                )
                .unwrap();
            let src_enum = ctx_enum
                .render_items(
                    items_enum,
                    &meas_enum,
                    (50.0, 30.0),
                    &HashMap::new(),
                    None,
                    "layout",
                )
                .unwrap();
            assert!(
                src_enum.contains(&format!("fill: rgb(\"{expected_hex}\")")),
                "enum value '{enum_val}' on container background must render {expected_hex}, got: {src_enum}"
            );
        }
    }

    #[test]
    fn cross_field_paint_equality_emitted_typst() {
        // 1. Text item with color: red inside container with background: red emits identical paint value
        let nested_yaml = r#"
name: NestedRed
unit: mm
dpi: 200
format: { type: single, width: 50, height: 30 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 30]
    background: red
    items:
      - type: text
        value: "Red On Red"
        at: [0, 0]
        size: [50, 30]
        font_size: 10
        color: red
"#;
        let template = crate::parse::parse_template(nested_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let data = HashMap::new();
        let geometry = HashMap::new();
        let ctx = super::RenderContext::new("mm", 200, &data, &env, &images);
        let (meas, _) = ctx
            .measure_items(items, (50.0, 30.0), [true, true], &geometry, "layout")
            .unwrap();
        let src = ctx
            .render_items(items, &meas, (50.0, 30.0), &geometry, None, "layout")
            .unwrap();

        // Both container #box and child #text emit exact same rgb("#ff0000ff")
        assert!(
            src.contains("#box(width: 50mm, height: 30mm, fill: rgb(\"#ff0000ff\")"),
            "container box must carry rgb(\"#ff0000ff\"), got: {src}"
        );
        assert!(
            src.contains("#text(size: 10pt, fill: rgb(\"#ff0000ff\"))"),
            "text must carry rgb(\"#ff0000ff\"), got: {src}"
        );

        // 2. CSS Level 1 colors vs rendering engine constants: red, green, gray, yellow
        for (name, expected_hex) in [
            ("red", "#ff0000ff"),
            ("green", "#008000ff"),
            ("gray", "#808080ff"),
            ("yellow", "#ffff00ff"),
        ] {
            let item = LayoutItem::Text {
                value: "Test".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: Some(DynamicValue::Literal(name.parse().unwrap())),
                wrap: false,
                line_spacing: None,
                alignment: Alignment::default(),
                overflow: Overflow::Ellipsis,
                when: None,
            };
            let src = render_test_items(&[item], (20.0, 10.0)).unwrap();
            assert!(
                src.contains(&format!("fill: rgb(\"{expected_hex}\")")),
                "name '{name}' must emit CSS value '{expected_hex}', got: {src}"
            );
        }
    }

    #[test]
    fn sheet_multi_slot_color_rendering() {
        let yaml = r#"
name: SheetColor
unit: mm
dpi: 200
format:
  type: sheet
  paper_width: 50
  paper_height: 50
  label_width: 20
  label_height: 20
  positions:
    - [0, 0]
    - [25, 0]
params:
  - name: bg
    type: string
  - name: stroke_col
    type: string
  - name: txt_col
    type: string
layout:
  - type: container
    at: [0, 0]
    size: [20, 20]
    background: "{bg}"
    stroke:
      thickness: 0.5
      color: "{stroke_col}"
    items:
      - type: text
        value: "Label"
        at: [0, 0]
        size: [20, 20]
        font_size: 8
        color: "{txt_col}"
"#;
        let template_content = crate::parse::parse_template(yaml).unwrap();
        let template = TemplateDefinition {
            id: "sheet_color".to_string(),
            group: None,
            content: template_content,
        };
        let Layout::Items(items) = &template.layout;
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };

        // Render slot 0 with bg: red, stroke: yellow, txt: white
        let data_slot0 = HashMap::from([
            ("bg".to_string(), serde_json::json!("red")),
            ("stroke_col".to_string(), serde_json::json!("yellow")),
            ("txt_col".to_string(), serde_json::json!("white")),
        ]);
        let resolved_slot0 =
            super::resolve_parameters(&template, &data_slot0, Some(&settings), Some(&datetime))
                .unwrap();
        let images_slot0 = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx_slot0 =
            super::RenderContext::new("mm", 200, &resolved_slot0.data, &env, &images_slot0);
        let (meas_slot0, _) = ctx_slot0
            .measure_items(items, (20.0, 20.0), [true, true], &HashMap::new(), "layout")
            .unwrap();
        let src_slot0 = ctx_slot0
            .render_items(
                items,
                &meas_slot0,
                (20.0, 20.0),
                &HashMap::new(),
                None,
                "layout",
            )
            .unwrap();

        // Render slot 1 with bg: navy, stroke: teal, txt: lime
        let data_slot1 = HashMap::from([
            ("bg".to_string(), serde_json::json!("navy")),
            ("stroke_col".to_string(), serde_json::json!("teal")),
            ("txt_col".to_string(), serde_json::json!("lime")),
        ]);
        let resolved_slot1 =
            super::resolve_parameters(&template, &data_slot1, Some(&settings), Some(&datetime))
                .unwrap();
        let images_slot1 = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx_slot1 =
            super::RenderContext::new("mm", 200, &resolved_slot1.data, &env, &images_slot1);
        let (meas_slot1, _) = ctx_slot1
            .measure_items(items, (20.0, 20.0), [true, true], &HashMap::new(), "layout")
            .unwrap();
        let src_slot1 = ctx_slot1
            .render_items(
                items,
                &meas_slot1,
                (20.0, 20.0),
                &HashMap::new(),
                None,
                "layout",
            )
            .unwrap();

        // Slot 0 carries red bg (#ff0000ff), yellow stroke (#ffff00ff), and white text (#ffffffff)
        assert!(
            src_slot0.contains("fill: rgb(\"#ff0000ff\")"),
            "slot 0 must carry red bg, got: {src_slot0}"
        );
        assert!(
            src_slot0.contains("stroke: 0.5mm + rgb(\"#ffff00ff\")"),
            "slot 0 must carry yellow stroke, got: {src_slot0}"
        );
        assert!(
            src_slot0.contains("fill: rgb(\"#ffffffff\")"),
            "slot 0 must carry white text, got: {src_slot0}"
        );

        // Slot 1 carries navy bg (#000080ff), teal stroke (#008080ff), and lime text (#00ff00ff)
        assert!(
            src_slot1.contains("fill: rgb(\"#000080ff\")"),
            "slot 1 must carry navy bg, got: {src_slot1}"
        );
        assert!(
            src_slot1.contains("stroke: 0.5mm + rgb(\"#008080ff\")"),
            "slot 1 must carry teal stroke, got: {src_slot1}"
        );
        assert!(
            src_slot1.contains("fill: rgb(\"#00ff00ff\")"),
            "slot 1 must carry lime text, got: {src_slot1}"
        );

        // And render_sheet_pages compiles the multi-slot sheet with painted containers and text to PDF
        let labels = vec![
            crate::models::LabelInput { data: data_slot0 },
            crate::models::LabelInput { data: data_slot1 },
        ];
        let pdf = super::render_sheet_pages(&template, &labels, 0, &settings, &datetime).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn list_join_emits_joined_text_in_typst_source() {
        let text_item = LayoutItem::Text {
            value: "{tags:join(', ')}".to_string(),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::fixed(50.0), SizeValue::fixed(20.0)]),
            ),
            font_size: FontSize::Fixed(12.0),
            font_weight: None,
            color: None,
            wrap: false,
            line_spacing: None,
            alignment: crate::models::Alignment {
                horizontal: crate::models::HorizontalAlign::Left,
                vertical: crate::models::VerticalAlign::Top,
            },
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let mut data: HashMap<String, super::JsonValue> = HashMap::new();
        data.insert("tags".to_string(), serde_json::json!(["A", "B"]));
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 180, &data, &env, &images);
        let items = vec![text_item];
        let geometry_values = HashMap::new();
        let (measured, _) = ctx
            .measure_items(
                &items,
                (100.0, 50.0),
                [true, true],
                &geometry_values,
                "layout",
            )
            .expect("measure items");
        let src = ctx
            .render_items(
                &items,
                &measured,
                (100.0, 50.0),
                &geometry_values,
                None,
                "layout",
            )
            .expect("render items");
        assert!(
            src.contains("A,\u{a0}B") || src.contains("A, B"),
            "rendered Typst source should contain joined list text 'A, B': {src}"
        );
    }

    #[test]
    fn param_types_refuse_array_values_with_exact_codes_and_reasons() {
        let array_val = serde_json::json!(["foo", "bar"]);
        let variables = BTreeMap::new();
        let datetime_ctx = no_datetime();

        let tpl_with_param =
            |name: &str, param_type: crate::models::ParamType| -> TemplateContent {
                let mut params = IndexMap::new();
                params.insert(
                    name.to_string(),
                    crate::models::ParamSpec {
                        param_type,
                        description: None,
                        default: None,
                        min: None,
                        max: None,
                    },
                );
                TemplateContent {
                    name: "Test".to_string(),
                    version: None,
                    description: String::new(),
                    unit: "mm".to_string(),
                    dpi: 200,
                    format: crate::models::TemplateFormat::Single {
                        width: crate::models::Dimension::Fixed(50.0).into(),
                        height: crate::models::Dimension::Fixed(20.0).into(),
                        media_width: None,
                    },
                    params,
                    layout: crate::models::Layout::Items(vec![]),
                }
            };

        let run_strict = |name: &str,
                          param_type: crate::models::ParamType|
         -> Result<super::ResolvedParams, AppError> {
            let tpl = tpl_with_param(name, param_type);
            let mut submitted = HashMap::new();
            submitted.insert(name.to_string(), array_val.clone());
            super::resolve_parameters_mode(
                &tpl,
                &submitted,
                Some(&variables),
                Some(&datetime_ctx),
                super::ResolveMode::Strict,
            )
        };

        // 1. String
        let err = run_strict(
            "title",
            crate::models::ParamType::String { multiline: false },
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(
            err.message_text(),
            "parameter 'title' is not a valid string"
        );

        // 2. Boolean
        let err = run_strict("flag", crate::models::ParamType::Boolean).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(
            err.message_text(),
            "parameter 'flag' is not a valid boolean"
        );

        // 3. Integer
        let err = run_strict("count", crate::models::ParamType::Integer).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(
            err.message_text(),
            "parameter 'count' is not a valid integer"
        );

        // 4. Number & Length
        let err = run_strict("ratio", crate::models::ParamType::Number).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(
            err.message_text(),
            "parameter 'ratio' is not a valid number"
        );

        let err = run_strict("size", crate::models::ParamType::Length).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(err.message_text(), "parameter 'size' is not a valid number");

        // 5. Enum (422 Unprocessable Entity, InvalidEnumValue)
        let err = run_strict(
            "tier",
            crate::models::ParamType::Enum {
                values: vec!["A".to_string(), "B".to_string()],
            },
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 422);
        assert_eq!(err.code(), "InvalidEnumValue");
        assert_eq!(err.reason(), None); // invalid_enum_value uses unreasoned new()
        assert_eq!(err.message_text(), "Invalid option selection");
        let details = err.details().expect("details");
        assert_eq!(details["selection"]["tier"], "[\"foo\",\"bar\"]");
        assert_eq!(details["allowed"]["tier"], serde_json::json!(["A", "B"]));
        assert!(details.get("reason").is_none(), "must carry no reason");
        assert_eq!(
            details.as_object().unwrap().len(),
            2,
            "details must be exactly selection+allowed"
        );

        // 6. Datetime
        let err = run_strict(
            "created_at",
            crate::models::ParamType::Datetime { time: false },
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("datetime_param_invalid"));
        assert_eq!(
            err.message_text(),
            "Invalid value for datetime parameter 'created_at': [\"foo\",\"bar\"]"
        );

        // 7. List with non-array value
        let tpl_list = tpl_with_param("tags", crate::models::ParamType::List);
        let mut submitted_str = HashMap::new();
        submitted_str.insert("tags".to_string(), serde_json::json!("not_an_array"));
        let err = super::resolve_parameters_mode(
            &tpl_list,
            &submitted_str,
            Some(&variables),
            Some(&datetime_ctx),
            super::ResolveMode::Strict,
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(err.message_text(), "parameter 'tags' is not a valid list");

        // 8. List with non-string element
        let mut submitted_bad_elem = HashMap::new();
        submitted_bad_elem.insert("tags".to_string(), serde_json::json!(["ok", 42]));
        let err = super::resolve_parameters_mode(
            &tpl_list,
            &submitted_bad_elem,
            Some(&variables),
            Some(&datetime_ctx),
            super::ResolveMode::Strict,
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("request_body_invalid"));
        assert_eq!(
            err.message_text(),
            "element at position 1 of parameter 'tags' must be a string"
        );
    }

    #[test]
    fn invalid_enum_value_pins_spec_selection_and_allowed() {
        let orientation_param = crate::models::ParamType::Enum {
            values: vec!["horizontal".to_string(), "vertical".to_string()],
        };
        let mut params = IndexMap::new();
        params.insert(
            "orientation".to_string(),
            crate::models::ParamSpec {
                param_type: orientation_param,
                description: None,
                default: None,
                min: None,
                max: None,
            },
        );
        let template = TemplateContent {
            name: "Test".to_string(),
            version: None,
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: crate::models::TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(50.0).into(),
                height: crate::models::Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params,
            layout: crate::models::Layout::Items(vec![]),
        };
        let mut submitted = HashMap::new();
        submitted.insert("orientation".to_string(), serde_json::json!("sideways"));
        let variables = BTreeMap::new();
        let datetime_ctx = no_datetime();
        let err = super::resolve_parameters_mode(
            &template,
            &submitted,
            Some(&variables),
            Some(&datetime_ctx),
            super::ResolveMode::Strict,
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 422);
        assert_eq!(err.code(), "InvalidEnumValue");
        assert_eq!(err.reason(), None);
        assert_eq!(err.message_text(), "Invalid option selection");
        let details = err.details().expect("details");
        assert_eq!(details["selection"]["orientation"], "sideways");
        assert_eq!(
            details["allowed"]["orientation"],
            serde_json::json!(["horizontal", "vertical"])
        );
        assert!(details.get("reason").is_none(), "must carry no reason");
        assert_eq!(
            details.as_object().unwrap().len(),
            2,
            "details must be exactly selection and allowed"
        );
        // Verify byte-identical keys: only selection and allowed present
        let keys: std::collections::BTreeSet<&String> =
            details.as_object().unwrap().keys().collect();
        assert_eq!(
            keys.into_iter().cloned().collect::<Vec<_>>(),
            vec!["allowed".to_string(), "selection".to_string()]
        );
    }

    #[test]
    fn test_unknown_param_names_and_validate_label_data_keys() {
        use crate::templates::TemplateDefinition;

        let yaml = r#"
name: Shelf Label Display Name
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
params:
  - name: title
    type: string
  - name: unused_param
    type: string
layout:
  - type: text
    value: "{title}"
    font_size: 10
    at: [0, 0]
    size: [50, 10]
"#;
        let template_content = crate::parse::parse_template(yaml).unwrap();
        let template = TemplateDefinition {
            id: "shelf".to_string(),
            group: None,
            content: template_content,
        };

        // 1. Empty map passes
        let empty_data = HashMap::new();
        assert!(super::validate_label_data_keys(&template, &empty_data).is_ok());

        // 2. All declared keys pass, including unused_param which layout does not read
        let valid_data = HashMap::from([
            ("title".to_string(), serde_json::json!("Bolts")),
            ("unused_param".to_string(), serde_json::json!("Extra")),
        ]);
        assert!(super::validate_label_data_keys(&template, &valid_data).is_ok());

        // 3. unknown_param_names unit checks
        let unknown =
            super::unknown_param_names(&template, ["zeta", "title", "alpha", "mid"].into_iter());
        assert_eq!(unknown, vec!["alpha", "mid", "zeta"]);

        // 4. Multiple unrecognized keys in data map produce one DataKeyUnknown error naming all sorted keys and template.id
        let bad_data = HashMap::from([
            ("zeta".to_string(), serde_json::json!("z")),
            ("alpha".to_string(), serde_json::json!("a")),
            ("title".to_string(), serde_json::json!("Bolts")),
            ("mid".to_string(), serde_json::json!("m")),
        ]);
        let err = super::validate_label_data_keys(&template, &bad_data).unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("data_key_unknown"));
        let msg = err.message_text();
        assert!(
            msg.contains("'alpha', 'mid', 'zeta'"),
            "msg should contain sorted keys: {msg}"
        );
        assert!(
            msg.contains("'shelf'"),
            "msg should contain template id 'shelf': {msg}"
        );
        assert!(
            !msg.contains("Shelf Label Display Name"),
            "msg should not contain display name"
        );

        // 5. Order is consistent across repeated runs regardless of map insertion order
        for perm in [
            ["alpha", "mid", "zeta"],
            ["zeta", "mid", "alpha"],
            ["mid", "alpha", "zeta"],
            ["zeta", "alpha", "mid"],
        ] {
            let mut map = HashMap::new();
            map.insert("title".to_string(), serde_json::json!("Bolts"));
            for k in perm {
                map.insert(k.to_string(), serde_json::json!("v"));
            }
            let err_rep = super::validate_label_data_keys(&template, &map).unwrap_err();
            assert_eq!(err_rep.message_text(), msg);
        }
    }

    #[test]
    fn repeating_container_drawn_geometry_sizes_each_instance_to_its_own_element() {
        // 4.8: Test that each instance is sized on its own, by rendering three elements of different lengths
        // into size: [content, content] instances and asserting each instance's drawn geometry rather than
        // that a PNG came back.
        let rep_yaml = r##"
name: RepAutoSizingGeometry
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: row, gap: 2 }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        background: "#eeeeee"
        items:
          - type: text
            value: "{tags}"
            size: [content, content]
            font_size: 8
"##;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::from([(
            "tags".to_string(),
            serde_json::json!(["A", "Medium tag", "A very substantially longer tag text"]),
        )]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();

        // Check measured children of the outer container
        let outer_children = &meas[0].children;
        assert_eq!(outer_children.len(), 3, "must expand 3 measured instances");
        let w0 = outer_children[0].intrinsic[0].expect("intrinsic width 0");
        let w1 = outer_children[1].intrinsic[0].expect("intrinsic width 1");
        let w2 = outer_children[2].intrinsic[0].expect("intrinsic width 2");

        assert!(w0 > 0.0, "w0 must be positive");
        assert!(w1 > w0, "w1 ({w1}) must be wider than w0 ({w0})");
        assert!(w2 > w1, "w2 ({w2}) must be wider than w1 ({w1})");

        let typst_src = ctx
            .render_items(
                items,
                &meas,
                (100.0, 100.0),
                &HashMap::new(),
                None,
                "layout",
            )
            .unwrap();
        // Assert that the generated Typst placement geometry contains distinct widths for the 3 instances
        let box_w0 = super::helpers::format_length(w0, "mm").unwrap();
        let box_w1 = super::helpers::format_length(w1, "mm").unwrap();
        let box_w2 = super::helpers::format_length(w2, "mm").unwrap();
        assert!(typst_src.contains(&format!("width: {box_w0}")));
        assert!(typst_src.contains(&format!("width: {box_w1}")));
        assert!(typst_src.contains(&format!("width: {box_w2}")));
    }

    #[test]
    fn repeating_container_rendered_order_and_siblings() {
        // 4.7: Three elements drawn in request order; siblings keep their places before and after the instances
        let rep_yaml = r#"
name: RepOrder
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: text
        value: "PRE"
        size: [content, content]
        font_size: 8
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
      - type: text
        value: "POST"
        size: [content, content]
        font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::from([(
            "tags".to_string(),
            serde_json::json!(["First", "Second", "Third"]),
        )]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();
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

        let pos_pre = src.find("PRE").expect("PRE in Typst");
        let pos_first = src.find("First").expect("First in Typst");
        let pos_second = src.find("Second").expect("Second in Typst");
        let pos_third = src.find("Third").expect("Third in Typst");
        let pos_post = src.find("POST").expect("POST in Typst");

        assert!(
            pos_pre < pos_first
                && pos_first < pos_second
                && pos_second < pos_third
                && pos_third < pos_post,
            "Typst markup must preserve authored order of instances and siblings"
        );
    }

    #[test]
    fn repeating_container_scoped_tokens_and_joined_outside() {
        // 4.11: Scoped token replacement, joined token outside, per-instance when:
        let rep_yaml = r#"
name: RepScope
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: cats
    type: list
  - name: items
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: text
        value: "All: {cats:join('+')}"
        size: [content, content]
        font_size: 8
      - type: container
        repeat: cats
        size: [content, content]
        flow: { direction: column }
        items:
          - type: container
            repeat: items
            size: [content, content]
            flow: { direction: column }
            items:
              - type: text
                value: "{cats}: {items}"
                size: [content, content]
                font_size: 8
              - type: text
                when:
                  items: Apple
                value: "(FAVORITE)"
                size: [content, content]
                font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::from([
            ("cats".to_string(), serde_json::json!(["Fruit", "Veg"])),
            (
                "items".to_string(),
                serde_json::json!(["Apple", "Broccoli"]),
            ),
        ]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();
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
            src.contains("Fruit+Veg"),
            "outside joined token must be expanded as joined list"
        );

        let p1 = src
            .find("Fruit:\u{a0}Apple")
            .or_else(|| src.find("Fruit: Apple"))
            .expect("Fruit: Apple in Typst");
        let p_fav1 = src
            .find("(FAVORITE)")
            .expect("FAVORITE for Apple 1 in Typst");
        let p2 = src
            .find("Fruit:\u{a0}Broccoli")
            .or_else(|| src.find("Fruit: Broccoli"))
            .expect("Fruit: Broccoli in Typst");
        let p3 = src
            .find("Veg:\u{a0}Apple")
            .or_else(|| src.find("Veg: Apple"))
            .expect("Veg: Apple in Typst");
        let p_fav2 = src
            .rfind("(FAVORITE)")
            .expect("FAVORITE for Apple 2 in Typst");
        let p4 = src
            .find("Veg:\u{a0}Broccoli")
            .or_else(|| src.find("Veg: Broccoli"))
            .expect("Veg: Broccoli in Typst");

        assert!(
            p1 < p_fav1 && p_fav1 < p2 && p2 < p3 && p3 < p_fav2 && p_fav2 < p4,
            "nested combinations and when conditions must be rendered in sequence"
        );
    }

    #[test]
    fn repeating_container_empty_list_and_default_empty_draw_no_instances() {
        // 4.7: [] and default: [] drawing the strip with no instances and no error
        let rep_yaml = r#"
name: RepEmpty
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: tags
    type: list
    default: []
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: text
        value: "PRE"
        size: [content, content]
        font_size: 8
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
      - type: text
        value: "POST"
        size: [content, content]
        font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };

        // 1. Over explicit empty list []
        let data_empty = HashMap::from([("tags".to_string(), serde_json::json!([]))]);
        let resolved_empty =
            super::resolve_parameters(&template, &data_empty, Some(&settings), Some(&datetime))
                .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved_empty.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();
        // Children of root container are PRE and POST (0 repeat instances)
        assert_eq!(meas[0].children.len(), 2, "must measure only 2 siblings");
        let src_empty = ctx
            .render_items(
                items,
                &meas,
                (100.0, 100.0),
                &HashMap::new(),
                None,
                "layout",
            )
            .unwrap();
        assert!(src_empty.contains("PRE"));
        assert!(src_empty.contains("POST"));
        assert!(!src_empty.contains("Tag:"));

        // 2. Over omitted data using declared default: []
        let data_omitted = HashMap::new();
        let resolved_def =
            super::resolve_parameters(&template, &data_omitted, Some(&settings), Some(&datetime))
                .unwrap();
        let images_def = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx_def = super::RenderContext::new("mm", 200, &resolved_def.data, &env, &images_def);
        let (meas_def, _) = ctx_def
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();
        assert_eq!(
            meas_def[0].children.len(),
            2,
            "must measure only 2 siblings"
        );
        let src_def = ctx_def
            .render_items(
                items,
                &meas_def,
                (100.0, 100.0),
                &HashMap::new(),
                None,
                "layout",
            )
            .unwrap();
        assert!(src_def.contains("PRE"));
        assert!(src_def.contains("POST"));
        assert!(!src_def.contains("Tag:"));
    }

    #[test]
    fn repeating_container_declared_default_draws_elements() {
        // 4.7: Declared default: supplying elements renders when data is omitted
        let rep_yaml = r#"
name: RepDefault
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: tags
    type: list
    default: ["CONSUMABLE", "KIDS"]
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
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
            2,
            "must measure 2 default instances"
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
        let p_cons = src.find("CONSUMABLE").expect("CONSUMABLE in Typst");
        let p_kids = src.find("KIDS").expect("KIDS in Typst");
        assert!(
            p_cons < p_kids,
            "default instances must be drawn in declared order"
        );
    }

    #[test]
    fn repeating_container_overflow_trim_draws_first_two_instances() {
        // 4.9: Container under overflow: trim draws the first two and succeeds
        let rep_yaml = r#"
name: RepTrim
unit: mm
dpi: 200
format: { type: single, width: 50, height: 25 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 25]
    flow: { direction: column, overflow: trim }
    items:
      - type: container
        repeat: tags
        size: [50, 10]
        items:
          - type: text
            at: [0, 0]
            value: "Tag: {tags}"
            size: [10, 5]
            font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::from([("tags".to_string(), serde_json::json!(["A", "B", "C"]))]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(items, (50.0, 25.0), [true, true], &HashMap::new(), "layout")
            .unwrap();
        let src = ctx
            .render_items(items, &meas, (50.0, 25.0), &HashMap::new(), None, "layout")
            .unwrap();
        let p_a = src
            .find("Tag:\u{a0}A")
            .or_else(|| src.find("Tag: A"))
            .expect("Tag: A in Typst");
        let p_b = src
            .find("Tag:\u{a0}B")
            .or_else(|| src.find("Tag: B"))
            .expect("Tag: B in Typst");
        assert!(p_a < p_b, "Tag A must appear before Tag B");
        assert!(
            !src.contains("Tag:\u{a0}C") && !src.contains("Tag: C"),
            "Tag C must be trimmed"
        );
    }

    #[test]
    fn repeating_container_when_gate_evaluated_once_draws_all_instances() {
        // 4.7: Repeating container gated by when: on an outer parameter:
        // when matching, the gate is evaluated once and both instances are drawn.
        let rep_yaml = r#"
name: RepWhenGate
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: show_tags
    type: enum
    values: ["yes", "no"]
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        when:
          show_tags: "yes"
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::from([
            ("show_tags".to_string(), serde_json::json!("yes")),
            ("tags".to_string(), serde_json::json!(["normal", "special"])),
        ]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(
                items,
                (100.0, 100.0),
                [true, true],
                &HashMap::new(),
                "layout",
            )
            .unwrap();
        assert_eq!(meas[0].children.len(), 2, "must measure 2 repeat instances");
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
        let p1 = src
            .find("Tag:\u{a0}normal")
            .or_else(|| src.find("Tag: normal"))
            .expect("Tag: normal in Typst");
        let p2 = src
            .find("Tag:\u{a0}special")
            .or_else(|| src.find("Tag: special"))
            .expect("Tag: special in Typst");
        assert!(p1 < p2, "both instances must be drawn in order");
    }

    #[test]
    fn repeating_container_wrap_places_third_instance_on_second_line() {
        // 4.9: Repeating container under flow wrap: true wraps overflow onto a second line
        let rep_yaml = r#"
name: RepWrap
unit: mm
dpi: 200
format: { type: single, width: 25, height: 50 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [25, 50]
    flow: { direction: row, wrap: true }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        items:
          - type: text
            at: [0, 0]
            value: "Tag: {tags}"
            size: [10, 5]
            font_size: 8
"#;
        let template = crate::parse::parse_template(rep_yaml).unwrap();
        let Layout::Items(items) = &template.layout;
        let data = HashMap::from([("tags".to_string(), serde_json::json!(["A", "B", "C"]))]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let resolved =
            super::resolve_parameters(&template, &data, Some(&settings), Some(&datetime)).unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", 200, &resolved.data, &env, &images);
        let (meas, _) = ctx
            .measure_items(items, (25.0, 50.0), [true, true], &HashMap::new(), "layout")
            .unwrap();
        assert_eq!(meas[0].children.len(), 3, "must measure 3 instances");
        let src = ctx
            .render_items(items, &meas, (25.0, 50.0), &HashMap::new(), None, "layout")
            .unwrap();
        // In a row of width 25, instances 0 and 1 take 10 mm each (dx: 0mm, dy: 0mm and dx: 10mm, dy: 0mm).
        // Instance 2 overflows width 25 and wraps to line 2 (dx: 0mm, dy: 10mm).
        assert!(src.contains("dx: 0mm, dy: 0mm"));
        assert!(src.contains("dx: 10mm, dy: 0mm"));
        assert!(
            src.contains("dx: 0mm, dy: 10mm"),
            "third instance must wrap to dy: 10mm on second line"
        );
        let p_a = src
            .find("Tag:\u{a0}A")
            .or_else(|| src.find("Tag: A"))
            .expect("Tag: A in Typst");
        let p_b = src
            .find("Tag:\u{a0}B")
            .or_else(|| src.find("Tag: B"))
            .expect("Tag: B in Typst");
        let p_c = src
            .find("Tag:\u{a0}C")
            .or_else(|| src.find("Tag: C"))
            .expect("Tag: C in Typst");
        assert!(p_a < p_b && p_b < p_c, "instances must be drawn in order");
    }

    #[test]
    fn issue_360_render_coercion_multi_error_surfaces_first_declared() {
        let yaml = r#"
name: CoercionOrder
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: zebra
    type: integer
  - name: alpha
    type: integer
layout:
  - type: text
    value: "{zebra} {alpha}"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#;
        let template = crate::parse::parse_template(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("zebra".to_string(), serde_json::json!("not_an_int"));
        data.insert("alpha".to_string(), serde_json::json!("also_not_an_int"));
        let dt = no_datetime();
        let settings = no_settings();
        let err = render_single_label(&template, &data, &settings, &dt).unwrap_err();
        assert!(
            err.message_text().contains("zebra"),
            "expected render coercion error to surface zebra first in declaration order, got: {}",
            err.message_text()
        );
        assert!(
            !err.message_text().contains("alpha"),
            "alpha should not be reported before zebra: {}",
            err.message_text()
        );
    }
}
