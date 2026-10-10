mod helpers;

pub const MAX_RENDER_DPI: u32 = 1200;

use crate::errors::AppError;
use crate::models::{
    resolve_coord, DynamicDimension, DynamicValue, Fit, LabelInput, Layout, LayoutItem, ParamSpec,
    ParamType, ParamValue, Placement, Point, Position, Rotation, Shape, Stroke, TemplateFormat,
};
use crate::reason::Reason;
use crate::templates::{TemplateContent, TemplateDefinition};
use chrono::{DateTime, Local};
use helpers::{
    assets_root, binarize_rgba, build_qr_svg, escape_typst_string, format_length, interpolate,
    parse_image_data_uri, resolve_dimension, resolve_dynamic_value_f32, resolve_image_asset,
    to_page_coords, typst_alignment, typst_font_options,
};

pub(crate) use helpers::value_to_string;
use serde_json::Value as JsonValue;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

#[derive(Debug, Clone)]
pub struct ResolvedParams {
    pub data: HashMap<String, JsonValue>,
    pub instants: BTreeMap<String, DateTime<Local>>,
}

/// A value the supplied-value rule accepts: what `{p}` prints and a `when:` compares, plus a
/// `datetime`'s instant.
#[derive(Debug, Clone, PartialEq)]
pub struct Coerced {
    pub value: ParamValue,
    pub instant: Option<DateTime<Local>>,
}

/// Why the supplied-value rule refuses a value. The message describes the value, not the
/// parameter, so each caller names where it was read; `element` is a `list` element's position.
#[derive(Debug)]
pub(crate) struct Refusal {
    pub message: String,
    pub element: Option<usize>,
}

/// Each tokened default of one template, resolved against one request's snapshot.
pub type ResolvedDefaults = BTreeMap<String, Coerced>;

/// 2^63: every whole `f64` in `[-2^63, 2^63)` is an `i64`.
const I64_BOUND: f64 = 9_223_372_036_854_775_808.0;

/// The supplied-value rule (`parameters`, "Supplied values are coerced by type"). `null`, and a
/// blank string for every type but `string`, is an omission (`Ok(None)`). Anything else is the
/// type's accepted form within the declared `min` and `max`, or refused.
pub(crate) fn coerce_param_value(
    val: &JsonValue,
    spec: &ParamSpec,
) -> Result<Option<Coerced>, Refusal> {
    let is_string = matches!(spec.param_type, ParamType::String { .. });
    match val {
        JsonValue::Null => return Ok(None),
        JsonValue::String(s) if !is_string && s.trim().is_empty() => return Ok(None),
        _ => {}
    }
    let refuse = |message: String| Refusal {
        message,
        element: None,
    };
    let not_a = |what: &str| refuse(format!("{val} is not {what}"));
    let out_of_bounds = || {
        let bound = |b: Option<f64>| b.map_or_else(|| "none".to_string(), |b| b.to_string());
        refuse(format!(
            "{val} is outside min {} and max {}",
            bound(spec.min),
            bound(spec.max)
        ))
    };
    let value = match &spec.param_type {
        ParamType::String { .. } => match val {
            JsonValue::String(s) => ParamValue::String(s.clone()),
            _ => return Err(not_a("a string")),
        },
        ParamType::Number => {
            let number = match val {
                JsonValue::Number(n) => n.as_f64(),
                JsonValue::String(s) => s.trim().parse::<f64>().ok(),
                _ => None,
            }
            .filter(|f| f.is_finite())
            .ok_or_else(|| not_a("a number"))?;
            if spec.min.is_some_and(|min| number < min) || spec.max.is_some_and(|max| number > max)
            {
                return Err(out_of_bounds());
            }
            ParamValue::Float(number)
        }
        ParamType::Integer => {
            let integer = match val {
                JsonValue::Number(n) => n.as_i64().or_else(|| {
                    n.as_f64()
                        .filter(|f| f.fract() == 0.0 && (-I64_BOUND..I64_BOUND).contains(f))
                        .map(|f| f as i64)
                }),
                JsonValue::String(s) => s.trim().parse::<i64>().ok(),
                _ => None,
            }
            .ok_or_else(|| not_a("an integer"))?;
            // Compared in i128 against the whole numbers the bounds admit: casting the integer to
            // f64 instead would admit 2^53 + 1 at `max: 2^53`.
            let wide = i128::from(integer);
            if spec.min.is_some_and(|min| wide < min.ceil() as i128)
                || spec.max.is_some_and(|max| wide > max.floor() as i128)
            {
                return Err(out_of_bounds());
            }
            ParamValue::Integer(integer)
        }
        ParamType::Boolean => {
            let boolean = match val {
                JsonValue::Bool(b) => Some(*b),
                JsonValue::String(s) => match s.trim() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => None,
                },
                JsonValue::Number(n) => match n.as_f64() {
                    Some(1.0) => Some(true),
                    Some(0.0) => Some(false),
                    _ => None,
                },
                _ => None,
            }
            .ok_or_else(|| not_a("a boolean"))?;
            ParamValue::Boolean(boolean)
        }
        ParamType::Enum { values } => match val {
            JsonValue::String(s) if values.contains(s) => ParamValue::String(s.clone()),
            _ => {
                return Err(refuse(format!(
                    "{val} is not one of the values: {}",
                    values.join(", ")
                )))
            }
        },
        ParamType::Datetime { .. } => {
            let JsonValue::String(s) = val else {
                return Err(not_a("a datetime string"));
            };
            let instant = crate::datetime_fmt::parse_datetime_override(s.trim())
                .map_err(|err| refuse(format!("{val} is not a datetime: {err}")))?;
            return Ok(Some(Coerced {
                value: ParamValue::String(crate::datetime_fmt::format_now(
                    crate::datetime_fmt::BARE_DATETIME_FORMAT,
                    instant,
                )),
                instant: Some(instant),
            }));
        }
        ParamType::List => {
            let JsonValue::Array(items) = val else {
                return Err(not_a("a list of strings"));
            };
            let mut strings = Vec::with_capacity(items.len());
            for (idx, item) in items.iter().enumerate() {
                match item {
                    JsonValue::String(s) => strings.push(s.clone()),
                    _ => {
                        return Err(Refusal {
                            message: format!("element at position {idx} is not a string: {item}"),
                            element: Some(idx),
                        })
                    }
                }
            }
            ParamValue::List(strings)
        }
    };
    Ok(Some(Coerced {
        value,
        instant: None,
    }))
}

/// [`coerce_param_value`], then for a parameter some `font_weight` reads the weight rule
/// (`parameters`, "Parameter references from layout attributes").
fn coerce_declared(
    val: &JsonValue,
    spec: &ParamSpec,
    reads_weight: bool,
) -> Result<Option<Coerced>, Refusal> {
    let coerced = coerce_param_value(val, spec)?;
    if let Some(Coerced {
        value: ParamValue::Integer(weight),
        ..
    }) = &coerced
    {
        if reads_weight && !crate::models::font_weight_ok(*weight) {
            return Err(Refusal {
                message: format!(
                    "{weight} is not a font weight: a multiple of 100 between 100 and 900"
                ),
                element: None,
            });
        }
    }
    Ok(coerced)
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

/// Each declared parameter's value: the label's value coerced by its type, else its default.
/// Every supplied value is coerced before any `when:` is evaluated. `defaults` holds the tokened
/// defaults `resolve_environment` resolved for this request.
pub fn resolve_parameters(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    defaults: &ResolvedDefaults,
) -> Result<ResolvedParams, AppError> {
    let mut resolved = data.clone();
    let mut instants = BTreeMap::new();
    let weight_params = template.weight_params();

    for (name, spec) in &template.params {
        let supplied = match data.get(name) {
            Some(val) => coerce_declared(val, spec, weight_params.contains(name.as_str()))
                .map_err(|refusal| {
                    AppError::param_value_invalid(
                        name,
                        refusal.element,
                        format!("invalid value for parameter '{name}': {}", refusal.message),
                    )
                })?,
            None => None,
        };
        let value = match supplied {
            Some(value) => Some(value),
            None => declared_default(name, spec, defaults)?,
        };
        match value {
            Some(Coerced { value, instant }) => {
                resolved.insert(name.clone(), JsonValue::from(&value));
                if let Some(instant) = instant {
                    instants.insert(name.clone(), instant);
                }
            }
            None => {
                resolved.remove(name);
            }
        }
    }

    Ok(ResolvedParams {
        data: resolved,
        instants,
    })
}

/// The value an omitted parameter takes: a literal default as judged at load, a tokened default as
/// resolved for this request, and `false` for a `boolean` that declares none.
fn declared_default(
    name: &str,
    spec: &ParamSpec,
    defaults: &ResolvedDefaults,
) -> Result<Option<Coerced>, AppError> {
    if spec.tokened_default().is_some() {
        return defaults.get(name).cloned().map(Some).ok_or_else(|| {
            AppError::internal(format!(
                "the tokened default of parameter '{name}' was not resolved for this request"
            ))
        });
    }
    Ok(match &spec.default {
        Some(value) => Some(Coerced {
            value: value.clone(),
            instant: spec.default_instant,
        }),
        None if spec.param_type == ParamType::Boolean => Some(Coerced {
            value: ParamValue::Boolean(false),
            instant: None,
        }),
        None => None,
    })
}

/// Resolve everything in `template` that does not come from a label against one request's
/// snapshot (`interpolation`, "One snapshot per request"): every `vars` key and datetime format
/// name in an interpolated string, and every tokened default, whose value must pass its type's
/// supplied-value rule. Any failure fails the whole request as `reference_unresolved`.
pub fn resolve_environment<'a>(
    template: &TemplateContent,
    settings: &'a BTreeMap<String, String>,
    datetime: &'a crate::datetime_fmt::DateTimeResolver<'a>,
) -> Result<RenderEnv<'a>, AppError> {
    for (path, text) in template.interpolated_strings() {
        for scanned in crate::interpolation::scan_tokens(text) {
            // Token syntax is judged at load.
            let Ok(token) = crate::interpolation::parse(scanned.raw) else {
                continue;
            };
            if let crate::interpolation::Source::Vars(key) = token.source {
                if !settings.contains_key(key) {
                    return Err(AppError::reference_unresolved(
                        &format!("vars.{key}"),
                        format!("{path} reads variable '{key}', which is not set"),
                    ));
                }
            }
            if let Some(crate::interpolation::Reader::Format(format)) = token.reader {
                if !datetime.formats.contains_key(format) {
                    return Err(AppError::reference_unresolved(
                        format,
                        format!(
                            "{path} uses datetime format '{format}', which the datetime_formats setting does not define"
                        ),
                    ));
                }
            }
        }
    }

    let mut defaults = ResolvedDefaults::new();
    let weight_params = template.weight_params();
    for (name, spec) in &template.params {
        let Some(text) = spec.tokened_default() else {
            continue;
        };
        let value = interpolate(text, &HashMap::new(), settings, datetime, None)?;
        let reads_weight = weight_params.contains(name.as_str());
        let why = match coerce_declared(&JsonValue::String(value.clone()), spec, reads_weight) {
            Ok(Some(coerced)) => {
                defaults.insert(name.clone(), coerced);
                continue;
            }
            Ok(None) => "the value is blank".to_string(),
            Err(refusal) => refusal.message,
        };
        return Err(AppError::reference_unresolved(
            name,
            format!("params.{name}.default resolves to '{value}', which parameter '{name}' refuses: {why}"),
        ));
    }

    Ok(RenderEnv {
        settings,
        datetime,
        defaults,
    })
}

/// Largest resolved label dimension (errors spec, `dimension_exceeds_limit`).
const MAX_LABEL_DIMENSION_MM: f32 = 1000.0;

fn check_dimension_limit(val: f32, unit: &str, label: &str) -> Result<(), AppError> {
    let val_mm = if unit == "in" { val * 25.4 } else { val };
    if !val_mm.is_finite() || val_mm <= 0.0 || val_mm > MAX_LABEL_DIMENSION_MM {
        return Err(AppError::unsupported_layout_item(
            Reason::DimensionExceedsLimit,
            format!("{label} {val} {unit} exceeds limit of {MAX_LABEL_DIMENSION_MM} mm"),
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
/// so placement needs no inset (#245).
fn pad_block(body: &str, pad: f32, vertical: crate::models::VerticalAlign) -> String {
    use crate::models::VerticalAlign;
    if pad <= 0.0 {
        return body.to_string();
    }
    match vertical {
        VerticalAlign::Top => format!("#pad(top: {pad}pt)[{body}]"),
        VerticalAlign::Bottom => format!("#pad(bottom: {pad}pt)[{body}]"),
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
    warned
        .output
        .map_err(|err| AppError::internal(format!("typst compile failed: {err}")))
}

fn compile_single_doc(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
) -> Result<PagedDocument, AppError> {
    if !matches!(template.format, TemplateFormat::Single { .. }) {
        return Err(AppError::invalid_request(
            Reason::FormatUnsupported,
            "this endpoint renders single templates only; render a sheet as a batch",
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
    let resolved = resolve_parameters(template, data, &env.defaults)?;
    let resolved_data = &resolved.data;
    let items = select_layout_items(template)?;
    let images = RefCell::new(ImageCollector::default());

    // Resolve initial width/height; Dynamic single may be overridden after measurement.
    let (mut width_units, height_units) = match &template.format {
        TemplateFormat::Single { width, height, .. } => (
            resolve_dimension(width, resolved_data)?,
            resolve_dynamic_value_f32(height, resolved_data)?,
        ),
        TemplateFormat::Sheet {
            label_width,
            label_height,
            ..
        } => (*label_width, *label_height),
    };

    check_dimension_limit(height_units, unit, "height")?;

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
            .ok_or_else(|| AppError::internal("dynamic single width requires max"))?;
        let min_w = min
            .as_ref()
            .map(|v| resolve_dynamic_value_f32(v, resolved_data))
            .transpose()?
            .ok_or_else(|| AppError::internal("dynamic single width requires min"))?;

        check_dimension_limit(min_w, unit, "width min")?;
        check_dimension_limit(max_w, unit, "width max")?;

        if min_w > max_w {
            let min_param = match min.as_ref() {
                Some(DynamicValue::Ref(p)) => Some(p.as_str()),
                _ => None,
            };
            let max_param = match max.as_ref() {
                Some(DynamicValue::Ref(p)) => Some(p.as_str()),
                _ => None,
            };
            return Err(AppError::width_bounds_inverted(
                min_w, max_w, unit, min_param, max_param,
            ));
        }

        let probe =
            RenderContext::new(unit, resolved_data, env, &images).with_instants(&resolved.instants);
        let (m_tree, root_w_req) = probe.measure_items(
            items,
            (max_w, height_units),
            [false, true],
            &geometry_values,
            "layout",
        )?;
        width_units = root_w_req.clamp(min_w, max_w);
        check_dimension_limit(width_units, unit, "width")?;
        measured = m_tree;
    } else {
        check_dimension_limit(width_units, unit, "width")?;
        let probe =
            RenderContext::new(unit, resolved_data, env, &images).with_instants(&resolved.instants);
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
    .map_err(|err| AppError::internal(format!("failed to build typst source: {err}")))?;
    writeln!(source, "#set text(font: \"Inter\")")
        .map_err(|err| AppError::internal(format!("failed to build typst source: {err}")))?;

    let context =
        RenderContext::new(unit, resolved_data, env, &images).with_instants(&resolved.instants);
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

/// Render a single representative label to PNG. For sheets, renders one slot. A thumbnail is a
/// whole request, so it resolves the template's environment itself.
pub fn render_thumbnail_png(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = resolve_environment(template, settings, datetime)?;
    let doc = compile_label_doc(template, data, &env)?;
    let page = doc
        .pages()
        .first()
        .ok_or_else(|| AppError::internal("typst did not produce any pages"))?;
    let pixmap = typst_render::render(page, &render_options(template.dpi as f32 / 72.0));
    pixmap
        .encode_png()
        .map_err(|err| AppError::internal(format!("failed to encode png: {err}")))
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

/// Render one label as a whole request: resolve the template's environment, then render a PNG.
pub fn render_single_label(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = resolve_environment(template, settings, datetime)?;
    render_single_label_image(template, data, &env, ImageRenderOptions::default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleKind {
    Png,
    Pdf,
}

/// One rendered `single` label and its resolved width (the page's), in millimetres.
pub struct RenderedSingle {
    pub bytes: Vec<u8>,
    pub width_mm: f64,
}

pub fn render_single_label_as(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
    kind: SingleKind,
    opts: ImageRenderOptions,
) -> Result<RenderedSingle, AppError> {
    let doc = compile_single_doc(template, data, env)?;
    let page = doc
        .pages()
        .first()
        .ok_or_else(|| AppError::internal("typst did not produce any pages"))?;
    let width_mm = page.frame.width().to_mm();
    let bytes = match kind {
        SingleKind::Png => {
            let dpi = opts.resolution_dpi.unwrap_or(template.dpi);
            let mut pixmap = typst_render::render(page, &render_options(dpi as f32 / 72.0));
            if opts.color_mode == ColorMode::BiLevel {
                binarize_rgba(pixmap.data_mut());
            }
            pixmap
                .encode_png()
                .map_err(|err| AppError::internal(format!("failed to encode png: {err}")))?
        }
        SingleKind::Pdf => typst_pdf::pdf(&doc, &Default::default())
            .map_err(|err| AppError::internal(format!("failed to encode pdf: {err:?}")))?,
    };
    Ok(RenderedSingle { bytes, width_mm })
}

pub fn render_single_label_image(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
    opts: ImageRenderOptions,
) -> Result<Vec<u8>, AppError> {
    render_single_label_as(template, data, env, SingleKind::Png, opts).map(|label| label.bytes)
}

pub fn render_single_label_pdf(
    template: &TemplateContent,
    data: &HashMap<String, JsonValue>,
    env: &RenderEnv,
) -> Result<Vec<u8>, AppError> {
    render_single_label_as(template, data, env, SingleKind::Pdf, Default::default())
        .map(|label| label.bytes)
}

pub fn render_sheet_pages(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    start_slot: u32,
    env: &RenderEnv,
) -> Result<Vec<u8>, AppError> {
    let TemplateFormat::Sheet {
        paper_width,
        paper_height,
        label_width,
        label_height,
        positions,
    } = &template.format
    else {
        return Err(AppError::internal(
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

    check_dimension_limit(page_width_units, unit, "paper width")?;
    check_dimension_limit(page_height_units, unit, "paper height")?;
    check_dimension_limit(*label_width, unit, "label width")?;
    check_dimension_limit(*label_height, unit, "label height")?;

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
            failures.push(crate::errors::BatchFailure::new(idx, err));
            rendered.push(String::new());
            continue;
        }
        let resolved = match resolve_parameters(template, &lbl.data, &env.defaults) {
            Ok(data) => data,
            Err(err) => {
                failures.push(crate::errors::BatchFailure::new(idx, err));
                rendered.push(String::new());
                continue;
            }
        };
        let geometry_values = render_geometry_values(&resolved.data, template);
        let context = RenderContext::new(unit, &resolved.data, env, &images)
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
                failures.push(crate::errors::BatchFailure::new(idx, err));
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
                failures.push(crate::errors::BatchFailure::new(idx, err));
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
            .map_err(|err| AppError::internal(format!("failed to build typst source: {err}")))?;
            writeln!(source, "#set text(font: \"Inter\")").map_err(|err| {
                AppError::internal(format!("failed to build typst source: {err}"))
            })?;
        } else {
            writeln!(source, "#pagebreak()").map_err(|err| {
                AppError::internal(format!("failed to build typst source: {err}"))
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
                AppError::internal(format!("failed to build typst source: {err}"))
            })?;
        }
    }
    tracing::debug!(name = %template.name, typst = %source, "render typst source");

    let doc = compile_paged(source, images.into_inner().files)?;
    typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| AppError::internal(format!("failed to encode pdf: {err:?}")))
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
                Some(ParamValue::Float(f)) => *f as f32,
                Some(ParamValue::Integer(i)) => *i as f32,
                _ => spec.min.unwrap_or(0.0) as f32,
            };
            map.insert(name.clone(), v);
        }
    }
    map
}

/// Render-time environment: the request's snapshot (the variables map and the datetime resolver)
/// and the template's tokened defaults resolved against it, built by `resolve_environment` and
/// passed together through every render call.
pub struct RenderEnv<'a> {
    pub settings: &'a BTreeMap<String, String>,
    pub datetime: &'a crate::datetime_fmt::DateTimeResolver<'a>,
    pub defaults: ResolvedDefaults,
}

pub(crate) struct RenderContext<'a> {
    pub unit: &'a str,
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
    pub background: &'a Option<crate::models::Color>,
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
        data: &'a HashMap<String, JsonValue>,
        env: &'a RenderEnv<'a>,
        images: &'a RefCell<ImageCollector>,
    ) -> Self {
        Self {
            unit,
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
                let Some(val) = self.data.get(rep_name) else {
                    continue;
                };
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

    /// The weight a `text` item is measured and drawn at: its literal, or the resolved value of
    /// the parameter it references, which `resolve_parameters` has held to the weight rule. `None`
    /// is an item that declares no weight.
    fn resolve_font_weight(
        &self,
        font_weight: Option<&DynamicValue<u16>>,
    ) -> Result<Option<u16>, AppError> {
        let name = match font_weight {
            None => return Ok(None),
            Some(DynamicValue::Literal(weight)) => return Ok(Some(*weight)),
            Some(DynamicValue::Ref(name)) => name,
        };
        let value = self.data.get(name).ok_or_else(|| {
            AppError::internal(format!(
                "parameter '{name}' has no value although load requires its default"
            ))
        })?;
        value
            .as_i64()
            .filter(|weight| crate::models::font_weight_ok(*weight))
            .and_then(|weight| u16::try_from(weight).ok())
            .map(Some)
            .ok_or_else(|| {
                AppError::internal(format!(
                    "parameter '{name}' resolved to {value}, which is not a font weight"
                ))
            })
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

                // The layout pass is unconditional: the emitted lines are the render payload
                // whether or not either axis asked for an intrinsic.
                let text_fit = helpers::layout_text(
                    helpers::TextLayoutItem {
                        raw_text: &text,
                        font_size,
                        font_weight: self.resolve_font_weight(font_weight.as_ref())?,
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
            LayoutItem::Qr {
                value,
                error_correction,
                module_size,
                quiet_zone,
                ..
            } => {
                if !demands[0] && !demands[1] {
                    return Ok(([None, None], None));
                }
                let payload = self.resolve_item_text(value)?;
                if payload.is_empty() {
                    return Ok((per_axis((0.0, 0.0)), None));
                }
                let m = module_size.ok_or_else(|| {
                    AppError::internal(format!(
                        "at {path}: a qr with a content or fill extent and no module_size passed load"
                    ))
                })?;
                let code = helpers::qr_code(payload.as_bytes(), *error_correction)?;
                let qz = *quiet_zone;
                let qr_dim = (code.width() as f32 + 2.0 * qz) * m;
                Ok((per_axis((qr_dim, qr_dim)), None))
            }
            // An image's box is always authored (`layout`, "Intrinsic sizes"); load refuses one
            // that is not.
            LayoutItem::Image { .. } => Ok(([None, None], None)),
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
                self.render_line_item(out, at, to, stroke, args.frame, args.path)?;
            }
            LayoutItem::Text {
                placement,
                font_weight,
                color,
                alignment,
                ..
            } => {
                let resolved_weight = self.resolve_font_weight(font_weight.as_ref())?;
                self.render_text_item(
                    out,
                    TextRenderArgs {
                        placement,
                        font_weight: resolved_weight,
                        color: color.as_ref(),
                        alignment,
                        pbox: args.pbox,
                        text_fit: args.measured_node.text.as_ref().unwrap(),
                    },
                )?;
            }
            LayoutItem::Qr {
                value,
                placement,
                error_correction,
                quiet_zone,
                ..
            } => {
                let payload = self.resolve_item_text(value)?;
                if !payload.is_empty() {
                    let svg_xml = build_qr_svg(payload.as_bytes(), *error_correction, *quiet_zone)?;
                    self.render_qr_item(out, svg_xml, placement, args.pbox)?;
                }
            }
            LayoutItem::Image {
                src,
                placement,
                fit,
                ..
            } => {
                self.render_image_item(out, src, placement, fit, args.pbox, args.path)?;
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

        let leading_pt = helpers::derived_leading_pt(
            weight,
            args.text_fit.font_size_pt,
            args.text_fit.line_spacing,
        )?;

        let body = format!(
            "#text(size: {}pt{weight_arg}{fill_arg})[#set par(leading: {leading_pt}pt)\n{body}]",
            args.text_fit.font_size_pt
        );

        let pad = match args.alignment.vertical {
            crate::models::VerticalAlign::Top => args.text_fit.a,
            crate::models::VerticalAlign::Bottom => args.text_fit.d,
            crate::models::VerticalAlign::Center => 0.0,
        };
        let body = pad_block(&body, pad, args.alignment.vertical);

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
            AppError::internal(format!("failed to build typst source: {err}"))
        })?;

        Ok(())
    }

    fn render_qr_item(
        &self,
        out: &mut String,
        svg_xml: String,
        placement: &Placement,
        pbox: PlacedBox,
    ) -> Result<(), AppError> {
        let top = pbox.y + pbox.h;
        let dx = format_length(pbox.x, self.unit)?;
        let dy = format_length(pbox.frame.1 - top, self.unit)?;
        let box_width = format_length(pbox.w, self.unit)?;
        let box_height = format_length(pbox.h, self.unit)?;
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
            AppError::internal(format!("failed to build typst source: {err}"))
        })?;

        Ok(())
    }

    /// The image an item names, or `None` when its source resolves to the empty string (layout spec,
    /// "The image item": such an image draws nothing).
    fn render_image_item(
        &self,
        out: &mut String,
        src: &str,
        placement: &Placement,
        fit: &Fit,
        pbox: PlacedBox,
        path: &str,
    ) -> Result<(), AppError> {
        let resolved_src = self.resolve_item_text(src)?;
        if resolved_src.is_empty() {
            return Ok(());
        }
        let (bytes, fmt) = if resolved_src.starts_with("data:") {
            parse_image_data_uri(&resolved_src, path)?
        } else {
            resolve_image_asset(&assets_root(), &resolved_src, path)?
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
            AppError::internal(format!("failed to build typst source: {err}"))
        })?;

        Ok(())
    }

    fn render_line_item(
        &self,
        out: &mut String,
        at: &Position,
        to: &Position,
        stroke: &Stroke,
        frame: (f32, f32),
        path: &str,
    ) -> Result<(), AppError> {
        let start_point = self.resolve_point(at, frame, path)?;
        let end_point = self.resolve_point(to, frame, path)?;
        self.check_line(&start_point, &end_point, frame, path)?;
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
        let color = format!("rgb(\"{}\")", stroke.color.hex());

        let content = format!(
            "#line(start: ({zero}, {zero}), end: ({dx}, {dy}), stroke: {thickness} + {color})"
        );
        writeln!(
            out,
            "#place(top + left, dx: {start_x}, dy: {start_y})[{content}]"
        )
        .map_err(|err| AppError::internal(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_container_item(
        &self,
        out: &mut String,
        args: ContainerRenderArgs<'_>,
    ) -> Result<(), AppError> {
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
            Some(bg) => format!("rgb(\"{}\")", bg.hex()),
            None => "none".to_string(),
        };
        let stroke = match args.stroke {
            Some(st) => {
                let thickness = format_length(st.thickness, self.unit)?;
                let color = format!("rgb(\"{}\")", st.color.hex());
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
                    AppError::internal(format!("failed to build typst source: {err}"))
                })?;
            }
            Shape::Ellipse => {
                if args.stroke.is_some() || args.background.is_some() {
                    let frame_content = format!(
                        "#ellipse(width: {box_width}, height: {box_height}, fill: {fill}, stroke: {stroke})"
                    );
                    writeln!(
                        out,
                        "#place(top + left, dx: {dx}, dy: {dy})[{frame_content}]"
                    )
                    .map_err(|err| {
                        AppError::internal(format!("failed to build typst source: {err}"))
                    })?;
                }
                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{rotated}]]"
                )
                .map_err(|err| {
                    AppError::internal(format!("failed to build typst source: {err}"))
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
    use serde_json::{json, Value as JsonValue};
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    fn render_test_items(items: &[LayoutItem], frame: (f32, f32)) -> Result<String, AppError> {
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
            defaults: Default::default(),
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);
        let geometry_values = HashMap::new();
        let (measured, _) =
            ctx.measure_items(items, frame, [true, true], &geometry_values, "layout")?;
        ctx.render_items(items, &measured, frame, &geometry_values, None, "layout")
    }

    /// Measure and draw read a weight through one helper: a literal as written, a reference as its
    /// resolved value, and an item without a weight as `None`. An absent value is a 500: load
    /// requires a referenced parameter's default.
    #[test]
    fn a_text_weight_resolves_through_one_helper() {
        let data = HashMap::from([("heft".to_string(), json!(700))]);
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
            defaults: Default::default(),
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);
        assert_eq!(ctx.resolve_font_weight(None).unwrap(), None);
        let literal = DynamicValue::Literal(300);
        assert_eq!(ctx.resolve_font_weight(Some(&literal)).unwrap(), Some(300));
        let heft = DynamicValue::param_ref("heft");
        assert_eq!(ctx.resolve_font_weight(Some(&heft)).unwrap(), Some(700));
        let gone = DynamicValue::param_ref("gone");
        let err = ctx.resolve_font_weight(Some(&gone)).unwrap_err();
        assert_eq!(err.status().as_u16(), 500);
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

    /// The emitted pad is the measured ink past the aligned edge for the instance rendered.
    #[test]
    fn the_emitted_pad_is_the_aligned_edge_metric() {
        let face = super::helpers::instance(400, 20.0).expect("face");
        let pitch = super::helpers::line_pitch(20.0, super::helpers::DEFAULT_LINE_SPACING);
        let edgy_ink = super::helpers::measure_block_ink(&face, &["Édgy"], 20.0, pitch);
        let gjpqy_ink = super::helpers::measure_block_ink(&face, &["gjpqy"], 20.0, pitch);

        let helix = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "HELIX",
            VerticalAlign::Top,
        );
        assert!(
            !helix.contains("#pad"),
            "top-aligned HELIX must not pad: {helix}"
        );

        let top = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "Édgy",
            VerticalAlign::Top,
        );
        assert!(
            top.contains(&format!("#pad(top: {}pt)", edgy_ink.a)),
            "unexpected top pad for Édgy: {top}"
        );
        // É's accent, 1928 − 1490 units at wght 400 and opsz 20 [fontTools], not the 4.82 pt band.
        let accent = (1928.0 - 1490.0) / 2048.0 * 20.0;
        assert!(
            (edgy_ink.a - accent).abs() < 1e-3,
            "Édgy top pad {}",
            edgy_ink.a
        );

        let bottom = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "gjpqy",
            VerticalAlign::Bottom,
        );
        assert!(
            bottom.contains(&format!("#pad(bottom: {}pt)", gjpqy_ink.d)),
            "unexpected bottom pad for gjpqy: {bottom}"
        );

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
                defaults: Default::default(),
            };
            let images = RefCell::new(super::ImageCollector::default());
            let ctx = super::RenderContext::new("mm", &data, &env, &images);
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
            defaults: Default::default(),
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);

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
            defaults: Default::default(),
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);

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
            defaults: Default::default(),
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);
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

    /// `measure_container_footprint` resolved this width with no fallback, so a
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(60.0),
                }
                .into(),
                height: 40.0.into(),
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

    /// The Task 1 loosening admits this at load and it then fails at render, because the
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
                error_correction: crate::models::ErrorCorrection::M,
                module_size: Some(1.0),
                quiet_zone: 0.0,
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

    fn test_placeholder_data(
        template: &TemplateContent,
        now: chrono::DateTime<chrono::Local>,
    ) -> HashMap<String, serde_json::Value> {
        template.placeholder_data(now)
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
            error_correction: crate::models::ErrorCorrection::M,
            module_size: Some(2.0),
            quiet_zone: 0.0,
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
                error_correction: crate::models::ErrorCorrection::M,
                module_size: Some(1.0),
                quiet_zone: 0.0,
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
                error_correction: crate::models::ErrorCorrection::M,
                module_size: None,
                quiet_zone: 0.0,
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(5.0),
                    max: Some(100.0),
                }
                .into(),
                height: 8.0.into(),
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
    /// idiom, accepted by `validate()` and rendered fine by
    /// `render_container_item` (which passes `allow_auto_fill: true`); only the measure pre-pass had
    /// been tightened, so every such container on a dynamic-width label started failing measurement
    /// with "size height is auto but no max_height provided".
    #[test]
    fn an_auto_height_fixed_width_container_measures_without_erroring() {
        let template = TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                }
                .into(),
                height: 30.0.into(),
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
                color: Color::black(),
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(80.0).into(),
                height: 40.0.into(),
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
                    color: Color::black(),
                }),
                background: None,
                rounded: None,
                padding: Padding::ZERO,
                flow: None,
                repeat: None,
                items,
            }]),
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
                error_correction: crate::models::ErrorCorrection::M,
                module_size: None,
                quiet_zone: 0.0,
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
                error_correction: crate::models::ErrorCorrection::M,
                module_size: None,
                quiet_zone: 0.0,
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
                color: Color::black(),
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(80.0).into(),
                height: 40.0.into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![outer]),
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: HEIGHT_MM.into(),
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
            height: height_mm.into(),
            media_width: None,
        };
        let Layout::Items(items) = &mut t.layout;
        if let Some(LayoutItem::Text { placement, .. }) = items.first_mut() {
            placement.extent =
                Extent::Size(Size([SizeValue::content(), SizeValue::fixed(height_mm)]));
        }
        t
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
                defaults: Default::default(),
            };
            let images = RefCell::new(super::ImageCollector::default());
            let ctx = super::RenderContext::new("mm", &data, &env, &images);

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
            let lines: Vec<&str> = text.split('\n').collect();
            let expected_h_pt = super::helpers::block_height_with_align_for_test(
                weight,
                font_pt,
                &lines,
                VerticalAlign::Top,
            );
            let expected_h_mm = super::helpers::pt_to_units_for_test(expected_h_pt, "mm");
            assert!(
                (measured_h_mm - expected_h_mm).abs() < 0.01,
                "{label}: measured {measured_h_mm}mm, expected {expected_h_mm}mm"
            );
        }
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
            content: TemplateContent {
                name: "Sheet2".to_string(),
                description: String::new(),
                categories: Vec::new(),
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
                        default_instant: None,
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
            &crate::render::resolve_environment(&two_slot_sheet(), &no_settings(), &no_datetime())
                .unwrap(),
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
            &crate::render::resolve_environment(&two_slot_sheet(), &no_settings(), &no_datetime())
                .unwrap(),
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
                data: HashMap::from([("message".to_string(), json!(["a", "b"]))]),
            },
        ];
        let err = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            0,
            &crate::render::resolve_environment(&two_slot_sheet(), &no_settings(), &no_datetime())
                .unwrap(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
    }

    #[test]
    fn render_single_label_produces_png() {
        let template = TemplateContent {
            name: "Test".to_string(),
            description: "Test template".to_string(),
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: 10.0.into(),
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
                    default_instant: None,
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(30.0).into(),
                height: 20.0.into(),
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
                    default_instant: None,
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
                    error_correction: crate::models::ErrorCorrection::M,
                    module_size: None,
                    quiet_zone: 0.0,
                    when: None,
                },
                LayoutItem::Line {
                    at: Position([0.0, 1.0]),
                    to: Position([30.0, 1.0]),
                    stroke: Stroke {
                        thickness: 0.2,
                        color: Color::black(),
                    },
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
                        color: Color::black(),
                    }),
                    background: None,
                    rounded: Some(0.4),
                    padding: Padding::ZERO,
                    flow: None,
                    repeat: None,
                    items: Vec::new(),
                },
            ]),
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
            content: TemplateContent {
                name: "Sheet".to_string(),
                description: "Sheet template".to_string(),
                categories: Vec::new(),
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
                        default_instant: None,
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
            },
        };

        let labels = vec![LabelInput {
            data: HashMap::from([("message".to_string(), json!("Hello"))]),
        }];

        let pdf = render_sheet_pages(
            &template,
            &labels,
            0,
            &crate::render::resolve_environment(&template, &no_settings(), &no_datetime()).unwrap(),
        )
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: 20.0.into(),
                media_width: None,
            },
            params: IndexMap::new(),
            layout: Layout::Items(vec![LayoutItem::Image {
                src: "{logo}".to_owned(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                ),
                fit: Fit::Contain,
                when: None,
            }]),
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
    fn render_image_absent_data_draws_nothing() {
        let template = image_single_template();
        let data = HashMap::new();
        assert!(render_single_label(&template, &data, &no_settings(), &no_datetime()).is_ok());
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
            content: TemplateContent {
                name: "Sheet".to_string(),
                description: String::new(),
                categories: Vec::new(),
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
                        default_instant: None,
                    },
                )]),
                layout: Layout::Items(vec![LayoutItem::Image {
                    src: "{logo}".to_owned(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(20.0), SizeValue::fixed(20.0)]),
                    ),
                    fit: Fit::Contain,
                    when: None,
                }]),
            },
        };
        let labels = vec![LabelInput {
            data: HashMap::from([(
                "logo".to_string(),
                json!(format!("data:image/png;base64,{PNG_1X1_B64}")),
            )]),
        }];
        let pdf = render_sheet_pages(
            &template,
            &labels,
            0,
            &crate::render::resolve_environment(&template, &no_settings(), &no_datetime()).unwrap(),
        )
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
        let data = HashMap::from([("logo".to_string(), json!("data:image/png;base64,@@@"))]);
        let image_err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("an undecodable image payload must not render");

        let geometry_err = render_test_items(
            &[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([30.0, 6.0]),
                stroke: Stroke {
                    thickness: 0.2,
                    color: Color::black(),
                },
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

    /// layout spec, "Render failures locate the item": an image that fails inside a repeat
    /// names its instance's path.
    #[test]
    fn an_image_render_failure_names_the_instance_path() {
        let yaml = "name: T\nunit: mm\ndpi: 100\nformat: { type: single, width: 60, height: 20 }\nparams:\n  - name: logos\n    type: list\nlayout:\n  - type: container\n    at: [0, 0]\n    size: [60, 20]\n    flow: { direction: row }\n    items:\n      - type: container\n        repeat: logos\n        size: [20, 20]\n        items:\n          - type: image\n            src: \"{logos}\"\n            at: [0, 0]\n            size: [20, 20]\n";
        let template = crate::parse::parse_template(yaml).expect("parse");
        template.validate().expect("validate");
        let data = HashMap::from([(
            "logos".to_string(),
            json!([
                format!("data:image/png;base64,{PNG_1X1_B64}"),
                "data:image/png;base64,@@@"
            ]),
        )]);
        let err = render_single_label(&template, &data, &no_settings(), &no_datetime())
            .expect_err("an undecodable image payload must not render");
        assert_eq!(err.reason(), Some("image_data_invalid"));
        let message = err.message_text();
        assert!(
            message.contains("layout[0].items[0]#1.items[0]"),
            "expected the second instance's image path in: {message}"
        );
    }

    fn image_single_template_with_src(src: &str) -> TemplateContent {
        let mut template = image_single_template();
        template.layout = Layout::Items(vec![LayoutItem::Image {
            src: src.to_string(),
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0).into(),
                height: 10.0.into(),
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
        };
        let data = HashMap::from([("message".to_string(), json!("Hello"))]);
        let pdf = render_single_label_pdf(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &no_settings(), &no_datetime()).unwrap(),
        )
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
        let summaries = registry.summaries(&BTreeMap::new(), &no_datetime());
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
        for summary in registry.summaries(&settings, &dt) {
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
            "catalog contents changed; update this gate deliberately"
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0).into(),
                height: 20.0.into(),
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
                    error_correction: crate::models::ErrorCorrection::M,
                    module_size: None,
                    quiet_zone: 0.0,
                    when: None,
                },
            ]),
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(60.0).into(),
                height: 20.0.into(),
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
            categories: Vec::new(),
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
        }
    }

    #[test]
    fn placeholder_data_holds_declared_parameters_only() {
        use crate::models::{Alignment, Fit, FontSize, Position, Size, SizeValue};
        let template = TemplateContent {
            name: "t".into(),
            description: String::new(),
            categories: Vec::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(40.0).into(),
                height: 20.0.into(),
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
                        default_instant: None,
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
                        default_instant: None,
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
                        default_instant: None,
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
                    src: "{logo}".into(),
                    placement: Placement::sized(
                        Position([0.0, 10.0]),
                        Size([SizeValue::fixed(5.0), SizeValue::fixed(5.0)]),
                    ),
                    fit: Fit::default(),
                    when: None,
                },
            ]),
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
            categories: Vec::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(40.0).into(),
                height: 20.0.into(),
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
                    default_instant: None,
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
        for summary in registry.summaries(&BTreeMap::new(), &datetime) {
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

    /// Label-level tests for the measured ink reservation (#392). Expected values come from what
    /// Typst laid out or rasterised, or from the fontTools/HarfBuzz figures in the change's design,
    /// never from the measurement under test; the calibration is the one place the two meet.
    mod measured_ink {
        use super::{fitted_pt, no_datetime, no_settings, parse_and_validate};
        use crate::errors::AppError;
        use crate::models::VerticalAlign;
        use crate::templates::TemplateContent;
        use std::collections::HashMap;
        use typst::layout::{Frame, FrameItem, Point, Transform};

        const DPI: u32 = 180;
        /// Inter's cap height at 20 pt: 1490 of 2048 units.
        const CAP_20: f64 = 1490.0 / 2048.0 * 20.0;
        /// How far `É`'s accent rises above cap height at wght 400, opsz 20: 1928 − 1490 units.
        const E_ACCENT_20: f64 = (1928.0 - 1490.0) / 2048.0 * 20.0;
        /// Inter's typographic ascender above cap height at 20 pt, which the old top band stopped at.
        const OLD_TOP_BAND_20: f64 = (1984.0 - 1490.0) / 2048.0 * 20.0;

        fn mm(pt: f64) -> String {
            format!("{:.9}", pt * 25.4 / 72.0)
        }

        /// A one-item label, 120 × 80 mm at 180 dpi, whose text box is 100 mm wide at (10, 10) mm,
        /// clear of every edge. `height` is YAML (`content` or millimetres), and `extra` adds keys
        /// to the item.
        fn label(
            value: &str,
            vertical: &str,
            font_size: &str,
            height: &str,
            extra: &str,
        ) -> TemplateContent {
            label_at(value, vertical, font_size, height, 10.0, extra)
        }

        fn label_at(
            value: &str,
            vertical: &str,
            font_size: &str,
            height: &str,
            y_mm: f64,
            extra: &str,
        ) -> TemplateContent {
            let value = serde_json::to_string(value).expect("json string");
            let yaml = format!(
                "name: Ink\nunit: mm\ndpi: {DPI}\nformat: {{ type: single, width: 120, height: 80 }}\nlayout:\n  - type: text\n    value: {value}\n    at: [10, {y_mm:.6}]\n    size: [100, {height}]\n    font_size: {font_size}\n    alignment: {{ horizontal: left, vertical: {vertical} }}\n{extra}"
            );
            parse_and_validate(&yaml).expect("valid template")
        }

        fn compiled(t: &TemplateContent) -> Result<crate::render::CompiledSource, AppError> {
            let (settings, datetime) = (no_settings(), no_datetime());
            let env = crate::render::RenderEnv {
                settings: &settings,
                datetime: &datetime,
                defaults: Default::default(),
            };
            crate::render::compile_label_source(t, &HashMap::new(), &env)
        }

        fn length_pt(s: &str) -> f64 {
            let s = s.trim();
            let (value, per_unit) = if let Some(v) = s.strip_suffix("mm") {
                (v, 72.0 / 25.4)
            } else if let Some(v) = s.strip_suffix("pt") {
                (v, 1.0)
            } else if let Some(v) = s.strip_suffix("in") {
                (v, 72.0)
            } else {
                panic!("no unit on length {s:?}")
            };
            value.parse::<f64>().expect("a number") * per_unit
        }

        fn between<'s>(s: &'s str, start: &str, end: &str) -> &'s str {
            let from = s.find(start).unwrap_or_else(|| panic!("{start:?} in {s}")) + start.len();
            let to = s[from..]
                .find(end)
                .unwrap_or_else(|| panic!("{end:?} in {s}"))
                + from;
            &s[from..to]
        }

        /// The first line of the one top-level item in `source` whose source holds `needle`, with
        /// its clipped box's top and height in page points. An item runs from its `#place` line to
        /// the next one, since a text item's body spans lines.
        fn item_box<'s>(source: &'s str, needle: &str) -> (&'s str, f64, f64) {
            let starts: Vec<usize> = source
                .match_indices("#place(top + left, dx: ")
                .map(|(i, _)| i)
                .filter(|&i| i == 0 || source.as_bytes()[i - 1] == b'\n')
                .collect();
            let items: Vec<&str> = starts
                .iter()
                .enumerate()
                .map(|(k, &i)| &source[i..starts.get(k + 1).copied().unwrap_or(source.len())])
                .filter(|item| item.contains(needle))
                .collect();
            assert_eq!(items.len(), 1, "one item holding {needle:?} in {source}");
            let line = items[0].lines().next().expect("a line");
            assert!(line.contains("clip: true"), "an unclipped item: {line}");
            let top = length_pt(between(line, "dy: ", ")[#box("));
            let height = length_pt(between(line, ", height: ", ", clip: true)"));
            (line, top, height)
        }

        /// The inset emitted at `edge` (`top` or `bottom`), if any.
        fn emitted_pad(source: &str, edge: &str) -> Option<f64> {
            let key = format!("#pad({edge}: ");
            let from = source.find(&key)? + key.len();
            let to = source[from..].find(')')? + from;
            Some(length_pt(&source[from..to]))
        }

        /// The lines a text item emits, in order.
        fn emitted_lines(source: &str) -> Vec<String> {
            let re = regex::Regex::new(r#"#text\("((?:[^"\\]|\\.)*)"\)"#).expect("regex");
            re.captures_iter(source).map(|c| c[1].to_string()).collect()
        }

        /// The first page's RGBA bytes, one row of `width` pixels after another.
        fn raster(source: String, files: &[(String, Vec<u8>)]) -> (usize, Vec<u8>) {
            let doc = crate::render::compile_paged(source, files.to_vec()).expect("compile");
            let pixmap = typst_render::render(
                &doc.pages()[0],
                &crate::render::render_options(DPI as f32 / 72.0),
            );
            (pixmap.width() as usize, pixmap.data().to_vec())
        }

        /// Containment as the layout-sizing spec defines it (task 6.1). The label is rendered twice
        /// from the same source, once as emitted and once with only the item's `clip: true` removed.
        /// Both pages grow by one inch on every side, which moves the item's box clear of every label
        /// edge by exactly `DPI` raster rows, so the text keeps the raster phase it has when emitted.
        /// Whatever the unclipped render adds in a row wholly outside the item's box is ink the clip
        /// cut; judging that difference rather than all ink keeps other items out of the verdict.
        fn containment(
            source: &str,
            files: &[(String, Vec<u8>)],
            needle: &str,
        ) -> Result<(), String> {
            let (line, top, height) = item_box(source, needle);
            let page = source
                .lines()
                .find(|l| l.starts_with("#set page("))
                .expect("a page setup");
            let re =
                regex::Regex::new(r"^#set page\(width: (.+), height: (.+), margin: 0(?:mm|in)\)$")
                    .expect("regex");
            let caps = re
                .captures(page)
                .unwrap_or_else(|| panic!("page setup {page}"));
            let grown = format!(
                "#set page(width: {} + 144pt, height: {} + 144pt, margin: 72pt)",
                &caps[1], &caps[2]
            );
            let clipped = source.replacen(page, &grown, 1);
            let unclipped =
                clipped.replacen(line, &line.replacen("clip: true", "clip: false", 1), 1);
            assert_ne!(clipped, unclipped, "the item's clip was not removed");
            let (clipped, unclipped) = (raster(clipped, files), raster(unclipped, files));

            let scale = f64::from(DPI) / 72.0;
            let (top_px, bottom_px) = ((72.0 + top) * scale, (72.0 + top + height) * scale);
            let stride = clipped.0 * 4;
            for (y, (a, b)) in clipped
                .1
                .chunks(stride)
                .zip(unclipped.1.chunks(stride))
                .enumerate()
            {
                let above = (y + 1) as f64 <= top_px;
                let below = y as f64 >= bottom_px;
                if (above || below) && a != b {
                    let side = if above { "above" } else { "below" };
                    return Err(format!(
                        "the clip cut ink in raster row {y}, wholly {side} the box ({top_px:.2}..{bottom_px:.2})"
                    ));
                }
            }
            Ok(())
        }

        fn assert_contained(t: &TemplateContent) {
            let c = compiled(t).expect("compile label");
            containment(&c.source, &c.files, "#text(\"").unwrap_or_else(|e| panic!("{e}"));
        }

        /// One glyph as Typst laid it out, in page points with y down: the baseline of the nearest
        /// enclosing frame that has one, and the top and bottom of its outline. Typst inlines each
        /// line frame into its paragraph's, which keeps the first line's baseline, so the baseline
        /// is the glyph's own only in a one-line block; the tests read it from nothing else.
        #[derive(Debug, Clone, Copy)]
        struct LaidGlyph {
            baseline: f64,
            /// Where its text item sits: the line's baseline less any vertical offset Typst moved
            /// into the item.
            item: f64,
            top: f64,
            bottom: f64,
        }

        /// Walk the frame tree, accumulating every group transform and item position down to each
        /// text item (design Decision 3a). Each glyph's outline is bounded from that item's font at
        /// that item's size and placed at the item's position plus the glyph's `x_offset` and the
        /// advances before it. `Glyph::y_offset` is never read: Typst moves a glyph's vertical offset
        /// into its text item's position and emits the glyph with zero (`typst-layout`
        /// `src/inline/shaping.rs:358,429`), so reading it would miss a positioned mark.
        fn walk(frame: &Frame, ts: Transform, baseline: Option<f64>, out: &mut Vec<LaidGlyph>) {
            let baseline = if frame.has_baseline() {
                Some(Point::with_y(frame.baseline()).transform(ts).y.to_pt())
            } else {
                baseline
            };
            for (pos, item) in frame.items() {
                match item {
                    FrameItem::Group(group) => walk(
                        &group.frame,
                        ts.pre_concat(Transform::translate(pos.x, pos.y))
                            .pre_concat(group.transform),
                        baseline,
                        out,
                    ),
                    FrameItem::Text(text) => {
                        let baseline = baseline.expect("a text item sits in a line frame");
                        let mut x = pos.x;
                        for glyph in &text.glyphs {
                            let gx = x + glyph.x_offset.at(text.size);
                            x += glyph.x_advance.at(text.size);
                            let Some((bottom, top)) = crate::render::helpers::glyph_ink(
                                text.font.ttf(),
                                ttf_parser::GlyphId(glyph.id),
                            ) else {
                                continue;
                            };
                            let y = |units: f32| {
                                Point::new(gx, pos.y - text.font.to_em(units).at(text.size))
                                    .transform(ts)
                                    .y
                                    .to_pt()
                            };
                            out.push(LaidGlyph {
                                baseline,
                                item: y(0.0),
                                top: y(top),
                                bottom: y(bottom),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        fn laid_out(t: &TemplateContent) -> Vec<LaidGlyph> {
            let c = compiled(t).expect("compile label");
            let doc = crate::render::compile_paged(c.source, c.files).expect("compile");
            let mut out = Vec::new();
            walk(&doc.pages()[0].frame, Transform::identity(), None, &mut out);
            assert!(!out.is_empty(), "no glyphs laid out");
            out
        }

        /// The highest ink above the baseline and the lowest below it, over a one-line block.
        fn laid_rise_fall(glyphs: &[LaidGlyph]) -> (f64, f64) {
            glyphs.iter().fold((f64::MIN, f64::MIN), |(r, f), g| {
                (r.max(g.baseline - g.top), f.max(g.bottom - g.baseline))
            })
        }

        fn baselines(glyphs: &[LaidGlyph]) -> Vec<f64> {
            let mut out: Vec<f64> = Vec::new();
            for g in glyphs {
                if !out.iter().any(|b| (b - g.baseline).abs() < 1e-6) {
                    out.push(g.baseline);
                }
            }
            out
        }

        fn row(pt: f64) -> i64 {
            (pt * f64::from(DPI) / 72.0).floor() as i64
        }

        /// First and last rows of the rendered PNG carrying ink.
        fn ink_rows(t: &TemplateContent) -> (u32, u32) {
            let png = crate::render::render_single_label(
                t,
                &HashMap::new(),
                &no_settings(),
                &no_datetime(),
            )
            .expect("render label");
            let img = image::load_from_memory(&png).expect("decode").to_luma8();
            let inked: Vec<u32> = (0..img.height())
                .filter(|&y| (0..img.width()).any(|x| img.get_pixel(x, y).0[0] < 128))
                .collect();
            (inked[0], inked[inked.len() - 1])
        }

        // ---- 3. Calibration against Typst's own frame ----

        /// A single line's rise and fall as Typst laid it out through the real renderer, at a fixed
        /// size in a box far larger than it needs.
        fn typst_line_ink(value: &str, weight: u16) -> (f64, f64) {
            let t = label(
                value,
                "top",
                "20",
                "40",
                &format!("    font_weight: {weight}\n    overflow: fail\n"),
            );
            laid_rise_fall(&laid_out(&t))
        }

        /// Each line's rise and fall as Typst laid out a block of `lines` through the real renderer.
        /// Typst inlines line frames, so line `i`'s baseline is the first line's plus `i − 1`
        /// pitches, the stacking `block_height_matches_typst_layout` pins, and each glyph belongs to
        /// the line whose baseline its text item sits nearest.
        fn typst_block_ink(lines: &[&str]) -> Vec<Option<(f64, f64)>> {
            let t = label(
                &lines.join("\n"),
                "top",
                "20",
                "70",
                "    wrap: false\n    overflow: fail\n",
            );
            let glyphs = laid_out(&t);
            let (first, pitch) = (glyphs[0].baseline, 24.0);
            let mut out = vec![None; lines.len()];
            for g in glyphs {
                let i = ((g.item - first) / pitch).round() as usize;
                let baseline = first + i as f64 * pitch;
                let (rise, fall) = (baseline - g.top, g.bottom - baseline);
                out[i] = Some(out[i].map_or((rise, fall), |(r, f): (f64, f64)| {
                    (r.max(rise), f.max(fall))
                }));
            }
            out
        }

        /// The measurement shapes the block as Typst does, as one paragraph: a run of characters of
        /// no specific script belongs to the run around it, across the line break. `0́` before `α`
        /// is shaped in a Greek run, where Inter leaves the acute unraised; alone, the acute is
        /// raised 393 units.
        #[test]
        fn the_block_measurement_matches_typsts_frame() {
            let face = crate::render::helpers::instance(400, 20.0).expect("face");
            for lines in [
                &["0\u{0301}", "α"][..],
                &["α", "0\u{0301}"],
                &["0\u{0301}", "HELIX"],
                &["HELIX", "Émile"],
                &["g\u{0323}", "HELIX", "gyp"],
                &["αH\u{0301}", "H\u{0301}α"],
                &["12:30", "1:1"],
                &["ПриветHello", "0\u{0301}", "Привет"],
            ] {
                let typst = typst_block_ink(lines);
                let measured = crate::render::helpers::line_inks(&face, lines, 20.0);
                for (i, (t, m)) in typst.iter().zip(&measured).enumerate() {
                    let (rise, fall) = t.expect("Typst inked the line");
                    let m = m.expect("measured ink");
                    assert!(
                        (f64::from(m.rise) - rise).abs() <= 0.01 && (f64::from(m.fall) - fall).abs() <= 0.01,
                        "{lines:?} line {i}: measured rise {} fall {}, Typst laid out rise {rise} fall {fall}",
                        m.rise,
                        m.fall
                    );
                }
            }
            // The oracle tells the paragraph from lines shaped apart.
            let alone =
                crate::render::helpers::shape_line_ink(&face, "0\u{0301}", 20.0).expect("ink");
            let (paragraph_rise, _) = typst_block_ink(&["0\u{0301}", "α"])[0].expect("ink");
            assert!(
                f64::from(alone.rise) - paragraph_rise > 3.0,
                "alone {} vs in the paragraph {paragraph_rise}",
                alone.rise
            );
        }

        /// A generic line takes the script of the line after it, so `0́` over `α` reserves only the
        /// ink its Greek run draws: its block fits a 40 pt box that shaping the lines apart, with
        /// the acute raised, would refuse at about 43 pt.
        #[test]
        fn a_line_of_no_script_is_measured_in_the_run_it_joins() {
            let t = label(
                "0\u{0301}\nα",
                "top",
                "20",
                &mm(40.0),
                "    wrap: false\n    overflow: fail\n",
            );
            crate::render::render_single_label(&t, &HashMap::new(), &no_settings(), &no_datetime())
                .expect("0́ over α fits 40 pt");
            assert_contained(&t);
        }

        #[test]
        fn the_line_measurement_matches_typsts_frame() {
            let face = crate::render::helpers::instance(400, 20.0).expect("face");
            for value in [
                "HELIX",
                "Émile",
                "E\u{0301}",
                "12:30",
                "αH\u{0301}",
                "ПриветHello",
                "g\u{0323}",
                "Ǻ",
                "gyp",
                "...",
                "g É",
            ] {
                let (rise, fall) = typst_line_ink(value, 400);
                let measured =
                    crate::render::helpers::shape_line_ink(&face, value, 20.0).expect("ink");
                assert!(
                    (f64::from(measured.rise) - rise).abs() <= 0.01
                        && (f64::from(measured.fall) - fall).abs() <= 0.01,
                    "{value:?}: measured rise {} fall {}, Typst laid out rise {rise} fall {fall}",
                    measured.rise,
                    measured.fall
                );
            }
        }

        /// The calibration can tell a segmented measurement from a whole-line one: shaping `αH́` as
        /// one buffer leaves the acute unpositioned, 393 units (3.84 pt at 20 pt) below where Typst
        /// puts it.
        #[test]
        fn the_calibration_refuses_whole_line_shaping() {
            let face = crate::render::helpers::instance(400, 20.0).expect("face");
            let value = "αH\u{0301}";
            let (typst_rise, _) = typst_line_ink(value, 400);

            let segmented =
                crate::render::helpers::shape_line_ink(&face, value, 20.0).expect("ink");
            assert!((f64::from(segmented.rise) - typst_rise).abs() <= 0.01);

            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(value);
            buffer.guess_segment_properties();
            buffer.set_flags(rustybuzz::BufferFlags::REMOVE_DEFAULT_IGNORABLES);
            let glyphs = rustybuzz::shape(&face, &[], buffer);
            let top_units = glyphs
                .glyph_infos()
                .iter()
                .zip(glyphs.glyph_positions())
                .filter_map(|(info, pos)| {
                    face.glyph_bounding_box(ttf_parser::GlyphId(info.glyph_id as u16))
                        .map(|b| f64::from(b.y_max) + f64::from(pos.y_offset))
                })
                .fold(f64::MIN, f64::max);
            let whole_line_rise = top_units / 2048.0 * 20.0;
            let miss = typst_rise - whole_line_rise;
            assert!(miss > 0.01, "whole-line shaping must fail the calibration");
            assert!(
                (miss - 3.84).abs() < 0.01,
                "whole-line shaping misses by {miss} pt"
            );
        }

        // ---- 6. The containment oracle, proved by sabotage ----

        /// Ink moved one row past an edge shows in the row beyond only by the part of a row the edge
        /// leaves uncovered, which is nothing when the edge sits at the bottom of its row. So each
        /// label is first moved to put the edge under test mid-row, and the sabotage is then visible
        /// by construction rather than by luck of the raster phase.
        #[test]
        fn the_containment_oracle_catches_one_row_of_cut_ink() {
            let scale = f64::from(DPI) / 72.0;
            let row_pt = 1.0 / scale;
            for (value, vertical, edge) in [("É", "top", "top"), ("gyp", "bottom", "bottom")] {
                let probe =
                    compiled(&label(value, vertical, "20", "content", "")).expect("compile");
                let (_, top, height) = item_box(&probe.source, "#text(\"");
                let edge_px = (72.0 + if edge == "top" { top } else { top + height }) * scale;
                let raise_pt = (edge_px.floor() + 0.5 - edge_px) / scale;
                let y_mm = 10.0 - raise_pt * 25.4 / 72.0;
                let c = compiled(&label_at(value, vertical, "20", "content", y_mm, ""))
                    .expect("compile");
                containment(&c.source, &c.files, "#text(\"").expect("the emitted pad contains it");

                let pad = emitted_pad(&c.source, edge).expect("a pad");
                let emitted = format!("#pad({edge}: ");
                let at = c.source.find(&emitted).expect("pad") + emitted.len();
                let end = c.source[at..].find(')').expect("pad end") + at;
                let sabotaged =
                    format!("{}{}pt{}", &c.source[..at], pad - row_pt, &c.source[end..]);
                let err = containment(&sabotaged, &c.files, "#text(\"")
                    .expect_err("a pad one raster row short must fail containment");
                assert!(
                    err.contains(&format!(
                        "wholly {}",
                        if edge == "top" { "above" } else { "below" }
                    )),
                    "{value}: {err}"
                );
            }
        }

        // ---- 7. Vertical alignment places a fixed metric box ----

        #[test]
        fn unaccented_capitals_sit_flush_with_the_top_edge() {
            let t = label("HELIX", "top", "20", "20", "");
            let c = compiled(&t).expect("compile");
            assert_eq!(emitted_pad(&c.source, "top"), None, "{}", c.source);
            let (_, box_top, _) = item_box(&c.source, "#text(\"");
            let glyphs = laid_out(&t);
            let metric_top = glyphs[0].baseline - CAP_20;
            assert!(
                (metric_top - box_top).abs() <= 0.01,
                "metric top {metric_top} pt, box top {box_top} pt"
            );
            let edge_px = box_top * f64::from(DPI) / 72.0;
            let (first, _) = ink_rows(&t);
            assert!(
                (f64::from(first) - edge_px).abs() <= 1.0,
                "first inked row {first}, box top edge at row {edge_px:.2}"
            );
        }

        #[test]
        fn an_accented_capital_is_inset_by_its_own_accent_and_stays_whole() {
            let t = label("Émile", "top", "20", "content", "");
            let c = compiled(&t).expect("compile");
            let pad = emitted_pad(&c.source, "top").expect("a top pad");
            assert!(
                (pad - E_ACCENT_20).abs() < 1e-3,
                "pad {pad} pt, accent {E_ACCENT_20} pt"
            );
            let glyphs = laid_out(&t);
            let (rise, _) = laid_rise_fall(&glyphs);
            assert!(
                (rise - CAP_20 - pad).abs() <= 0.01,
                "Typst laid the accent {rise} pt high"
            );
            let (_, box_top, _) = item_box(&c.source, "#text(\"");
            assert!((glyphs[0].baseline - CAP_20 - box_top - pad).abs() <= 0.01);
            assert_contained(&t);
        }

        #[test]
        fn a_descender_is_inset_by_its_own_depth_and_stays_whole() {
            let t = label("gyp", "bottom", "20", "content", "");
            let c = compiled(&t).expect("compile");
            let pad = emitted_pad(&c.source, "bottom").expect("a bottom pad");
            let glyphs = laid_out(&t);
            let (_, fall) = laid_rise_fall(&glyphs);
            assert!(
                (pad - fall).abs() <= 0.01,
                "pad {pad} pt, lowest descender {fall} pt"
            );
            let (_, box_top, box_h) = item_box(&c.source, "#text(\"");
            assert!((box_top + box_h - glyphs[0].baseline - pad).abs() <= 0.01);
            assert_contained(&t);
        }

        #[test]
        fn a_centred_baseline_does_not_follow_the_glyphs() {
            let mut rows = Vec::new();
            let mut ink = Vec::new();
            for value in ["HELIX", "Émile", "testj", "gyp"] {
                let t = label(value, "center", "20", "40", "");
                let c = compiled(&t).expect("compile");
                assert!(!c.source.contains("#pad("), "{value}: {}", c.source);
                let b = baselines(&laid_out(&t));
                assert_eq!(b.len(), 1, "{value}");
                rows.push(row(b[0]));
                ink.push((value, ink_rows(&t)));
            }
            assert!(rows.iter().all(|r| *r == rows[0]), "baseline rows {rows:?}");
            for (i, (a, ink_a)) in ink.iter().enumerate() {
                for (b, ink_b) in &ink[i + 1..] {
                    assert_ne!(ink_a, ink_b, "{a} and {b} leave the same ink gaps");
                }
            }
        }

        #[test]
        fn a_block_is_never_pulled_toward_its_aligned_edge() {
            let helix = label("HELIX", "top", "20", "20", "");
            let ace = label("ace", "top", "20", "20", "");
            let (helix_glyphs, ace_glyphs) = (laid_out(&helix), laid_out(&ace));
            assert_eq!(
                row(baselines(&helix_glyphs)[0]),
                row(baselines(&ace_glyphs)[0]),
                "HELIX and ace must share a baseline row"
            );
            let (_, box_top, _) = item_box(&compiled(&ace).expect("compile").source, "#text(\"");
            let (rise, _) = laid_rise_fall(&ace_glyphs);
            let gap = CAP_20 - rise;
            assert!(gap > 3.0, "ace inks {gap} pt below cap height");
            let scale = f64::from(DPI) / 72.0;
            let (first, _) = ink_rows(&ace);
            assert!(
                (f64::from(first) - (box_top + gap) * scale).abs() <= 1.0,
                "ace's first inked row {first} must lie {gap} pt below the top edge at row {:.2}",
                box_top * scale
            );
        }

        #[test]
        fn ink_between_the_lines_moves_nothing() {
            let extra = "    wrap: false\n";
            let top = |v: &str| {
                compiled(&label(v, "top", "20", "30", extra))
                    .expect("compile")
                    .source
            };
            let bottom = |v: &str| label(v, "bottom", "20", "30", extra);

            assert_eq!(emitted_pad(&top("Hg\nÉH"), "top"), None);
            let pad = emitted_pad(&top("ÉH\nHg"), "top").expect("a top pad");
            assert!((pad - E_ACCENT_20).abs() < 1e-3, "top pad {pad} pt");

            let t = bottom("ÉH\nHg");
            let pad = emitted_pad(&compiled(&t).expect("compile").source, "bottom")
                .expect("a bottom pad");
            let (_, g_depth) = typst_line_ink("Hg", 400);
            assert!(
                (pad - g_depth).abs() <= 0.01,
                "bottom pad {pad} pt, g falls {g_depth} pt"
            );
            let source = compiled(&bottom("Hg\nÉH")).expect("compile").source;
            assert_eq!(emitted_pad(&source, "bottom"), None, "{source}");
        }

        // ---- 8. Vertical fitting reserves the ink each alignment can expose ----

        /// Proof: refused before #392, which reserved 0.4824em that `HELIX` does not use.
        #[test]
        fn aligned_edges_are_unchanged() {
            let t = label("HELIX", "top", "20", &mm(CAP_20), "    overflow: fail\n");
            crate::render::render_single_label(&t, &HashMap::new(), &no_settings(), &no_datetime())
                .expect("HELIX renders in a box one cap height tall");
        }

        /// Proof: before #392 both values reserved the same font band and settled at one size.
        #[test]
        fn auto_shrink_sees_the_emitted_ink() {
            let range = "{ min: 10, max: 30 }";
            let helix = label("HELIX", "top", range, &mm(16.0), "");
            let accented = label("HÉLIX", "top", range, &mm(16.0), "");
            let helix_size = fitted_pt(&compiled(&helix).expect("compile").source);
            let accented_size = fitted_pt(&compiled(&accented).expect("compile").source);
            assert!(
                accented_size < helix_size,
                "HÉLIX settles at {accented_size} pt, HELIX at {helix_size} pt"
            );
            // 22 pt: the largest step with 1490/2048 × s ≤ 16.01 pt, nothing reserved.
            assert_eq!(helix_size, 22.0);
            assert_contained(&accented);
        }

        #[test]
        fn a_centred_multiline_blocks_line_budget_counts_the_reserve() {
            let height = mm(CAP_20 + 2.0 * 24.0);
            let t = |value: &str, overflow: &str| {
                label(
                    value,
                    "center",
                    "20",
                    &height,
                    &format!("    wrap: false\n    overflow: {overflow}\n"),
                )
            };
            let plain = t("HELIX\nHELIX\nHELIX", "ellipsis");
            assert_eq!(
                emitted_lines(&compiled(&plain).expect("compile").source),
                ["HELIX"; 3]
            );
            assert_contained(&plain);

            let descender = t("HELIX\nHELIX\nHELgX", "ellipsis");
            assert_eq!(
                emitted_lines(&compiled(&descender).expect("compile").source),
                ["HELIX", "HELIX..."]
            );
            assert_contained(&descender);

            let plain_fail = t("HELIX\nHELIX\nHELIX", "fail");
            assert_contained(&plain_fail);
            let Err(err) = compiled(&t("HELIX\nHELIX\nHELgX", "fail")) else {
                panic!("the descender overflows under fail");
            };
            assert_eq!(err.reason(), Some("text_does_not_fit"));
        }

        /// Proof: before #392 `Hg\nÉH` reserved the font's bands on top of its metric block.
        #[test]
        fn ink_between_the_lines_reserves_nothing() {
            let box_h = |value: &str| {
                let t = label(value, "top", "20", "content", "    wrap: false\n");
                item_box(&compiled(&t).expect("compile").source, "#text(\"").2
            };
            let metric_block_2 = CAP_20 + 24.0;
            let inner = box_h("Hg\nÉH");
            assert!(
                (inner - metric_block_2).abs() < 0.005,
                "Hg\\nÉH resolves {inner} pt"
            );

            let outer = label("ÉH\nHg", "top", "20", "content", "    wrap: false\n");
            let (_, g_depth) = typst_line_ink("Hg", 400);
            let got = item_box(&compiled(&outer).expect("compile").source, "#text(\"").2;
            let want = metric_block_2 + E_ACCENT_20 + g_depth;
            assert!(
                (got - want).abs() < 0.01,
                "ÉH\\nHg resolves {got} pt, want {want} pt"
            );
        }

        #[test]
        fn a_centred_item_asking_for_a_content_height_grows_by_the_reservation() {
            let resolved = |value: &str, vertical: &str| {
                let t = label(value, vertical, "20", "content", "");
                let c = compiled(&t).expect("compile");
                let (_, top, h) = item_box(&c.source, "#text(\"");
                (t, top, h)
            };
            for vertical in ["top", "center"] {
                let (_, _, h) = resolved("HELIX", vertical);
                assert!(
                    (h - CAP_20).abs() < 0.005,
                    "{vertical} HELIX resolves {h} pt"
                );
            }

            let (top_t, _, top_h) = resolved("Égypt", "top");
            let (rise, fall) = laid_rise_fall(&laid_out(&top_t));
            let (a, d) = (rise - CAP_20, fall);
            assert!(
                a > 4.0 && d > 3.0,
                "Égypt inks {a} pt above and {d} pt below"
            );
            assert!(
                (top_h - (CAP_20 + a + d)).abs() < 0.01,
                "top Égypt resolves {top_h} pt"
            );
            assert_contained(&top_t);

            let (centre_t, box_top, centre_h) = resolved("Égypt", "center");
            let m = a.max(d);
            assert!(
                (centre_h - (CAP_20 + 2.0 * m)).abs() < 0.01,
                "centred Égypt resolves {centre_h} pt"
            );
            let glyphs = laid_out(&centre_t);
            let ink_top = glyphs.iter().map(|g| g.top).fold(f64::MAX, f64::min);
            let ink_bottom = glyphs.iter().map(|g| g.bottom).fold(f64::MIN, f64::max);
            assert!(
                ((ink_top - box_top) - (m - a)).abs() < 0.01,
                "gap above {}",
                ink_top - box_top
            );
            assert!(
                ((box_top + centre_h - ink_bottom) - (m - d)).abs() < 0.01,
                "gap below {}",
                box_top + centre_h - ink_bottom
            );
            assert_contained(&centre_t);
        }

        #[test]
        fn an_asymmetric_font_reserves_twice_its_larger_overflow() {
            let resolved = |vertical: &str| {
                let t = label("É", vertical, "20", "content", "");
                item_box(&compiled(&t).expect("compile").source, "#text(\"").2
            };
            let (centre, top) = (resolved("center"), resolved("top"));
            assert!(
                (centre - (CAP_20 + 2.0 * E_ACCENT_20)).abs() < 0.005,
                "centred {centre} pt"
            );
            assert!((top - (CAP_20 + E_ACCENT_20)).abs() < 0.005, "top {top} pt");
        }

        /// Proof: before #392 both weights reserved the same font band.
        #[test]
        fn the_reservation_is_read_from_the_instance_rendered() {
            let resolved = |weight: u16| {
                let t = label(
                    "É",
                    "top",
                    "20",
                    "content",
                    &format!("    font_weight: {weight}\n"),
                );
                item_box(&compiled(&t).expect("compile").source, "#text(\"").2
            };
            let (regular, bold) = (resolved(400), resolved(700));
            assert!(
                bold - regular > 0.05,
                "the two instances resolve {regular} and {bold} pt"
            );
            // fontTools on the bundled font: É tops out at 1928 units at wght 400 and 1939.7 at 700.
            assert!(
                (regular - (CAP_20 + E_ACCENT_20)).abs() < 0.01,
                "wght 400 resolves {regular} pt"
            );
            let bold_accent = (1939.7 - 1490.0) / 2048.0 * 20.0;
            assert!(
                (bold - (CAP_20 + bold_accent)).abs() < 0.01,
                "wght 700 resolves {bold} pt"
            );
        }

        /// Proof: before #392 the reservation stopped at Inter's ascender and cut `Ǻ`'s top.
        #[test]
        fn a_glyph_outside_the_declared_band_still_clips() {
            let t = label("Ǻ", "top", "20", "content", "");
            crate::render::render_single_label(&t, &HashMap::new(), &no_settings(), &no_datetime())
                .expect("Ǻ renders");
            assert_contained(&t);
            let pad =
                emitted_pad(&compiled(&t).expect("compile").source, "top").expect("a top pad");
            assert!(pad > OLD_TOP_BAND_20 + 2.0, "Ǻ inset by {pad} pt");
        }

        #[test]
        fn a_mark_after_a_change_of_script_is_measured_in_its_own_segment() {
            let pad = |value: &str| {
                let t = label(value, "top", "20", "content", "");
                emitted_pad(&compiled(&t).expect("compile").source, "top").expect("a top pad")
            };
            let (mixed, alone) = (pad("αH\u{0301}"), pad("H\u{0301}"));
            assert_eq!(
                mixed, alone,
                "αH́ is inset by {mixed} pt, H́ alone by {alone} pt"
            );
            assert!((alone - E_ACCENT_20).abs() < 0.01, "H́ inset by {alone} pt");
            assert_contained(&label("αH\u{0301}", "top", "20", "content", ""));
        }

        #[test]
        fn a_character_the_font_does_not_map_is_outside_the_guarantee() {
            let t = label("H\u{4E2D}", "top", "20", "content", "    overflow: fail\n");
            crate::render::render_single_label(&t, &HashMap::new(), &no_settings(), &no_datetime())
                .expect("an unmapped character is fitted and rendered, not refused");
            // Inter's .notdef inks from −416 to 1856 units [fontTools].
            let notdef_a = (1856.0 - 1490.0) / 2048.0 * 20.0;
            let notdef_d = 416.0 / 2048.0 * 20.0;
            let h = item_box(&compiled(&t).expect("compile").source, "#text(\"").2;
            assert!(
                (h - (CAP_20 + notdef_a + notdef_d)).abs() < 0.01,
                "resolves {h} pt"
            );
        }

        /// *A centred item with headroom is unaffected*: its box fits both the font bands and the
        /// measured ink at `font_size.max`, so it resolves the same size, lines and box, and a
        /// centred block's placement never read the reservation. The literal is this template's
        /// source captured on the commit before #392 (`bf6d58a`).
        #[test]
        fn a_centred_item_with_headroom_is_unaffected() {
            const BEFORE_392: &str = "#set page(width: 120mm, height: 80mm, margin: 0mm)\n#set text(font: \"Inter\")\n#place(top + left, dx: 10mm, dy: 50mm)[#box(width: 100mm, height: 20mm, clip: true)[#align(horizon + left)[#text(size: 14pt)[#set par(leading: 6.6144543pt)\n#text(\"Widget\u{a0}42\")]]]]\n";
            let t = label("Widget 42", "center", "{ min: 8, max: 14 }", "20", "");
            assert_eq!(compiled(&t).expect("compile").source, BEFORE_392);
        }

        // ---- 9.5. The #124 raster tests, restated for the measured reservation ----

        /// Guards the other two `alignment.vertical` values (honoured literally), so a
        /// centering fix cannot hardcode centre. `test` inks nothing above cap height, so top
        /// alignment emits no inset and puts its metric top on the slot edge; its round letters
        /// overshoot the baseline, so bottom alignment is inset by exactly that overshoot as Typst
        /// lays it out. Both stay whole.
        #[test]
        fn autolength_text_top_and_bottom_pin_to_slot_edges() {
            let top = super::autolength_tape("test", false, VerticalAlign::Top, 12.0);
            let c = compiled(&top).expect("compile");
            assert_eq!(emitted_pad(&c.source, "top"), None, "{}", c.source);
            let (_, box_top, _) = item_box(&c.source, "#text(\"");
            let cap_12 = 1490.0 / 2048.0 * 12.0;
            assert!((laid_out(&top)[0].baseline - cap_12 - box_top).abs() <= 0.01);
            assert_contained(&top);

            let bottom = super::autolength_tape("test", false, VerticalAlign::Bottom, 12.0);
            let c = compiled(&bottom).expect("compile");
            let pad = emitted_pad(&c.source, "bottom").expect("a bottom pad");
            let glyphs = laid_out(&bottom);
            let (_, fall) = laid_rise_fall(&glyphs);
            assert!(
                fall > 0.0 && (pad - fall).abs() <= 0.01,
                "pad {pad} pt, overshoot {fall} pt"
            );
            let (_, box_top, box_h) = item_box(&c.source, "#text(\"");
            assert!((box_top + box_h - glyphs[0].baseline - pad).abs() <= 0.01);
            assert_contained(&bottom);
        }

        /// A slot tight enough that the metric box alone would cut accents or descenders keeps every
        /// glyph whole at a fixed size, at either aligned edge, because the inset is the ink the
        /// value carries. Judged against the unclipped reference: comparing ink against a roomier
        /// render proves nothing when both are clipped alike.
        #[test]
        fn ink_survives_a_tight_slot_at_top_and_bottom_alignment() {
            for (text, vertical) in [
                ("Édgy", VerticalAlign::Top),
                ("gjpqy", VerticalAlign::Bottom),
                ("Édgy", VerticalAlign::Bottom),
                ("gjpqy", VerticalAlign::Top),
            ] {
                assert_contained(&super::tape_of_height(text, vertical, 12.0, 5.3));
            }
        }

        /// #245's tape under the measured reservation: 21.5 pt is the largest 0.5 pt step at which
        /// the two lines broken there fit 18.1 mm (51.31 pt) with twice their larger outer ink, the
        /// depth of the `g` on the last line. At 22 pt that sum is about 51.8 pt.
        #[test]
        fn center_aligned_multiline_auto_shrink_descender_fits_and_closes_stroke() {
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
            let template = parse_and_validate(yaml).expect("valid template");
            let c = compiled(&template).expect("compile");
            assert_eq!(fitted_pt(&c.source), 21.5);
            assert_eq!(emitted_lines(&c.source).len(), 2, "{}", c.source);
            assert_contained(&template);

            // The rule, derived from what Typst lays out rather than from the measurement: at a
            // fixed size, in a box tall enough to keep every line, the block needs its metric height
            // plus twice its larger outer ink. 21.5 pt fits 18.1 mm and 22 pt does not.
            let need = |size: f64| {
                let fixed = yaml
                    .replace("height: 18.1 }", "height: 80 }")
                    .replace("size: [120, 18.1]", "size: [120, 60]")
                    .replace(
                        "font_size:\n      min: 10\n      max: 32",
                        &format!("font_size: {size}"),
                    );
                let t = parse_and_validate(&fixed).expect("valid template");
                let lines = emitted_lines(&compiled(&t).expect("compile").source).len();
                assert_eq!(lines, 2, "at {size} pt");
                let glyphs = laid_out(&t);
                // The walker reports the first line's baseline for every glyph (see `LaidGlyph`).
                let first = glyphs[0].baseline;
                let cap = 1490.0 / 2048.0 * size;
                let pitch = 1.2 * size;
                let top = glyphs.iter().map(|g| g.top).fold(f64::MAX, f64::min);
                let bottom = glyphs.iter().map(|g| g.bottom).fold(f64::MIN, f64::max);
                let a = (first - cap - top).max(0.0);
                let d = (bottom - (first + pitch)).max(0.0);
                cap + pitch + 2.0 * a.max(d)
            };
            let slot = 18.1 * 72.0 / 25.4 + 0.01;
            assert!(need(21.5) <= slot, "21.5 pt needs {} pt", need(21.5));
            assert!(need(22.0) > slot, "22 pt needs {} pt", need(22.0));
        }

        // ---- 9. Text is laid out against the box it will get ----

        /// Proof: before #392 the closed-form budget kept one line.
        #[test]
        fn a_longer_run_fits_where_the_first_line_alone_does_not() {
            let t = label(
                "g\u{0323}\nHELIX\nHELIX",
                "center",
                "20",
                &mm(28.0),
                "    wrap: false\n    line_spacing: 0.5\n    overflow: ellipsis\n",
            );
            let c =
                compiled(&t).expect("a two-line run fits although the first line alone does not");
            assert_eq!(emitted_lines(&c.source), ["g\u{0323}", "HELIX..."]);
            assert_contained(&t);
        }
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

        // The compiled label emits the same leading for an authored pitch at 0.5, 0.99, 1.2 and 1.5
        let env = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
            defaults: Default::default(),
        };
        for spacing in [0.5, 0.99, 1.2, 1.5] {
            for lines in 1..=3usize {
                let body = (0..lines).map(|_| "Hxy").collect::<Vec<_>>().join("\n");
                let template = TemplateContent {
                    name: "AuthoredBlockHeight".to_string(),
                    description: String::new(),
                    categories: Vec::new(),
                    unit: "mm".to_string(),
                    dpi: 200,
                    format: TemplateFormat::Single {
                        width: Dimension::Fixed(100.0).into(),
                        height: 100.0.into(),
                        media_width: None,
                    },
                    params: IndexMap::new(),
                    layout: Layout::Items(vec![LayoutItem::Text {
                        value: body,
                        placement: Placement::sized(
                            Position([0.0, 0.0]),
                            Size([SizeValue::fixed(100.0), SizeValue::content()]),
                        ),
                        font_size: FontSize::Fixed(20.0),
                        font_weight: None,
                        color: None,
                        wrap: false,
                        line_spacing: Some(spacing),
                        alignment: crate::models::Alignment::default(),
                        overflow: Overflow::Ellipsis,
                        when: None,
                    }]),
                };
                let compiled = super::compile_label_source(&template, &HashMap::new(), &env)
                    .expect("compile authored agreement");
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
                    "authored {lines} line(s) at pitch {spacing}: predicted {predicted:.2}pt, Typst laid out {rendered:.2}pt ({:.1}% off)",
                    drift * 100.0
                );
                let expected_leading = super::helpers::derived_leading_pt(400, 20.0, Some(spacing))
                    .expect("derived leading");
                assert!(
                    compiled
                        .source
                        .contains(&format!("#set par(leading: {expected_leading}pt)")),
                    "emitted Typst must carry leading {expected_leading}pt: got {}",
                    compiled.source
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
                categories: Vec::new(),
                unit: "mm".to_string(),
                dpi: 180,
                format: TemplateFormat::Single {
                    width: Dimension::Fixed(100.0).into(),
                    height: 60.0.into(),
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
            };
            render_single_label(&template, &HashMap::new(), &no_settings(), &no_datetime())
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(100.0).into(),
                height: 16.0.into(),
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
        };

        let env = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
            defaults: Default::default(),
        };
        let tpl_tight = make_range_template(Some(0.8));
        let compiled_tight =
            super::compile_label_source(&tpl_tight, &HashMap::new(), &env).expect("compile tight");
        let size_tight = fitted_pt(&compiled_tight.source);

        let tpl_loose = make_range_template(Some(1.5));
        let compiled_loose =
            super::compile_label_source(&tpl_loose, &HashMap::new(), &env).expect("compile loose");
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
                categories: Vec::new(),
                unit: "mm".to_string(),
                dpi: 180,
                format: TemplateFormat::Single {
                    width: Dimension::Fixed(60.0).into(),
                    height: 20.0.into(),
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
            };
            render_single_label(&template, &HashMap::new(), &no_settings(), &no_datetime())
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
                        stroke: Stroke {
                            thickness: 0.2,
                            color: Color::black(),
                        },
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
            stroke: Stroke {
                thickness: 0.2,
                color: Color::black(),
            },
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
                stroke: Stroke {
                    thickness: 0.2,
                    color: Color::black(),
                },
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(5.0)),
                    max: Some(DynamicValue::Literal(100.0)),
                },
                height: 12.0.into(),
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
                    stroke: Stroke {
                        thickness: 0.2,
                        color: Color::black(),
                    },
                    when: None,
                },
            ]),
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

    /// The render-time endpoint bound (`check_line`) is the mirror of the load-time one. Load-time validation now rejects every template that
    /// could reach it (a plain endpoint past `width.max` is rejected outright, and an edge-relative
    /// one sizes the label to its own inset), so it is exercised here at the context level.
    #[test]
    fn a_line_endpoint_outside_the_frame_errors_at_render() {
        let err = render_test_items(
            &[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([30.0, 6.0]),
                stroke: Stroke {
                    thickness: 0.2,
                    color: Color::black(),
                },
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
            height: 12.0.into(),
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

    /// Fitted sizes of the height-bound fixtures under the measured reservation (#392). Each is
    /// centred, so it reserves twice its value's larger outer ink rather than the font's bands, and
    /// a value without deep ink settles at or near its range's maximum.
    #[test]
    fn fixture_renders_reflect_new_centered_ink_reservation_numbers() {
        let (registry, _dir) = crate::templates::load_all_for_tests();

        // 1. brother_24mm_printed_on: line 1 in an 8.0mm box fits at its 24pt maximum: only the dot
        // of `i` and the overshoot of round letters leave its metric box.
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
            defaults: Default::default(),
        };
        let compiled1 =
            super::compile_label_source(printed_on, &data1, &env1).expect("compile printed_on");
        let size1 = fitted_pt(&compiled1.source);
        assert_eq!(
            size1, 24.0,
            "brother_24mm_printed_on line 1 must fit at 24pt"
        );

        // 2. brother_24mm_lines_divider: line 1 in a 7.5mm box (max 20pt) fits at 18pt, reserving
        // twice the depth of `g`.
        let lines_divider = registry
            .get("brother_24mm_lines_divider")
            .expect("lines_divider template");
        let mut data2 = HashMap::new();
        data2.insert("line1".to_string(), json!("Storage Bin A-42"));
        data2.insert("line2".to_string(), json!("Workshop / North Wall"));
        let env2 = super::RenderEnv {
            settings: &no_settings(),
            datetime: &no_datetime(),
            defaults: Default::default(),
        };
        let compiled2 = super::compile_label_source(lines_divider, &data2, &env2)
            .expect("compile lines_divider");
        let size2 = fitted_pt(&compiled2.source);
        assert_eq!(
            size2, 18.0,
            "brother_24mm_lines_divider line 1 must fit at 18pt"
        );

        // 3. brother_24mm_multiline: 2-line wrapped text in a 16.1mm box (max 32pt) fits at 19.5pt.
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
            defaults: Default::default(),
        };
        let compiled3 =
            super::compile_label_source(multiline, &data3, &env3).expect("compile multiline");
        let size3 = fitted_pt(&compiled3.source);
        assert_eq!(
            size3, 19.5,
            "brother_24mm_multiline 2-line text must fit at 19.5pt"
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
            defaults: Default::default(),
        };
        let compiled4 =
            super::compile_label_source(avery, &data4, &env4).expect("compile avery5163");
        let src4 = &compiled4.source;

        // {id} in horizontal orientation (0.35in box) fits at its 22pt maximum.
        let id_idx = src4.find("\"A1\"").expect("id text in source");
        let size4_id = fitted_pt_at(src4, id_idx);
        assert_eq!(
            size4_id, 22.0,
            "avery5163_asset_tag {{id}} must fit at 22pt"
        );

        // {name} in horizontal orientation (0.4in box) fits at its 24pt maximum.
        let name_idx = src4.find("Floor").expect("name text in source");
        let size4_name = fitted_pt_at(src4, name_idx);
        assert_eq!(
            size4_name, 24.0,
            "avery5163_asset_tag {{name}} must fit at 24pt"
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
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Literal(60.0)),
                },
                height: 12.0.into(),
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
                    default_instant: None,
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
    /// width), so the label falls back to `width.min`. That only leaves room
    /// for the item when its own `at.x` fits inside the fallback: anchored at x=30 on a 10mm label
    /// there is no box left to draw, and it errors rather than silently disappearing.
    #[test]
    fn a_to_sized_qr_anchored_past_the_fallback_width_errors() {
        let qr_at = |x: f32| TemplateContent {
            name: "T".to_string(),
            description: String::new(),
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: DynamicDimension::Dynamic {
                    min: Some(DynamicValue::Literal(10.0)),
                    max: Some(DynamicValue::Ref("target_width".to_string())),
                },
                height: 12.0.into(),
                media_width: None,
            },
            params: IndexMap::from([(
                "target_width".to_string(),
                ParamSpec {
                    param_type: ParamType::Number,
                    description: None,
                    default: Some(crate::models::ParamValue::Float(100.0)),
                    min: Some(10.0),
                    max: Some(100.0),
                    default_instant: None,
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
                error_correction: crate::models::ErrorCorrection::M,
                module_size: Some(0.5),
                quiet_zone: 0.0,
                when: None,
            }]),
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
            AppError::template_invalid(Reason::TemplateValidationFailed, err.to_string())
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
    type: number
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

    fn width_template(unit: &str, width: &str, height: f32) -> TemplateContent {
        parse_and_validate(&format!(
            r#"
name: Width
unit: {unit}
dpi: 200
params:
  - name: message
    type: string
format:
  type: single
  height: {height}
  width: {width}
layout:
  - type: text
    value: "{{message}}"
    at: [0, 0]
    size: [content, {height}]
    font_size: 8
"#
        ))
        .unwrap()
    }

    #[test]
    fn a_rendered_single_label_reports_its_resolved_width() {
        let settings = BTreeMap::new();
        let dt = resolver();
        let data = |message: &str| HashMap::from([("message".to_string(), json!(message))]);
        let render = |template: &TemplateContent, message: &str, kind: super::SingleKind| {
            let env = super::resolve_environment(template, &settings, &dt).unwrap();
            super::render_single_label_as(template, &data(message), &env, kind, Default::default())
                .unwrap()
        };

        let fixed = width_template("mm", "62.5", 18.0);
        for kind in [super::SingleKind::Png, super::SingleKind::Pdf] {
            let label = render(&fixed, "Hi", kind);
            assert_eq!((label.width_mm * 100.0).round() as i64, 6250, "{kind:?}");
        }
        // An inch template reports millimetres: 2 in = 50.8 mm.
        let inches = width_template("in", "2", 0.7);
        assert_eq!(
            (render(&inches, "Hi", super::SingleKind::Pdf).width_mm * 100.0).round() as i64,
            5080
        );

        // Content-sized: each reported width matches the PNG actually encoded, within pixel rounding.
        let fitted = width_template("mm", "{ min: 5, max: 200 }", 18.0);
        let mut widths = Vec::new();
        for message in ["Hi", "A much longer message on the tape"] {
            let label = render(&fitted, message, super::SingleKind::Png);
            let png_px = image::load_from_memory(&label.bytes).unwrap().width();
            let expected_px = label.width_mm / 25.4 * f64::from(fitted.dpi);
            assert!(
                (f64::from(png_px) - expected_px).abs() <= 1.0,
                "{message}: {png_px}px vs {expected_px}"
            );
            widths.push(label.width_mm);
        }
        assert!(widths[1] > widths[0], "{widths:?}");
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
    fn an_inactive_branch_is_neither_measured_nor_rendered() {
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
  - type: qr
    value: "{v_text}"
    at: [0, 0]
    size: [content, content]
    module_size: 0.5
    when: { orientation: v }
"#;
        let template = parse_and_validate(yaml).unwrap();
        let mut data = HashMap::new();
        data.insert("orientation".to_string(), json!("h"));
        data.insert("h_text".to_string(), json!("Horizontal only"));
        data.insert("v_text".to_string(), json!("A".repeat(8000)));

        let res = render_single_label(&template, &data, &BTreeMap::new(), &resolver());
        assert!(
            res.is_ok(),
            "should succeed because v_text is in inactive branch"
        );
    }

    /// A label holding `items` after a fixed text, with `code` and `photo` declared and never supplied.
    fn empty_content_label(items: &str) -> TemplateContent {
        let yaml = r#"
name: Empty Content
unit: mm
dpi: 200
params:
  - name: code
    type: string
  - name: photo
    type: string
format: { type: single, width: 60, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 20]
    flow: { direction: row }
    items:
      #ITEMS
      - type: text
        value: "after"
        size: [content, content]
        font_size: 8
"#
        .replace("      #ITEMS\n", items);
        parse_and_validate(&yaml).unwrap()
    }

    fn render_empty(t: &TemplateContent) -> Vec<u8> {
        render_single_label(t, &HashMap::new(), &BTreeMap::new(), &resolver()).unwrap()
    }

    #[test]
    fn an_empty_qr_takes_no_room_and_draws_nothing() {
        let qr = "      - type: qr\n        value: \"{code}\"\n        size: [content, content]\n        module_size: 0.5\n";
        assert_eq!(
            render_empty(&empty_content_label(qr)),
            render_empty(&empty_content_label(""))
        );
    }

    #[test]
    fn an_empty_image_src_draws_nothing_in_its_box() {
        let image = "      - type: image\n        src: \"{photo}\"\n        size: [15, 15]\n";
        // An authored image box keeps its room, so its reference is an empty 15 x 15 container.
        let spacer = "      - type: container\n        size: [15, 15]\n        items: []\n";
        assert_eq!(
            render_empty(&empty_content_label(image)),
            render_empty(&empty_content_label(spacer))
        );
    }

    #[test]
    fn an_absent_token_renders_as_empty_text() {
        let yaml = r#"
name: Absent Token
unit: mm
dpi: 200
params:
  - name: message
    type: string
format: { type: single, width: 60, height: 18 }
layout:
  - type: text
    value: "[{message}]"
    at: [0, 0]
    size: [60, 18]
    font_size: 10
"#;
        let template = parse_and_validate(yaml).unwrap();
        let empty = HashMap::from([("message".to_string(), json!(""))]);
        let render = |data: &HashMap<String, JsonValue>| {
            render_single_label(&template, data, &BTreeMap::new(), &resolver()).unwrap()
        };
        assert_eq!(render(&HashMap::new()), render(&empty));
    }

    #[test]
    fn an_absent_repeated_list_draws_no_instance() {
        let yaml = r#"
name: Absent Repeat
unit: mm
dpi: 200
params:
  - name: tags
    type: list
format: { type: single, width: 60, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [60, 20]
    flow: { direction: row, gap: 1 }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        items:
          - type: text
            value: "{tags}"
            size: [content, content]
            font_size: 8
"#;
        let template = parse_and_validate(yaml).unwrap();
        let empty = HashMap::from([("tags".to_string(), json!([]))]);
        let render = |data: &HashMap<String, JsonValue>| {
            render_single_label(&template, data, &BTreeMap::new(), &resolver()).unwrap()
        };
        assert_eq!(render(&HashMap::new()), render(&empty));
    }

    #[test]
    fn dimension_exceeding_max_label_dimension_returns_422() {
        let yaml = r#"
name: Dim Limit Test
unit: mm
dpi: 200
params:
  - name: target_width
    type: number
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

    /// The 1000 mm limit is a constant: a variable named like the removed setting does not lift it.
    #[test]
    fn dimension_limit_ignores_a_variable_named_max_label_dimension_mm() {
        let yaml = r#"
name: Dim Limit Variable
unit: mm
dpi: 200
params:
  - name: target_width
    type: number
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
        let data = HashMap::from([("target_width".to_string(), json!(1500.0))]);
        let variables =
            BTreeMap::from([("max_label_dimension_mm".to_string(), "5000".to_string())]);

        let res = render_single_label(&template, &data, &variables, &resolver());
        assert!(
            matches!(&res, Err(err) if err.reason() == Some("dimension_exceeds_limit")),
            "expected dimension_exceeds_limit, got {:?}",
            res.as_ref().map(|_| "rendered")
        );
    }

    #[test]
    fn dynamic_container_padding_overflow_at_runtime_returns_container_padding_no_room() {
        let yaml = r#"
name: Dynamic Container Padding Overflow
unit: mm
dpi: 200
params:
  - name: target_width
    type: number
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
    type: number
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
                Some("param_value_invalid"),
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
        let resolved = super::resolve_parameters(
            template,
            data,
            &crate::render::resolve_environment(template, &empty_vars, resolver)
                .unwrap()
                .defaults,
        )?;
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
        assert_eq!(err.code(), "TemplateInvalid");
        assert_eq!(err.reason(), Some("reference_unresolved"));
        assert_eq!(err.details().unwrap()["field"], "no_such_format");
        assert!(err.message_text().contains("no_such_format"));
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
    fn datetime_param_included_in_placeholders() {
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
            &crate::render::resolve_environment(&template, &empty_vars, &resolver)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let second = super::resolve_parameters(
            &template,
            &HashMap::new(),
            &crate::render::resolve_environment(&template, &empty_vars, &resolver)
                .unwrap()
                .defaults,
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
            let resolved = super::resolve_parameters(
                &template,
                &data,
                &crate::render::resolve_environment(
                    &template,
                    &std::collections::BTreeMap::new(),
                    &resolver,
                )
                .unwrap()
                .defaults,
            )
            .unwrap();
            let empty_settings = BTreeMap::new();
            let env = super::RenderEnv {
                settings: &empty_settings,
                datetime: &resolver,
                defaults: Default::default(),
            };
            super::RenderContext::new("mm", &resolved.data, &env, &images)
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
            super::resolve_parameters(
                &template,
                &rfc,
                &crate::render::resolve_environment(
                    &template,
                    &std::collections::BTreeMap::new(),
                    &resolver
                )
                .unwrap()
                .defaults
            )
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
        // outline declares no default, so it takes its first value and its container is active.
        // Horizontal branch must be active via its default.
        assert_eq!(data.get("outline"), Some(&json!("yes")));
        let settings = BTreeMap::new();
        let dt_resolved = crate::datetime_fmt::DateTimeResolver {
            formats: &BTreeMap::new(),
            now: dt.now,
        };
        let resolved = super::resolve_parameters(
            template,
            &data,
            &crate::render::resolve_environment(
                template,
                &std::collections::BTreeMap::new(),
                &dt_resolved,
            )
            .unwrap()
            .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &dt_resolved,
            defaults: Default::default(),
        };
        let ctx = super::RenderContext::new(&template.unit, &resolved.data, &env, &images)
            .with_instants(&resolved.instants);
        let Layout::Items(items) = &template.layout;
        assert!(
            ctx.is_item_active(&items[0]),
            "outline container must be active: an undefaulted enum takes its first value"
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
    fn parameter_resolved_authored_size_zero_errors_with_size_invalid() {
        let yaml = r#"
name: Zero Size
unit: mm
dpi: 200
params:
  - name: w
    type: number
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
    type: number
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
    type: number
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
    type: number
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
                    color: Color::black(),
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
            "#box(width: 30mm, height: 10mm, fill: none, stroke: 1mm + rgb(\"#000000\"), radius: 0mm, clip: true)"
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
                color: Color::from_rgb(255, 0, 0),
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
            src.contains("#box(width: 20mm, height: 10mm, fill: none, stroke: 0.5mm + rgb(\"#ff0000\"), radius: 0mm, clip: true)"),
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
            background: Some(Color::from_rgb(0, 0, 128)),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[bg_only], (20.0, 10.0)).expect("render bg only");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#000080\"), stroke: none, radius: 0mm, clip: true)"),
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
                color: Color::from_rgb(0, 255, 0),
            }),
            background: Some(Color::from_rgb(255, 255, 0)),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[both], (20.0, 10.0)).expect("render both");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#ffff00\"), stroke: 0.2mm + rgb(\"#00ff00\"), radius: 0mm, clip: true)"),
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
                color: Color::black(),
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
            src.contains("#box(width: 20mm, height: 10mm, fill: none, stroke: 0.2mm + rgb(\"#000000\"), radius: 5mm, clip: true)"),
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
            background: Some(Color::from_rgb(0, 0, 0)),
            rounded: Some(1.5),
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[rounded_fill_no_stroke], (20.0, 10.0))
            .expect("render rounded fill no stroke");
        assert!(
            src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#000000\"), stroke: none, radius: 1.5mm, clip: true)"),
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
            stroke: Stroke {
                thickness: 0.4,
                color: Color::from_rgb(0x80, 0, 0x80),
            },
            when: None,
        };
        let src = render_test_items(&[line], (20.0, 10.0)).expect("render line");
        assert!(
            src.contains(
                "#line(start: (0mm, 0mm), end: (10mm, -5mm), stroke: 0.4mm + rgb(\"#800080\"))"
            ),
            "got: {src}"
        );

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
                color: Color::black(),
            }),
            background: Some(Color::from_rgb(255, 0, 0)),
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
        assert!(src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#ff0000\"), stroke: 0.5mm + rgb(\"#000000\"), radius: 0mm, clip: true)["));
        assert!(src.contains("child_text"));
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
                color: Color::black(),
            }),
            background: Some(Color::from_rgb(255, 0, 0)),
            rounded: Some(2.0),
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src = render_test_items(&[rect_item], (20.0, 10.0)).expect("render rect");
        assert!(src.contains("#box(width: 20mm, height: 10mm, fill: rgb(\"#ff0000\"), stroke: 0.5mm + rgb(\"#000000\"), radius: 2mm, clip: true)[]"));
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
                color: Color::black(),
            }),
            background: Some(Color::from_rgb(0, 255, 0)),
            rounded: None,
            padding: Padding::ZERO,
            flow: None,
            repeat: None,
            items: vec![],
        };
        let src_ellipse = render_test_items(&[ellipse_item], (30.0, 20.0)).expect("render ellipse");
        assert!(src_ellipse.contains("#ellipse(width: 30mm, height: 20mm, fill: rgb(\"#00ff00\"), stroke: 0.5mm + rgb(\"#000000\"))"));
        assert!(src_ellipse.contains("#box(width: 30mm, height: 20mm, clip: true)[]"));
        assert!(
            src_ellipse.find("#ellipse").unwrap()
                < src_ellipse
                    .find("#box(width: 30mm, height: 20mm, clip: true)")
                    .unwrap(),
            "ellipse paint must precede child box"
        );

        // 3. Strokeless and fill-less ellipse emits no #ellipse, just the clip box
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

        // 4. Nested containers of mixed shapes compile and render to PNG
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
        shape: ellipse
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
            &crate::render::resolve_environment(&template_nested, &no_settings(), &no_datetime())
                .unwrap(),
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
                defaults: Default::default(),
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
            &crate::render::resolve_environment(&template_padded, &no_settings(), &no_datetime())
                .unwrap(),
            super::ImageRenderOptions::default(),
        )
        .expect("png padded");
        assert_eq!(&png[1..4], b"PNG");
        let pdf = render_single_label_pdf(
            &template_padded,
            &HashMap::new(),
            &crate::render::resolve_environment(&template_padded, &no_settings(), &no_datetime())
                .unwrap(),
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
            &crate::render::resolve_environment(&template_square, &no_settings(), &no_datetime())
                .unwrap(),
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
    background: '#00ff00'
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
            &crate::render::resolve_environment(&template, &settings, &datetime).unwrap(),
            super::ImageRenderOptions::default(),
        )
        .expect("render png");
        assert!(!png.is_empty());
        assert_eq!(&png[1..4], b"PNG");

        let pdf = render_single_label_pdf(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime).unwrap(),
        )
        .expect("render pdf");
        assert!(!pdf.is_empty());
        assert_eq!(&pdf[0..4], b"%PDF");
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
            content: template_content,
        };
        let labels = vec![LabelInput {
            data: HashMap::new(),
        }];
        let pdf = render_sheet_pages(
            &template,
            &labels,
            0,
            &crate::render::resolve_environment(&template, &no_settings(), &no_datetime()).unwrap(),
        )
        .unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    /// Render the four catalog tapes and confirm they are unchanged from the baseline.
    #[test]
    fn catalog_brother_tapes_render_unchanged_from_baseline() {
        let (registry, _dir) = crate::templates::load_all_for_tests();
        let baseline_dir = std::path::Path::new("tests/fixtures/renders");

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

            let baseline_path = baseline_dir.join(format!("{tape_id}.png"));
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
            defaults: Default::default(),
        };

        // When bold and mode are omitted (no defaults), bold is false, so its branch selects, and
        // the absent mode's branch does not
        let resolved_omitted = super::resolve_parameters(
            &template,
            &HashMap::new(),
            &crate::render::resolve_environment(
                &template,
                &std::collections::BTreeMap::new(),
                &res_dt,
            )
            .unwrap()
            .defaults,
        )
        .unwrap();
        let ctx = super::RenderContext::new("mm", &resolved_omitted.data, &env, &images)
            .with_instants(&resolved_omitted.instants);
        assert!(
            ctx.is_item_active(&items[0]),
            "when: {{ bold: 'false' }} must select when bold is omitted: a boolean defaults to false"
        );
        assert!(
            !ctx.is_item_active(&items[1]),
            "when: {{ mode: draft }} must not select when mode is omitted"
        );

        // When bold: false is explicitly provided, bold branch selects
        let mut with_bold_false = HashMap::new();
        with_bold_false.insert("bold".to_string(), json!(false));
        let resolved_bf = super::resolve_parameters(
            &template,
            &with_bold_false,
            &crate::render::resolve_environment(
                &template,
                &std::collections::BTreeMap::new(),
                &res_dt,
            )
            .unwrap()
            .defaults,
        )
        .unwrap();
        let ctx_bf = super::RenderContext::new("mm", &resolved_bf.data, &env, &images)
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
            defaults: Default::default(),
        };
        let resolved1 = super::resolve_parameters(
            &template1,
            &HashMap::new(),
            &crate::render::resolve_environment(
                &template1,
                &std::collections::BTreeMap::new(),
                &res_dt,
            )
            .unwrap()
            .defaults,
        )
        .unwrap();
        let ctx1 = super::RenderContext::new("mm", &resolved1.data, &env, &images)
            .with_instants(&resolved1.instants);
        assert!(
            ctx1.is_item_active(&items1[0]),
            "default: draft must select when: mode: draft"
        );

        // 2. Tokened default that fails resolution -> fails render with reference_unresolved
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
        assert_eq!(err.code(), "TemplateInvalid");
        assert_eq!(err.reason(), Some("reference_unresolved"));
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
        assert_eq!(err.code(), "TemplateInvalid");
        assert_eq!(err.reason(), Some("reference_unresolved"));
        assert_eq!(err.details().unwrap()["field"], "vars.missing");
    }

    #[test]
    fn a_suffixed_length_default_is_refused_at_load() {
        let yaml = r#"
name: Test Suffixed Default
unit: mm
dpi: 200
params:
  - name: w
    type: number
    default: "80mm"
format: { type: single, width: 100, height: 20 }
layout:
  - type: text
    value: "{w}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(
            err.message_text().contains("params.w.default"),
            "{}",
            err.message_text()
        );
    }

    #[test]
    fn a_null_string_value_is_omitted_and_takes_the_default() {
        let yaml = r#"
name: Test String Null
unit: mm
dpi: 200
params:
  - name: title
    type: string
    default: Untitled
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
        assert_eq!(s, "Prefix:Untitled:Suffix");
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
        let pdf = render_sheet_pages(
            template,
            &labels,
            0,
            &crate::render::resolve_environment(template, &BTreeMap::new(), &resolver()).unwrap(),
        )
        .unwrap();
        assert!(!pdf.is_empty());

        let Layout::Items(items) = &template.layout;
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let res_dt = resolver();
        let empty_settings = BTreeMap::new();
        let env = super::RenderEnv {
            settings: &empty_settings,
            datetime: &res_dt,
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            template,
            &data,
            &crate::render::resolve_environment(
                template,
                &std::collections::BTreeMap::new(),
                &res_dt,
            )
            .unwrap()
            .defaults,
        )
        .unwrap();
        let ctx = super::RenderContext::new("in", &resolved.data, &env, &images)
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
            color: Some(crate::models::Color::from_str("red").unwrap()),
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let src_named = render_test_items(&[named_item], (50.0, 20.0)).expect("render named");
        assert!(
            src_named.contains("fill: rgb(\"#ff0000\")"),
            "red must emit rgb(\"#ff0000\"), got: {src_named}"
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
            color: Some(crate::models::Color::from_str("#ff4136").unwrap()),
            wrap: false,
            line_spacing: None,
            alignment: Alignment::default(),
            overflow: Overflow::Ellipsis,
            when: None,
        };
        let src_hex = render_test_items(&[hex_item], (50.0, 20.0)).expect("render hex");
        assert!(
            src_hex.contains("fill: rgb(\"#ff4136\")"),
            "#ff4136 must emit rgb(\"#ff4136\"), got: {src_hex}"
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
    fn text_color_absent_renders_black_e2e() {
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
            defaults: Default::default(),
        };
        let data = HashMap::new();
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
            color: color.map(|s| crate::models::Color::from_str(s).unwrap()),
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
            defaults: Default::default(),
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);
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
            defaults: Default::default(),
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let data = HashMap::new();
        let geometry = HashMap::new();
        let ctx = super::RenderContext::new("mm", &data, &env, &images);
        let (meas, _) = ctx
            .measure_items(items, (50.0, 30.0), [true, true], &geometry, "layout")
            .unwrap();
        let src = ctx
            .render_items(items, &meas, (50.0, 30.0), &geometry, None, "layout")
            .unwrap();

        // Both container #box and child #text emit exact same rgb("#ff0000")
        assert!(
            src.contains("#box(width: 50mm, height: 30mm, fill: rgb(\"#ff0000\")"),
            "container box must carry rgb(\"#ff0000\"), got: {src}"
        );
        assert!(
            src.contains("#text(size: 10pt, fill: rgb(\"#ff0000\"))"),
            "text must carry rgb(\"#ff0000\"), got: {src}"
        );

        // 2. Each colour name emits its stated value, not the rendering engine's constant
        for (name, expected_hex) in [
            ("black", "#000000"),
            ("white", "#ffffff"),
            ("red", "#ff0000"),
            ("green", "#008000"),
            ("blue", "#0000ff"),
        ] {
            let item = LayoutItem::Text {
                value: "Test".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(20.0), SizeValue::fixed(10.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: Some(name.parse().unwrap()),
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
            defaults: Default::default(),
        };
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &data, &env, &images);
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
                        default_instant: None,
                    },
                );
                TemplateContent {
                    name: "Test".to_string(),
                    description: String::new(),
                    categories: Vec::new(),
                    unit: "mm".to_string(),
                    dpi: 200,
                    format: crate::models::TemplateFormat::Single {
                        width: crate::models::Dimension::Fixed(50.0).into(),
                        height: 20.0.into(),
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
            super::resolve_parameters(&tpl, &submitted, &Default::default())
        };

        // 1. String
        let err = run_strict(
            "title",
            crate::models::ParamType::String { multiline: false },
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            r#"invalid value for parameter 'title': ["foo","bar"] is not a string"#
        );

        // 2. Boolean
        let err = run_strict("flag", crate::models::ParamType::Boolean).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            r#"invalid value for parameter 'flag': ["foo","bar"] is not a boolean"#
        );

        // 3. Integer
        let err = run_strict("count", crate::models::ParamType::Integer).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            r#"invalid value for parameter 'count': ["foo","bar"] is not an integer"#
        );

        // 4. Number
        let err = run_strict("ratio", crate::models::ParamType::Number).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            r#"invalid value for parameter 'ratio': ["foo","bar"] is not a number"#
        );

        // 5. Enum
        let err = run_strict(
            "tier",
            crate::models::ParamType::Enum {
                values: vec!["A".to_string(), "B".to_string()],
            },
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));

        // 6. Datetime
        let err = run_strict(
            "created_at",
            crate::models::ParamType::Datetime { time: false },
        )
        .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            r#"invalid value for parameter 'created_at': ["foo","bar"] is not a datetime string"#
        );

        // 7. List with non-array value
        let tpl_list = tpl_with_param("tags", crate::models::ParamType::List);
        let mut submitted_str = HashMap::new();
        submitted_str.insert("tags".to_string(), serde_json::json!("not_an_array"));
        let err =
            super::resolve_parameters(&tpl_list, &submitted_str, &Default::default()).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            r#"invalid value for parameter 'tags': "not_an_array" is not a list of strings"#
        );

        // 8. List with non-string element
        let mut submitted_bad_elem = HashMap::new();
        submitted_bad_elem.insert("tags".to_string(), serde_json::json!(["ok", 42]));
        let err = super::resolve_parameters(&tpl_list, &submitted_bad_elem, &Default::default())
            .unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("param_value_invalid"));
        assert_eq!(
            err.message_text(),
            "invalid value for parameter 'tags': element at position 1 is not a string: 42"
        );
    }

    #[test]
    fn an_enum_value_outside_values_is_param_value_invalid() {
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
                default_instant: None,
            },
        );
        let template = TemplateContent {
            name: "Test".to_string(),
            description: String::new(),
            categories: Vec::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: crate::models::TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(50.0).into(),
                height: 20.0.into(),
                media_width: None,
            },
            params,
            layout: crate::models::Layout::Items(vec![]),
        };
        let mut submitted = HashMap::new();
        submitted.insert("orientation".to_string(), serde_json::json!("sideways"));
        let err =
            super::resolve_parameters(&template, &submitted, &Default::default()).unwrap_err();
        assert_eq!(err.status().as_u16(), 400);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(
            err.details(),
            Some(&serde_json::json!({ "reason": "param_value_invalid", "param": "orientation" }))
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
            defaults: Default::default(),
        };

        // 1. Over explicit empty list []
        let data_empty = HashMap::from([("tags".to_string(), serde_json::json!([]))]);
        let resolved_empty = super::resolve_parameters(
            &template,
            &data_empty,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved_empty.data, &env, &images);
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
        let resolved_def = super::resolve_parameters(
            &template,
            &data_omitted,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images_def = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx_def = super::RenderContext::new("mm", &resolved_def.data, &env, &images_def);
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
            defaults: Default::default(),
        };
        let resolved = super::resolve_parameters(
            &template,
            &data,
            &crate::render::resolve_environment(&template, &settings, &datetime)
                .unwrap()
                .defaults,
        )
        .unwrap();
        let images = std::cell::RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new("mm", &resolved.data, &env, &images);
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
}

/// What a label prints for the values and defaults of its parameters (#413). Assertions read the
/// `#text("…")` lines of the Typst source a label compiles to.
#[cfg(test)]
mod parameter_value_tests {
    use crate::errors::AppError;
    use crate::templates::TemplateContent;
    use serde_json::{json, Value as JsonValue};
    use std::collections::{BTreeMap, HashMap};

    fn load(yaml: &str) -> TemplateContent {
        let content = crate::parse::parse_template(yaml).expect("parse template");
        content.validate().expect("validate template");
        content
    }

    /// Every `#text("…")` line in `source`, unescaped.
    fn text_lines(source: &str) -> Vec<String> {
        let mut lines = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find("#text(\"") {
            rest = &rest[at + "#text(\"".len()..];
            let mut line = String::new();
            let mut chars = rest.char_indices();
            let mut end = rest.len();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, escaped)) = chars.next() {
                            line.push(match escaped {
                                'n' => '\n',
                                other => other,
                            });
                        }
                    }
                    '"' => {
                        end = i;
                        break;
                    }
                    other => line.push(other),
                }
            }
            lines.push(line.replace('\u{00A0}', " "));
            rest = &rest[end..];
        }
        lines
    }

    /// The lines one label of `template` prints, rendered like a single request against `vars` and
    /// the seeded datetime formats.
    fn printed(
        template: &TemplateContent,
        data: JsonValue,
        vars: &[(&str, &str)],
    ) -> Result<Vec<String>, AppError> {
        let data: HashMap<String, JsonValue> = serde_json::from_value(data).expect("data object");
        let vars: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let formats = crate::settings::default_datetime_formats();
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now: chrono::Local::now(),
        };
        let env = super::resolve_environment(template, &vars, &datetime)?;
        let compiled = super::compile_label_source(template, &data, &env)?;
        Ok(text_lines(&compiled.source))
    }

    /// The lines a template's thumbnail prints, with no variables set.
    fn thumbnail_printed(template: &TemplateContent) -> Result<Vec<String>, AppError> {
        let vars = BTreeMap::new();
        let formats = crate::settings::default_datetime_formats();
        let now = chrono::Local::now();
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now,
        };
        let data = template.placeholder_data(now);
        let env = super::resolve_environment(template, &vars, &datetime)?;
        let compiled = super::compile_label_source(template, &data, &env)?;
        Ok(text_lines(&compiled.source))
    }

    /// A printed result in a comparable form: the lines, or the error's reason and message.
    fn shown(result: Result<Vec<String>, AppError>) -> Result<Vec<String>, String> {
        result.map_err(|e| format!("{:?}: {}", e.reason(), e.message_text()))
    }

    fn lines(expected: &[&str]) -> Result<Vec<String>, String> {
        Ok(expected.iter().map(|s| s.to_string()).collect())
    }

    fn single(params: &str, layout: &str) -> String {
        format!(
            "name: T\nunit: mm\ndpi: 100\nparams:\n{params}\nformat: {{ type: single, width: 80, height: 40 }}\nlayout:\n{layout}\n"
        )
    }

    fn text_at(value: &str, y: u32) -> String {
        format!(
            "  - type: text\n    value: \"{value}\"\n    at: [0, {y}]\n    size: [80, 8]\n    font_size: 6\n"
        )
    }

    // A3
    #[test]
    fn a_number_prints_as_supplied_and_as_declared() {
        let supplied = load(&single(
            "  - name: price\n    type: number\n",
            &text_at("{price}", 0),
        ));
        let defaulted = load(&single(
            "  - name: price\n    type: number\n    default: 1234.5678\n",
            &text_at("{price}", 0),
        ));
        assert_eq!(
            [
                shown(printed(&supplied, json!({ "price": 1234.5678 }), &[])),
                shown(printed(&defaulted, json!({}), &[])),
            ],
            [lines(&["1234.5678"]), lines(&["1234.5678"])],
            "[supplied, defaulted]"
        );
    }

    // A4
    #[test]
    fn a_padded_number_default_drives_geometry() {
        let yaml = single(
            "  - name: w\n    type: number\n    default: \" 12 \"\n",
            "  - type: text\n    value: \"box\"\n    at: [0, 0]\n    size: [\"{w}\", 10]\n    font_size: 6\n",
        );
        let template = crate::parse::parse_template(&yaml).expect("parse template");
        template
            .validate()
            .expect("a default of \" 12 \" must load as the width 12");
        let data = HashMap::new();
        let vars = BTreeMap::new();
        let formats = crate::settings::default_datetime_formats();
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now: chrono::Local::now(),
        };
        let env = super::RenderEnv {
            settings: &vars,
            datetime: &datetime,
            defaults: Default::default(),
        };
        let source = super::compile_label_source(&template, &data, &env)
            .expect("render")
            .source;
        assert!(
            source.contains("width: 12mm"),
            "expected a 12mm box: {source}"
        );
    }

    // A4a
    #[test]
    fn a_datetime_default_keeps_its_time() {
        let layout = text_at("{printed_on}", 0) + &text_at("{printed_on:time}", 10);
        let literal = load(&single(
            "  - name: printed_on\n    type: datetime\n    default: \"2026-08-19T14:30\"\n",
            &layout,
        ));
        let tokened = load(&single(
            "  - name: printed_on\n    type: datetime\n    default: \"{vars.when}\"\n",
            &layout,
        ));
        let expected = lines(&["2026-08-19", "14:30"]);
        assert_eq!(
            [
                shown(printed(
                    &literal,
                    json!({ "printed_on": "2026-08-19T14:30" }),
                    &[]
                )),
                shown(printed(&literal, json!({}), &[])),
                shown(printed(
                    &tokened,
                    json!({}),
                    &[("when", "2026-08-19T14:30")]
                )),
            ],
            [expected.clone(), expected.clone(), expected],
            "[supplied, literal default, tokened default]"
        );
    }

    // A9
    #[test]
    fn an_omitted_boolean_without_a_default_is_false() {
        let template = load(&single(
            "  - name: bold\n    type: boolean\n",
            "  - type: container\n    when: { bold: false }\n    at: [0, 0]\n    size: [80, 10]\n    items:\n      - type: text\n        value: \"plain\"\n        at: [0, 0]\n        size: [80, 8]\n        font_size: 6\n",
        ));
        assert_eq!(printed(&template, json!({}), &[]).unwrap(), vec!["plain"]);
    }

    // A13
    #[test]
    fn null_and_blank_are_omissions_except_for_a_string() {
        let template = load(&single(
            "  - name: tags\n    type: list\n    default: [CONSUMABLE]\n  - name: copies\n    type: integer\n    default: 1\n  - name: title\n    type: string\n    default: Untitled\n",
            &(text_at("{tags:join(',')}", 0) + &text_at("{copies}", 10) + &text_at("[{title}]", 20)),
        ));
        assert_eq!(
            [
                shown(printed(
                    &template,
                    json!({ "tags": null, "copies": "", "title": "" }),
                    &[]
                )),
                shown(printed(&template, json!({ "tags": "" }), &[])),
            ],
            [
                lines(&["CONSUMABLE", "1", "[]"]),
                lines(&["CONSUMABLE", "1", "[Untitled]"])
            ],
            "[null/blank/empty, blank list]"
        );
    }

    // A15
    #[test]
    fn a_token_in_a_list_default_is_literal() {
        let template = load(&single(
            "  - name: tags\n    type: list\n    default: [\"{vars.brand}\"]\n",
            &text_at("{tags:join(',')}", 0),
        ));
        assert_eq!(
            printed(&template, json!({}), &[]).unwrap(),
            vec!["{vars.brand}"]
        );
    }

    // A16
    #[test]
    fn thumbnail_placeholders_follow_the_type_table() {
        let bounded = load(&single(
            "  - name: qty\n    type: integer\n    min: 5\n    max: 10\n",
            &text_at("{qty}", 0),
        ));
        let unbounded = load(&single(
            "  - name: qty\n    type: integer\n",
            &text_at("{qty}", 0),
        ));
        let gated = load(&single(
            "  - name: mode\n    type: enum\n    values: [first, second]\n",
            "  - type: container\n    when: { mode: first }\n    at: [0, 0]\n    size: [80, 10]\n    items:\n      - type: text\n        value: \"gated\"\n        at: [0, 0]\n        size: [80, 8]\n        font_size: 6\n",
        ));
        assert_eq!(
            [
                shown(thumbnail_printed(&bounded)),
                shown(thumbnail_printed(&unbounded)),
                shown(thumbnail_printed(&gated)),
            ],
            [lines(&["10"]), lines(&["42"]), lines(&["gated"])],
            "[bounded, unbounded, gate-only enum]"
        );
    }
}
