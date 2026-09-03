use crate::errors::AppError;
use crate::models::{
    Alignment, FontSize, HorizontalAlign, Overflow, Point, QrParams, VerticalAlign,
};
use crate::reason::Reason;
use base64::Engine as _;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;

/// In-place global luminance threshold of premultiplied-RGBA bytes to pure black/white (slice 1: no
/// dithering). Typst pages render opaque (alpha 255), so premultiplied == straight and Rec.601 luma is
/// correct. Threshold 128 = 0.5.
pub(super) fn binarize_rgba(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0.iter_mut() {
        let luma = (77 * px[0] as u32 + 150 * px[1] as u32 + 29 * px[2] as u32) >> 8;
        let v = if luma < 128 { 0u8 } else { 255u8 };
        px[0] = v;
        px[1] = v;
        px[2] = v;
        px[3] = 255;
    }
}

pub fn value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => value.clone(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

fn process_literal_chunk(chunk: &str, template: &str, out: &mut String) -> Result<(), AppError> {
    let mut chars = chunk.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                } else {
                    return Err(AppError::invalid_request(
                        Reason::InterpolationSyntax,
                        format!("unterminated '{{' in template '{template}'"),
                    ));
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    out.push('}');
                } else {
                    return Err(AppError::invalid_request(
                        Reason::InterpolationSyntax,
                        format!("unmatched '}}' in template '{template}'"),
                    ));
                }
            }
            other => out.push(other),
        }
    }
    Ok(())
}

/// Substitution-only interpolation (ADR-0010, ADR-0055).
///
/// - `{sys.now[:<fmt>]}` resolves the request's captured instant.
/// - `{vars.<key>}` resolves from `variables`.
/// - `{<name>[:<fmt>]}` resolves from declared parameter `instants` (if datetime parameter) or `data`.
///
/// `{{`/`}}` emit literal braces. An unresolved token or an unmatched brace is an error.
pub(super) fn interpolate(
    template: &str,
    data: &HashMap<String, JsonValue>,
    variables: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
    instants: Option<&BTreeMap<String, chrono::DateTime<chrono::Local>>>,
) -> Result<String, AppError> {
    let mut out = String::with_capacity(template.len());
    let tokens = crate::interpolation::scan_tokens(template);
    let mut pos = 0;

    for scanned in tokens {
        if scanned.start > pos {
            process_literal_chunk(&template[pos..scanned.start], template, &mut out)?;
        }
        pos = scanned.end;

        let token = crate::interpolation::parse(scanned.raw).map_err(|_| {
            AppError::invalid_request(
                Reason::InterpolationSyntax,
                format!(
                    "invalid interpolation token '{}' in template '{template}'",
                    scanned.raw
                ),
            )
        })?;

        let inner = scanned
            .raw
            .strip_prefix('{')
            .unwrap_or(scanned.raw)
            .strip_suffix('}')
            .unwrap_or(scanned.raw);

        let resolved = match token.source {
            crate::interpolation::Source::Sys(crate::interpolation::SysValue::Now) => {
                match token.reader {
                    Some(crate::interpolation::Reader::Format(fmt)) => {
                        datetime.format(datetime.now, Some(fmt), inner)?
                    }
                    Some(crate::interpolation::Reader::Join(_)) => {
                        return Err(AppError::field_value_not_scalar(inner));
                    }
                    None => datetime.format(datetime.now, None, inner)?,
                }
            }
            crate::interpolation::Source::Vars(key) => {
                if token.reader.is_some() {
                    return Err(AppError::missing_field(inner));
                }
                variables
                    .get(key)
                    .cloned()
                    .ok_or_else(|| AppError::missing_field(&format!("vars.{key}")))?
            }
            crate::interpolation::Source::Bare(name) => {
                if let Some(instant) = instants.and_then(|inst| inst.get(name)) {
                    match token.reader {
                        Some(crate::interpolation::Reader::Format(fmt)) => {
                            datetime.format(*instant, Some(fmt), inner)?
                        }
                        Some(crate::interpolation::Reader::Join(_)) => {
                            return Err(AppError::missing_field(name));
                        }
                        None => datetime.format(*instant, None, inner)?,
                    }
                } else {
                    let val = data
                        .get(name)
                        .ok_or_else(|| AppError::missing_field(name))?;
                    match token.reader {
                        Some(crate::interpolation::Reader::Join(sep)) => match val {
                            JsonValue::Array(arr) => {
                                let mut joined = String::new();
                                for (i, elem) in arr.iter().enumerate() {
                                    if i > 0 {
                                        joined.push_str(sep);
                                    }
                                    match elem {
                                        JsonValue::String(s) => joined.push_str(s),
                                        _ => return Err(AppError::field_value_not_scalar(name)),
                                    }
                                }
                                joined
                            }
                            _ => return Err(AppError::field_value_not_scalar(name)),
                        },
                        Some(crate::interpolation::Reader::Format(_)) => {
                            return Err(AppError::missing_field(name));
                        }
                        None => match val {
                            JsonValue::Array(_) => {
                                return Err(AppError::field_value_not_scalar(name));
                            }
                            other => value_to_string(other),
                        },
                    }
                }
            }
        };
        out.push_str(&resolved);
    }

    if pos < template.len() {
        process_literal_chunk(&template[pos..], template, &mut out)?;
    }

    Ok(out)
}

pub(super) fn resolve_dynamic_value_f32(
    dyn_val: &crate::models::DynamicValue<f32>,
    data: &HashMap<String, JsonValue>,
) -> Result<f32, AppError> {
    match dyn_val {
        crate::models::DynamicValue::Literal(v) => Ok(*v),
        crate::models::DynamicValue::Ref(name) => {
            let val = data
                .get(name)
                .ok_or_else(|| AppError::missing_field(name))?;
            match val {
                JsonValue::Number(n) => n.as_f64().map(|f| f as f32).ok_or_else(|| {
                    AppError::invalid_request(
                        Reason::RequestBodyInvalid,
                        format!("parameter '{name}' is not a valid number"),
                    )
                }),
                JsonValue::String(s) => {
                    let trimmed = s.trim();
                    let num_str = trimmed
                        .strip_suffix("mm")
                        .or_else(|| trimmed.strip_suffix("in"))
                        .unwrap_or(trimmed);
                    num_str.trim().parse::<f32>().map_err(|_| {
                        AppError::invalid_request(
                            Reason::RequestBodyInvalid,
                            format!("parameter '{name}' is not a valid number"),
                        )
                    })
                }
                _ => Err(AppError::invalid_request(
                    Reason::RequestBodyInvalid,
                    format!("parameter '{name}' is not a valid number"),
                )),
            }
        }
    }
}

pub(super) fn resolve_dynamic_value_u16(
    dyn_val: &crate::models::DynamicValue<u16>,
    data: &HashMap<String, JsonValue>,
) -> Result<u16, AppError> {
    match dyn_val {
        crate::models::DynamicValue::Literal(v) => Ok(*v),
        crate::models::DynamicValue::Ref(name) => {
            let val = data
                .get(name)
                .ok_or_else(|| AppError::missing_field(name))?;
            match val {
                JsonValue::Number(n) => n
                    .as_u64()
                    .map(|u| u as u16)
                    .or_else(|| n.as_f64().map(|f| f.round() as u16))
                    .ok_or_else(|| {
                        AppError::invalid_request(
                            Reason::RequestBodyInvalid,
                            format!("parameter '{name}' is not a valid integer"),
                        )
                    }),
                JsonValue::String(s) => s.trim().parse::<u16>().map_err(|_| {
                    AppError::invalid_request(
                        Reason::RequestBodyInvalid,
                        format!("parameter '{name}' is not a valid integer"),
                    )
                }),
                _ => Err(AppError::invalid_request(
                    Reason::RequestBodyInvalid,
                    format!("parameter '{name}' is not a valid integer"),
                )),
            }
        }
    }
}

pub(super) fn resolve_dynamic_value_color(
    dyn_val: &crate::models::DynamicValue<crate::models::Color>,
    data: &HashMap<String, JsonValue>,
) -> Result<crate::models::Color, AppError> {
    match dyn_val {
        crate::models::DynamicValue::Literal(color) => Ok(color.clone()),
        crate::models::DynamicValue::Ref(name) => {
            let val = data
                .get(name)
                .ok_or_else(|| AppError::color_param_invalid(name, "parameter was not supplied"))?;
            let s = match val {
                JsonValue::String(s) => s.trim(),
                _ => {
                    return Err(AppError::color_param_invalid(
                        name,
                        "expected a colour string",
                    ))
                }
            };
            if s.starts_with('{') && s.ends_with('}') && s.len() >= 2 {
                return Err(AppError::color_param_invalid(
                    name,
                    "references cannot be chained",
                ));
            }
            s.parse::<crate::models::Color>().map_err(|_| {
                AppError::color_param_invalid(name, format!("unrecognised colour '{s}'"))
            })
        }
    }
}

pub(super) fn resolve_dimension(
    dimension: &crate::models::DynamicDimension,
    data: &HashMap<String, JsonValue>,
) -> Result<f32, AppError> {
    match dimension {
        crate::models::DynamicDimension::Fixed(dyn_val) => resolve_dynamic_value_f32(dyn_val, data),
        crate::models::DynamicDimension::Dynamic { min, max } => {
            let max_val = max
                .as_ref()
                .map(|v| resolve_dynamic_value_f32(v, data))
                .transpose()?;
            let min_val = min
                .as_ref()
                .map(|v| resolve_dynamic_value_f32(v, data))
                .transpose()?;
            max_val
                .or(min_val)
                .ok_or_else(|| AppError::unsupported_format("dynamic dimension missing min/max"))
        }
    }
}

pub(super) fn format_length(value: f32, unit: &str) -> Result<String, AppError> {
    let unit = match unit {
        "mm" | "in" => unit,
        _ => return Err(AppError::unsupported_format("unknown unit")),
    };
    Ok(format!("{}{}", format_float(value), unit))
}

fn format_float(value: f32) -> String {
    let mut s = format!("{value:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
}

pub(super) fn escape_typst_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn to_nonbreaking(value: &str) -> String {
    value.replace(' ', "\u{00A0}")
}

pub(super) fn build_qr_svg(payload: &[u8], params: &Option<QrParams>) -> Result<String, AppError> {
    let ecc = params
        .as_ref()
        .and_then(|params| params.error_correction.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .map(|value| match value.as_str() {
            "L" => Ok(EcLevel::L),
            "M" => Ok(EcLevel::M),
            "Q" => Ok(EcLevel::Q),
            "H" => Ok(EcLevel::H),
            _ => Err(AppError::unsupported_layout_item(
                Reason::QrErrorCorrectionInvalid,
                "qr error_correction must be one of L, M, Q, H",
            )),
        })
        .transpose()?
        .unwrap_or(EcLevel::M);

    let code = QrCode::with_error_correction_level(payload, ecc).map_err(|err| {
        AppError::render_failed(
            Reason::QrGenerationFailed,
            format!("qr generation failed: {err}"),
        )
    })?;

    let qz = params
        .as_ref()
        .and_then(|params| params.quiet_zone)
        .unwrap_or(0.0);

    let mut renderer = code.render::<svg::Color>();
    renderer.quiet_zone(false);
    let svg = renderer.build();

    if qz > 0.0 {
        let w = code.width() as f32;
        let total = w + 2.0 * qz;
        let neg_qz = -qz;
        let old_vb = format!("viewBox=\"0 0 {w} {w}\"");
        let new_vb = format!("viewBox=\"{neg_qz} {neg_qz} {total} {total}\"");
        if svg.contains(&old_vb) {
            Ok(svg.replace(&old_vb, &new_vb))
        } else {
            let re = regex::Regex::new(r#"viewBox="0 0 \d+ \d+""#).unwrap();
            Ok(re.replace(&svg, new_vb.as_str()).into_owned())
        }
    } else {
        Ok(svg)
    }
}

pub(super) fn raster_image_dimensions(
    bytes: &[u8],
    fmt: ImageFmt,
    dpi: u32,
    unit: &str,
    path: &str,
) -> Result<(f32, f32), AppError> {
    let format = match fmt {
        ImageFmt::Png => image::ImageFormat::Png,
        ImageFmt::Jpg => image::ImageFormat::Jpeg,
        ImageFmt::Svg => unreachable!("svg is not a raster format"),
    };
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes));
    reader.set_format(format);
    let (px_w, px_h) = reader.into_dimensions().map_err(|e| {
        AppError::unsupported_layout_item(
            Reason::IntrinsicSizeUndefined,
            format!("at {path}: failed to read image dimensions: {e}"),
        )
    })?;

    let scale = if unit == "in" {
        1.0 / (dpi as f32)
    } else {
        25.4 / (dpi as f32)
    };

    Ok((px_w as f32 * scale, px_h as f32 * scale))
}

pub(super) fn svg_axis_intrinsic(
    svg_str: &str,
    axis: usize, // 0 = width, 1 = height
    unit: &str,
    dpi: u32,
    path: &str,
) -> Result<f32, AppError> {
    let root_svg_re = regex::Regex::new(r#"<svg\b([^>]*)>"#).unwrap();
    let Some(caps) = root_svg_re.captures(svg_str) else {
        return Err(AppError::unsupported_layout_item(
            Reason::IntrinsicSizeUndefined,
            format!("at {path}: svg root tag not found"),
        ));
    };
    let attrs = caps.get(1).map_or("", |m| m.as_str());

    let attr_name = if axis == 0 { "width" } else { "height" };
    let attr_re = regex::Regex::new(&format!(r#"\b{attr_name}\s*=\s*["']([^"']+)["']"#)).unwrap();

    let scale_px = if unit == "in" {
        1.0 / (dpi as f32)
    } else {
        25.4 / (dpi as f32)
    };

    if let Some(c) = attr_re.captures(attrs) {
        let val_str = c.get(1).unwrap().as_str().trim();
        if !val_str.ends_with('%')
            && !val_str.ends_with("em")
            && !val_str.ends_with("rem")
            && !val_str.ends_with("ex")
            && !val_str.ends_with("ch")
        {
            if let Some(num_str) = val_str.strip_suffix("in") {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    let val_in_unit = if unit == "in" { num } else { num * 25.4 };
                    return Ok(val_in_unit);
                }
            } else if let Some(num_str) = val_str.strip_suffix("mm") {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    let val_in_unit = if unit == "mm" { num } else { num / 25.4 };
                    return Ok(val_in_unit);
                }
            } else if let Some(num_str) = val_str.strip_suffix("cm") {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    let num_mm = num * 10.0;
                    let val_in_unit = if unit == "mm" { num_mm } else { num_mm / 25.4 };
                    return Ok(val_in_unit);
                }
            } else if let Some(num_str) = val_str.strip_suffix("pt") {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    let val_in_unit = if unit == "in" {
                        num / 72.0
                    } else {
                        num * 25.4 / 72.0
                    };
                    return Ok(val_in_unit);
                }
            } else if let Some(num_str) = val_str.strip_suffix("pc") {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    let num_pt = num * 12.0;
                    let val_in_unit = if unit == "in" {
                        num_pt / 72.0
                    } else {
                        num_pt * 25.4 / 72.0
                    };
                    return Ok(val_in_unit);
                }
            } else if let Some(num_str) = val_str
                .strip_suffix('q')
                .or_else(|| val_str.strip_suffix('Q'))
            {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    let num_mm = num * 0.25;
                    let val_in_unit = if unit == "mm" { num_mm } else { num_mm / 25.4 };
                    return Ok(val_in_unit);
                }
            } else if let Some(num_str) = val_str.strip_suffix("px") {
                if let Ok(num) = num_str.trim().parse::<f32>() {
                    return Ok(num * scale_px);
                }
            } else if let Ok(num) = val_str.parse::<f32>() {
                return Ok(num * scale_px);
            }
        }
    }

    let vb_re = regex::Regex::new(r#"\bviewBox\s*=\s*["']([^"']+)["']"#).unwrap();
    if let Some(c) = vb_re.captures(attrs) {
        let vb_str = c.get(1).unwrap().as_str().trim();
        let parts: Vec<&str> = vb_str
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() == 4 {
            let vb_dim_str = if axis == 0 { parts[2] } else { parts[3] };
            if let Ok(num) = vb_dim_str.parse::<f32>() {
                return Ok(num * scale_px);
            }
        }
    }

    Err(AppError::unsupported_layout_item(
        Reason::IntrinsicSizeUndefined,
        format!("at {path}: svg has no absolute dimension or viewBox on requested axis"),
    ))
}

pub(super) fn to_page_coords(point: &Point, page_height_units: f32) -> (f32, f32) {
    (point.x, page_height_units - point.y)
}

pub(super) fn typst_font_options() -> TypstKitFontOptions {
    let dir = crate::resolve_dir(std::env::var_os("LABELER_FONTS_DIR"), "fonts");
    // Exclude host system fonts so render output depends only on the bundled fonts and is identical
    // across dev, CI, and the deployed container; a system-installed face must never shadow the
    // bundled Inter. See #100. (`include_embedded_fonts` stays on for Typst's default fallback faces.)
    TypstKitFontOptions::default()
        .include_system_fonts(false)
        .include_dirs([dir])
}

/// The box a fit has to land inside, in template units. Grouped because the width, the height and
/// the unit they are expressed in are one fact, and passing them as three parallel floats pushed the
/// fitting entry points past a sane argument count.
#[derive(Clone, Copy)]
pub(super) struct FitBox<'a> {
    pub width_units: f32,
    pub height_units: f32,
    pub unit: &'a str,
}

const WGHT: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"wght");
const OPSZ: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"opsz");

/// Parse a face and confirm it carries the axes the fitter varies. Byte-taking and free of the cache
/// so a test can hand it any font without depending on which font some earlier test loaded first.
fn load_face(bytes: &[u8]) -> Result<ttf_parser::Face<'_>, AppError> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|err| {
        AppError::render_failed(
            Reason::FontParseFailed,
            format!("failed to parse font: {err}"),
        )
    })?;
    // `set_variation` reports success for any variable face even when no axis matches the tag, so it
    // cannot serve as the check. Verify up front: a font without these axes would measure silently
    // unweighted, which is the bug this measurement path exists to remove (#96).
    for tag in [WGHT, OPSZ] {
        if !face
            .variation_axes()
            .into_iter()
            .any(|axis| axis.tag == tag)
        {
            return Err(AppError::render_failed(
                Reason::FontAxisMissing,
                format!("measurement font lacks the '{tag}' variation axis"),
            ));
        }
    }
    Ok(face)
}

fn font_bytes() -> Result<&'static [u8], AppError> {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    if let Some(bytes) = BYTES.get() {
        return Ok(bytes);
    }
    let path = crate::resolve_dir(std::env::var_os("LABELER_FONTS_DIR"), "fonts")
        .join("InterVariable.ttf");
    let bytes = std::fs::read(&path).map_err(|err| {
        AppError::render_failed(
            Reason::FontReadFailed,
            format!("failed to read font: {err}"),
        )
    })?;
    load_face(&bytes)?;
    // A concurrent caller may win the race to populate the cache; either value is valid, so fall
    // back to the stored bytes rather than treating the lost race as an error.
    let _ = BYTES.set(bytes);
    Ok(BYTES.get().expect("font bytes initialized"))
}

/// The face instanced the way Typst will render it: `wght` from the item's weight, `opsz` from the
/// font size. Typst sets both automatically (typst-library `text/font/variations.rs`), and measuring
/// the default instance instead is what made bold text overflow and large text shrink needlessly.
/// Out-of-range values normalise against the axis, so this clamps as Typst's do.
fn instance(weight: u16, size_pt: f32) -> Result<ttf_parser::Face<'static>, AppError> {
    let mut face = load_face(font_bytes()?)?;
    face.set_variation(WGHT, f32::from(weight));
    face.set_variation(OPSZ, size_pt);
    Ok(face)
}

fn break_lines(
    face: &ttf_parser::Face,
    segments: &[&str],
    wrap: bool,
    size: f32,
    width_pt: f32,
) -> Vec<String> {
    if wrap {
        segments
            .iter()
            .flat_map(|seg| wrap_text(face, seg, size, width_pt))
            .collect()
    } else {
        segments.iter().map(|s| (*s).to_string()).collect()
    }
}

fn text_fits(
    face: &ttf_parser::Face,
    segments: &[&str],
    wrap: bool,
    size_pt: f32,
    width_pt: f32,
    height_pt: f32,
    vertical: VerticalAlign,
) -> bool {
    const EPS: f32 = 0.01;
    let lines = break_lines(face, segments, wrap, size_pt, width_pt);
    let h = block_height(face, size_pt, lines.len(), vertical);
    if h > height_pt + EPS {
        return false;
    }
    for line in &lines {
        if text_width(face, line, size_pt) > width_pt + EPS {
            return false;
        }
    }
    true
}

/// Largest font in [min_size, max_size] (0.5pt steps) at which `text` fits the box, else min_size.
pub(super) fn largest_fitting_font(
    segments: &[&str],
    wrap: bool,
    weight: u16,
    vertical: VerticalAlign,
    min_size: f32,
    max_size: f32,
    fit: FitBox,
) -> f32 {
    let width_pt = units_to_pt(fit.width_units, fit.unit);
    let height_pt = units_to_pt(fit.height_units, fit.unit);
    // Parse once, mutate per candidate: the loop runs up to ~76 times at 0.5pt steps, and
    // `set_variation` only rewrites normalised coordinates.
    let mut face = match instance(weight, max_size) {
        Ok(face) => face,
        Err(_) => return min_size,
    };
    let mut size = max_size;
    while size >= min_size - f32::EPSILON {
        // opsz tracks the size Typst would render this candidate at.
        face.set_variation(OPSZ, size);
        if text_fits(&face, segments, wrap, size, width_pt, height_pt, vertical) {
            return size;
        }
        size -= 0.5;
    }
    min_size
}

#[derive(Debug, Clone)]
pub struct TextFit {
    pub font_size_pt: f32,
    pub lines: Vec<String>,
    pub width_units: f32,
    pub height_units: f32,
}

#[derive(Debug, Clone)]
pub(super) struct TextLayoutItem<'a> {
    pub raw_text: &'a str,
    pub font_size: &'a FontSize,
    pub font_weight: Option<crate::models::DynamicValue<u16>>,
    pub wrap: bool,
    pub alignment: Alignment,
    pub overflow: Overflow,
}

pub(super) fn layout_text(
    item: TextLayoutItem<'_>,
    box_size: (f32, f32),
    unit: &str,
    path: &str,
) -> Result<TextFit, AppError> {
    let weight = match item.font_weight {
        Some(crate::models::DynamicValue::Literal(val)) => val,
        _ => 400,
    };

    let width_pt = units_to_pt(box_size.0, unit);
    let height_pt = units_to_pt(box_size.1, unit);
    let vertical = item.alignment.vertical;

    // Step 1: Break (Segmentation)
    let normalized = item.raw_text.replace("\r\n", "\n");
    let segments: Vec<&str> = normalized.split('\n').collect();

    // Step 2: Shrink font_size if range
    let (chosen_size, face) = match item.font_size {
        FontSize::Fixed(s) => (*s, instance(weight, *s)?),
        FontSize::Range { min, max } => {
            let fitted = largest_fitting_font(
                &segments,
                item.wrap,
                weight,
                vertical,
                *min,
                *max,
                FitBox {
                    width_units: box_size.0,
                    height_units: box_size.1,
                    unit,
                },
            );
            (fitted, instance(weight, fitted)?)
        }
    };

    // Step 3: Break & Overflow at chosen_size
    let raw_lines = break_lines(&face, &segments, item.wrap, chosen_size, width_pt);

    let fits = text_fits(
        &face,
        &segments,
        item.wrap,
        chosen_size,
        width_pt,
        height_pt,
        vertical,
    );

    let emitted_raw = if fits {
        raw_lines
    } else {
        match item.overflow {
            Overflow::Fail => {
                return Err(AppError::unsupported_layout_item(
                    Reason::TextDoesNotFit,
                    format!("at {path}: text does not fit within box"),
                ));
            }
            Overflow::Ellipsis => {
                let ellipsis_width = text_width(&face, ELLIPSIS, chosen_size);
                if width_pt < ellipsis_width {
                    return Err(AppError::unsupported_layout_item(
                        Reason::TextDoesNotFit,
                        format!(
                            "at {path}: box width {}{unit} is narrower than ellipsis marker",
                            box_size.0
                        ),
                    ));
                }
                let line_1_h = block_height(&face, chosen_size, 1, vertical);
                if line_1_h > height_pt + 0.01 {
                    return Err(AppError::unsupported_layout_item(
                        Reason::TextDoesNotFit,
                        format!(
                            "at {path}: box height {}{unit} is shorter than one line at font size {chosen_size}pt",
                            box_size.1
                        ),
                    ));
                }

                let max_lines = ((height_pt - overflow_em(&face, vertical) * chosen_size
                    + leading(chosen_size))
                    / (cap_height(&face, chosen_size) + leading(chosen_size)))
                .floor()
                .max(1.0) as usize;

                let mut lines = raw_lines;
                let any_dropped = lines.len() > max_lines;
                if any_dropped {
                    lines.truncate(max_lines);
                }

                let last = lines.len().saturating_sub(1);
                for (index, line) in lines.iter_mut().enumerate() {
                    if text_width(&face, line, chosen_size) > width_pt
                        || (any_dropped && index == last)
                    {
                        *line = ellipsize(&face, line, chosen_size, width_pt);
                    }
                }
                lines
            }
        }
    };

    // Step 4: Intrinsic metrics and emission
    let emitted_count = emitted_raw.len();
    let block_h_pt = if emitted_count == 0 {
        0.0
    } else {
        block_height(&face, chosen_size, emitted_count, vertical)
    };
    let max_w_pt = emitted_raw
        .iter()
        .map(|l| text_width(&face, l, chosen_size))
        .fold(0.0_f32, f32::max);

    let height_units = pt_to_units(block_h_pt, unit);
    let width_units = pt_to_units(max_w_pt, unit);

    let lines = emitted_raw
        .iter()
        .map(|l| to_nonbreaking(l.as_str()))
        .collect();

    Ok(TextFit {
        font_size_pt: chosen_size,
        lines,
        width_units,
        height_units,
    })
}

/// The overflow marker appended to a shortened line.
const ELLIPSIS: &str = "...";

/// Shorten `line` until it and the marker fit in `width_pt`, then append the marker. Callers have
/// already refused a box narrower than the marker itself, so the result always fits.
fn ellipsize(face: &ttf_parser::Face, line: &str, size: f32, width_pt: f32) -> String {
    let mut out = line.to_string();
    while !out.is_empty() && text_width(face, &format!("{out}{ELLIPSIS}"), size) > width_pt {
        out.pop();
    }
    format!("{out}{ELLIPSIS}")
}

fn wrap_text(face: &ttf_parser::Face, segment: &str, size: f32, width_pt: f32) -> Vec<String> {
    if segment.trim().is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let space_width = text_width(face, " ", size);
    let mut current = String::new();
    let mut current_width = 0.0;
    for word in segment.split_whitespace() {
        let word_width = text_width(face, word, size);
        if current.is_empty() {
            if word_width <= width_pt {
                current.push_str(word);
                current_width = word_width;
            } else {
                let mut chunk = String::new();
                let mut chunk_width = 0.0;
                for ch in word.chars() {
                    let ch_width = text_width(face, &ch.to_string(), size);
                    if !chunk.is_empty() && chunk_width + ch_width > width_pt {
                        lines.push(chunk);
                        chunk = String::new();
                        chunk_width = 0.0;
                    }
                    chunk.push(ch);
                    chunk_width += ch_width;
                }
                if !chunk.is_empty() {
                    current = chunk;
                    current_width = chunk_width;
                }
            }
            continue;
        }

        if current_width + space_width + word_width <= width_pt {
            current.push(' ');
            current.push_str(word);
            current_width += space_width + word_width;
        } else {
            lines.push(current);
            current = String::new();
            if word_width <= width_pt {
                current.push_str(word);
                current_width = word_width;
            } else {
                let mut chunk = String::new();
                let mut chunk_width = 0.0;
                for ch in word.chars() {
                    let ch_width = text_width(face, &ch.to_string(), size);
                    if !chunk.is_empty() && chunk_width + ch_width > width_pt {
                        lines.push(chunk);
                        chunk = String::new();
                        chunk_width = 0.0;
                    }
                    chunk.push(ch);
                    chunk_width += ch_width;
                }
                current = chunk;
                current_width = chunk_width;
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn text_width(face: &ttf_parser::Face, text: &str, size: f32) -> f32 {
    let upem = f32::from(face.units_per_em());
    text.chars()
        .map(|ch| {
            // A character Inter lacks measures as .notdef, which is what fontdue did. Typst renders
            // it from a fallback face, so it is not an error here — only an approximation, as before.
            // Dropping it instead would measure zero width, and under-measuring is what overflows.
            let glyph = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
            f32::from(face.glyph_hor_advance(glyph).unwrap_or(0))
        })
        .sum::<f32>()
        / upem
        * size
}

/// Typst reads the typographic (OS/2 sTypo*) metrics and falls back to hhea; match it, or every
/// derived number is measured against a font the renderer is not using
/// (typst-library `text/font/metrics.rs`).
fn typo_ascender(face: &ttf_parser::Face) -> f32 {
    f32::from(
        face.typographic_ascender()
            .unwrap_or_else(|| face.ascender()),
    )
}

fn typo_descender(face: &ttf_parser::Face) -> f32 {
    f32::from(
        face.typographic_descender()
            .unwrap_or_else(|| face.descender()),
    )
}

/// Typst's line box runs cap-height to baseline (`text/mod.rs` top/bottom edge defaults).
fn cap_height(face: &ttf_parser::Face, size: f32) -> f32 {
    let upem = f32::from(face.units_per_em());
    // Falls back to the *typographic* ascender, as Typst does, not the hhea one. Only differs for a
    // font supplied through LABELER_FONTS_DIR, which is exactly what the bundled-font tests cannot see.
    let cap = face
        .capital_height()
        .filter(|v| *v > 0)
        .map(f32::from)
        .unwrap_or_else(|| typo_ascender(face));
    cap / upem * size
}

/// Ink above the cap-height line, as a fraction of the em: where accents live.
fn ascent_overflow_em(face: &ttf_parser::Face) -> f32 {
    let upem = f32::from(face.units_per_em());
    let cap = face
        .capital_height()
        .filter(|v| *v > 0)
        .map(f32::from)
        .unwrap_or_else(|| typo_ascender(face));
    ((typo_ascender(face) - cap) / upem).max(0.0)
}

/// Ink below the baseline, as a fraction of the em: where descenders live.
fn descent_overflow_em(face: &ttf_parser::Face) -> f32 {
    (-typo_descender(face) / f32::from(face.units_per_em())).max(0.0)
}

/// The inset the renderer emits at the aligned edge, so ink stays inside the clipped slot (#124).
fn pad_em(face: &ttf_parser::Face, vertical: VerticalAlign) -> f32 {
    match vertical {
        VerticalAlign::Top => ascent_overflow_em(face),
        VerticalAlign::Bottom => descent_overflow_em(face),
        VerticalAlign::Center => 0.0,
    }
}

/// The pad in points, for callers outside this module: `render/mod.rs` has no `Face` and `instance`
/// stays private.
pub(super) fn pad_pt(weight: u16, size: f32, vertical: VerticalAlign) -> Result<f32, AppError> {
    // Short-circuit before touching the font. Center pads nothing, and loading the measurement face
    // here would give a centered fixed-size render a new way to fail — it would start depending on
    // InterVariable being present and carrying the wght/opsz axes to emit source it does not use.
    if matches!(vertical, VerticalAlign::Center) {
        return Ok(0.0);
    }
    let face = instance(weight, size)?;
    Ok(pad_em(&face, vertical) * size)
}

/// What the *fitter* holds back so ink falling outside the cap-height line box cannot clip:
/// `Top` and `Bottom` reserve both overflows because padding the aligned edge pushes the opposite
/// edge toward the slot floor/ceiling; `Center` reserves twice the larger overflow because the metric
/// block is centred and the slack on each side must absorb the overflow on that side (ADR-0084).
fn overflow_em(face: &ttf_parser::Face, vertical: VerticalAlign) -> f32 {
    match vertical {
        VerticalAlign::Top | VerticalAlign::Bottom => {
            ascent_overflow_em(face) + descent_overflow_em(face)
        }
        VerticalAlign::Center => 2.0 * ascent_overflow_em(face).max(descent_overflow_em(face)),
    }
}

/// Typst's default paragraph leading, 0.65em (`model/par.rs`). Sits *between* lines only.
fn leading(size: f32) -> f32 {
    size * 0.65
}

/// Height of an `n`-line metric block as Typst stacks it: leading between lines, not after the last one
/// (typst-layout `collect.rs` pushes it only when `i > 0`). A fused per-line constant — which is what
/// a "line height" is — overshoots by one leading per block and shrinks text that would have fit.
fn metric_block_height(face: &ttf_parser::Face, size: f32, lines: usize) -> f32 {
    let n = lines.max(1) as f32;
    n * cap_height(face, size) + (n - 1.0) * leading(size)
}

/// The reserved demand: the metric block height Typst lays out plus the ink reservation for the
/// item's vertical alignment.
fn block_height(face: &ttf_parser::Face, size: f32, lines: usize, vertical: VerticalAlign) -> f32 {
    // The overflow is read off *this* face, already instanced at the candidate size: the metrics move
    // with the opsz axis, so a ratio captured earlier would belong to a different instance.
    metric_block_height(face, size, lines) + overflow_em(face, vertical) * size
}

#[cfg(test)]
pub(crate) fn block_height_for_test(weight: u16, size: f32, lines: usize) -> f32 {
    let face = instance(weight, size).expect("face");
    metric_block_height(&face, size, lines)
}

#[cfg(test)]
pub(crate) fn block_height_with_align_for_test(
    weight: u16,
    size: f32,
    lines: usize,
    vertical: VerticalAlign,
) -> f32 {
    let face = instance(weight, size).expect("face");
    block_height(&face, size, lines, vertical)
}

#[cfg(test)]
pub(crate) fn text_width_for_test(weight: u16, size: f32, text: &str) -> f32 {
    let face = instance(weight, size).expect("face");
    text_width(&face, text, size)
}

#[cfg(test)]
pub(crate) fn units_to_pt_for_test(value: f32, unit: &str) -> f32 {
    units_to_pt(value, unit)
}

#[cfg(test)]
pub(crate) fn pt_to_units_for_test(value_pt: f32, unit: &str) -> f32 {
    pt_to_units(value_pt, unit)
}

fn units_to_pt(value: f32, unit: &str) -> f32 {
    match unit {
        "in" => value * 72.0,
        "mm" => value * 72.0 / 25.4,
        _ => value,
    }
}

fn pt_to_units(value_pt: f32, unit: &str) -> f32 {
    match unit {
        "in" => value_pt / 72.0,
        "mm" => value_pt * 25.4 / 72.0,
        _ => value_pt,
    }
}

pub(super) fn typst_alignment(alignment: &Alignment) -> String {
    let horizontal = match alignment.horizontal {
        HorizontalAlign::Left => "left",
        HorizontalAlign::Center => "center",
        HorizontalAlign::Right => "right",
    };
    let vertical = match alignment.vertical {
        VerticalAlign::Top => "top",
        VerticalAlign::Center => "horizon",
        VerticalAlign::Bottom => "bottom",
    };
    format!("{vertical} + {horizontal}")
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ImageFmt {
    Png,
    Jpg,
    Svg,
}

impl ImageFmt {
    pub(super) fn ext(&self) -> &'static str {
        match self {
            ImageFmt::Png => "png",
            ImageFmt::Jpg => "jpg",
            ImageFmt::Svg => "svg",
        }
    }

    fn from_mime(mime: &str) -> Result<Self, AppError> {
        match mime.trim() {
            "image/png" => Ok(ImageFmt::Png),
            "image/jpeg" | "image/jpg" => Ok(ImageFmt::Jpg),
            "image/svg+xml" => Ok(ImageFmt::Svg),
            other => Err(AppError::unsupported_layout_item(
                Reason::ImageFormatUnsupported,
                format!("unsupported image type '{other}'"),
            )),
        }
    }

    fn from_path(path: &str) -> Result<Self, AppError> {
        let ext = path.rsplit('.').next().map(|e| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("png") => Ok(ImageFmt::Png),
            Some("jpg") | Some("jpeg") => Ok(ImageFmt::Jpg),
            Some("svg") => Ok(ImageFmt::Svg),
            _ => Err(AppError::unsupported_layout_item(
                Reason::ImageFormatUnsupported,
                format!("unsupported image extension for '{path}'"),
            )),
        }
    }
}

pub(super) fn assets_root() -> PathBuf {
    crate::resolve_dir(std::env::var_os("LABELER_CONFIG_DIR"), "/config").join("assets")
}

pub(super) fn parse_image_data_uri(value: &str) -> Result<(Vec<u8>, ImageFmt), AppError> {
    let rest = value.strip_prefix("data:").ok_or_else(|| {
        AppError::unsupported_layout_item(
            Reason::ImageDataInvalid,
            "image data must be a base64 data URI",
        )
    })?;
    let (meta, payload) = rest.split_once(',').ok_or_else(|| {
        AppError::unsupported_layout_item(Reason::ImageDataInvalid, "malformed image data URI")
    })?;
    let mut params = meta.split(';');
    let mime = params.next().unwrap_or("");
    if !params.any(|p| p.eq_ignore_ascii_case("base64")) {
        return Err(AppError::unsupported_layout_item(
            Reason::ImageDataInvalid,
            "image data URI must be base64-encoded",
        ));
    }
    let fmt = ImageFmt::from_mime(mime)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|_| {
            AppError::unsupported_layout_item(
                Reason::ImageDataInvalid,
                "image data is not valid base64",
            )
        })?;
    Ok((bytes, fmt))
}

pub(super) fn resolve_image_asset(root: &Path, src: &str) -> Result<(Vec<u8>, ImageFmt), AppError> {
    let fmt = ImageFmt::from_path(src)?;
    let canon_root = root.canonicalize().map_err(|_| {
        AppError::unsupported_layout_item(
            Reason::AssetsDirUnavailable,
            "assets directory is not available",
        )
    })?;
    let candidate = canon_root.join(src);
    let canon = candidate.canonicalize().map_err(|_| {
        AppError::unsupported_layout_item(
            Reason::ImageAssetMissing,
            format!("image asset not found: {src}"),
        )
    })?;
    if !canon.starts_with(&canon_root) {
        return Err(AppError::unsupported_layout_item(
            Reason::ImageAssetPathEscapes,
            "image asset path escapes the assets directory",
        ));
    }
    let bytes = std::fs::read(&canon).map_err(|_| {
        AppError::unsupported_layout_item(
            Reason::ImageAssetUnreadable,
            format!("image asset not readable: {src}"),
        )
    })?;
    Ok((bytes, fmt))
}

#[cfg(test)]
mod binarize_tests {
    use super::binarize_rgba;

    #[test]
    fn binarize_rgba_makes_pure_black_or_white() {
        // grays: 0, 64, 127 (->black), 128, 200, 255 (->white). Pixels 0 and 4 start
        // non-opaque, so forcing alpha to 255 is something the assertion below can observe.
        let mut data = vec![
            0, 0, 0, 0, 64, 64, 64, 255, 127, 127, 127, 255, 128, 128, 128, 255, 200, 200, 200,
            200, 255, 255, 255, 255,
        ];
        binarize_rgba(&mut data);
        for (i, px) in data.as_chunks::<4>().0.iter().enumerate() {
            assert!(px[3] == 255, "pixel {i} alpha not forced opaque: {px:?}");
            assert!(
                (px[0], px[1], px[2]) == (0, 0, 0) || (px[0], px[1], px[2]) == (255, 255, 255),
                "pixel {i} not pure B/W: {px:?}"
            );
        }
        // 0.5 split: index 2 (127) -> black, index 3 (128) -> white
        assert_eq!(&data[8..11], &[0, 0, 0]);
        assert_eq!(&data[12..15], &[255, 255, 255]);
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_image_data_uri, resolve_image_asset};
    use base64::Engine as _;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const PNG_1X1_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

    fn unique_dir(label: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("labeler_img_{label}_{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_data_uri_accepts_png() {
        let uri = format!("data:image/png;base64,{PNG_1X1_B64}");
        let (bytes, fmt) = parse_image_data_uri(&uri).expect("parse");
        assert!(!bytes.is_empty());
        assert_eq!(fmt.ext(), "png");
    }

    #[test]
    fn parse_data_uri_rejects_non_data_uri() {
        assert!(parse_image_data_uri("not-a-data-uri").is_err());
    }

    #[test]
    fn parse_data_uri_rejects_bad_base64() {
        assert!(parse_image_data_uri("data:image/png;base64,@@@not base64@@@").is_err());
    }

    #[test]
    fn parse_data_uri_rejects_unsupported_mime() {
        let uri = format!("data:image/gif;base64,{PNG_1X1_B64}");
        assert!(parse_image_data_uri(&uri).is_err());
    }

    #[test]
    fn resolve_asset_reads_file_under_root() {
        let dir = unique_dir("ok");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(PNG_1X1_B64)
            .unwrap();
        fs::write(dir.join("logo.png"), &bytes).unwrap();
        let (got, fmt) = resolve_image_asset(&dir, "logo.png").expect("resolve");
        assert_eq!(got, bytes);
        assert_eq!(fmt.ext(), "png");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_asset_rejects_traversal() {
        let root = unique_dir("escape");
        let parent = root.parent().unwrap();
        let secret = parent.join(format!("labeler_secret_{}.png", std::process::id()));
        fs::write(&secret, b"x").unwrap();
        let rel = format!("../{}", secret.file_name().unwrap().to_str().unwrap());
        assert!(resolve_image_asset(&root, &rel).is_err());
        fs::remove_file(&secret).ok();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_asset_missing_file_errors() {
        let dir = unique_dir("missing");
        assert!(resolve_image_asset(&dir, "nope.png").is_err());
        fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod helpers_tests {
    use super::{largest_fitting_font, layout_text, FitBox};
    use crate::errors::AppError;
    use crate::models::{Alignment, FontSize, HorizontalAlign, Overflow, VerticalAlign};

    #[test]
    fn largest_fitting_font_picks_max_then_steps_down() {
        assert_eq!(
            largest_fitting_font(
                &["Hi"],
                false,
                400,
                VerticalAlign::Center,
                6.0,
                20.0,
                FitBox {
                    width_units: 200.0,
                    height_units: 50.0,
                    unit: "mm"
                }
            ),
            20.0
        );
        assert_eq!(
            largest_fitting_font(
                &["A long label that cannot fit"],
                false,
                400,
                VerticalAlign::Center,
                6.0,
                20.0,
                FitBox {
                    width_units: 2.0,
                    height_units: 3.0,
                    unit: "mm"
                }
            ),
            6.0
        );
    }

    fn test_layout(
        text: &str,
        font_size: &FontSize,
        wrap: bool,
        align: Alignment,
        overflow: Overflow,
        box_size: (f32, f32),
    ) -> Result<super::TextFit, AppError> {
        layout_text(
            super::TextLayoutItem {
                raw_text: text,
                font_size,
                font_weight: None,
                wrap,
                alignment: align,
                overflow,
            },
            box_size,
            "mm",
            "layout[0]",
        )
    }

    #[test]
    fn layout_text_short_text_is_content_width() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let m = test_layout(
            "Hi",
            &FontSize::Range {
                min: 6.0,
                max: 20.0,
            },
            false,
            align,
            Overflow::Ellipsis,
            (200.0, 50.0),
        )
        .unwrap();
        assert_eq!(m.font_size_pt, 20.0);
        assert!(m.width_units > 0.0 && m.width_units < 200.0);
        assert_eq!(m.lines, vec!["Hi".to_string()]);
    }

    #[test]
    fn layout_text_overflow_ellipsizes_at_min_and_uses_budget() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let m = test_layout(
            "An extremely long label that cannot possibly fit even at the minimum font size",
            &FontSize::Range {
                min: 6.0,
                max: 20.0,
            },
            false,
            align,
            Overflow::Ellipsis,
            (8.0, 3.0),
        )
        .unwrap();
        assert_eq!(m.font_size_pt, 6.0);
        assert_eq!(m.lines.len(), 1);
        assert!(m.lines[0].ends_with("...") || m.lines[0].ends_with('\u{2026}'));
    }

    #[test]
    fn layout_text_overflow_fail_returns_err() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let res = test_layout(
            "An extremely long label that cannot possibly fit even at the minimum font size",
            &FontSize::Range {
                min: 6.0,
                max: 20.0,
            },
            false,
            align,
            Overflow::Fail,
            (8.0, 3.0),
        );
        assert!(res.is_err());
    }

    /// Task 9.2's first irreducible case: a box narrower than the marker itself has nothing left
    /// to shorten, so `ellipsis` reaches the same refusal `fail` would.
    #[test]
    fn layout_text_ellipsis_refuses_a_box_narrower_than_the_marker() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        let err = test_layout(
            "ABC",
            &FontSize::Fixed(6.0),
            false,
            align,
            Overflow::Ellipsis,
            (0.5, 10.0),
        )
        .expect_err("a box narrower than '...' cannot be ellipsized");
        assert_eq!(err.reason(), Some("text_does_not_fit"));
        assert!(
            err.message_text().contains("narrower than ellipsis marker"),
            "got {}",
            err.message_text()
        );
    }

    /// Task 9.2's second irreducible case: shortening a line cannot buy vertical room, so a box
    /// shorter than one line at the chosen size is refused rather than cut through the middle.
    #[test]
    fn layout_text_ellipsis_refuses_a_box_shorter_than_one_line() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        let err = test_layout(
            "ABC",
            &FontSize::Fixed(12.0),
            false,
            align,
            Overflow::Ellipsis,
            (50.0, 0.5),
        )
        .expect_err("a box shorter than one line cannot be ellipsized");
        assert_eq!(err.reason(), Some("text_does_not_fit"));
        assert!(
            err.message_text().contains("shorter than one line"),
            "got {}",
            err.message_text()
        );
    }

    /// `wrap_text` splits an over-wide word per glyph and keeps a glyph that is wider than the box
    /// on its own line, so an over-wide line can sit anywhere in the block, not only last. Every
    /// emitted line must fit: clipping is never an outcome of an overflow policy.
    #[test]
    fn layout_text_ellipsizes_every_over_wide_line_not_only_the_last() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        let face = super::instance(400, 6.0).unwrap();
        let glyph_w = super::text_width(&face, "W", 6.0);
        let marker_w = super::text_width(&face, "...", 6.0);
        assert!(
            marker_w < glyph_w,
            "the case needs a box between '...' and 'W': {marker_w} vs {glyph_w}"
        );
        let box_w = super::pt_to_units((marker_w + glyph_w) / 2.0, "mm");

        let m = test_layout(
            "WW",
            &FontSize::Fixed(6.0),
            true,
            align,
            Overflow::Ellipsis,
            (box_w, 10.0),
        )
        .unwrap();

        assert!(
            m.lines.len() > 1,
            "expected a wrapped block, got {:?}",
            m.lines
        );
        assert!(
            m.width_units <= box_w + 1e-4,
            "line wider than its box: {} > {box_w} in {:?}",
            m.width_units,
            m.lines
        );
    }

    /// The marker records dropped content. A block that fits the line budget dropped nothing off
    /// its end, so its final line is emitted as authored even when an earlier over-wide line had
    /// to be shortened: only the line that overflowed is touched.
    #[test]
    fn layout_text_ellipsis_leaves_a_final_line_that_fits_intact() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        let face = super::instance(400, 6.0).unwrap();
        let glyph_w = super::text_width(&face, "W", 6.0);
        let marker_w = super::text_width(&face, "...", 6.0);
        assert!(
            marker_w < glyph_w,
            "the case needs a box between '...' and 'W': {marker_w} vs {glyph_w}"
        );
        let box_w = super::pt_to_units((marker_w + glyph_w) / 2.0, "mm");

        let m = test_layout(
            "W i",
            &FontSize::Fixed(6.0),
            true,
            align,
            Overflow::Ellipsis,
            // Two lines of 6pt fit in 6mm and three do not, so the block exactly fills the line
            // budget: the case where a line count alone cannot tell whether anything was dropped.
            (box_w, 6.0),
        )
        .unwrap();

        assert_eq!(
            m.lines,
            vec!["...".to_string(), "i".to_string()],
            "the over-wide first line becomes the marker and the fitting last line is untouched"
        );
    }

    #[test]
    fn layout_text_fixed_font_no_shrink() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let m = test_layout(
            "Hi",
            &FontSize::Fixed(12.0),
            false,
            align,
            Overflow::Ellipsis,
            (200.0, 50.0),
        )
        .unwrap();
        assert_eq!(m.font_size_pt, 12.0);
        assert_eq!(m.lines, vec!["Hi".to_string()]);
    }

    #[test]
    fn layout_text_multiline_wraps_and_width_is_longest_line() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let m = test_layout(
            "alpha bravo charlie delta",
            &FontSize::Range {
                min: 6.0,
                max: 10.0,
            },
            true,
            align,
            Overflow::Ellipsis,
            (20.0, 20.0),
        )
        .unwrap();
        assert!(m.lines.len() >= 2, "expected wrapping, got {:?}", m.lines);
        assert!(m.width_units <= 20.0 + 0.01);
    }

    #[test]
    fn layout_text_multiline_short_text_is_single_line() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let m = test_layout(
            "Hi",
            &FontSize::Range {
                min: 6.0,
                max: 10.0,
            },
            true,
            align,
            Overflow::Ellipsis,
            (50.0, 20.0),
        )
        .unwrap();
        assert_eq!(m.lines.len(), 1);
    }

    #[test]
    fn layout_text_empty_input_is_one_line() {
        let align = Alignment {
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Center,
        };
        let m = test_layout(
            "",
            &FontSize::Range {
                min: 6.0,
                max: 10.0,
            },
            true,
            align,
            Overflow::Ellipsis,
            (50.0, 20.0),
        )
        .unwrap();
        assert_eq!(m.lines, vec![String::new()]);
        assert_eq!(m.width_units, 0.0);
        assert!(m.height_units > 0.0);
    }

    #[test]
    fn crlf_normalisation_matches_lf() {
        let align = Alignment::default();
        let font_size = FontSize::Range {
            min: 6.0,
            max: 14.0,
        };
        let face = super::instance(400, 10.0).unwrap();
        let notdef_w = super::text_width(&face, "\r", 10.0);
        assert!(
            notdef_w > 0.0,
            "guard: bare \\r must measure non-zero (.notdef) in Inter"
        );

        let crlf = test_layout(
            "abc\r\nabc",
            &font_size,
            false,
            align.clone(),
            Overflow::Ellipsis,
            (50.0, 20.0),
        )
        .unwrap();

        let lf = test_layout(
            "abc\nabc",
            &font_size,
            false,
            align,
            Overflow::Ellipsis,
            (50.0, 20.0),
        )
        .unwrap();

        assert_eq!(crlf.lines.len(), lf.lines.len());
        assert_eq!(crlf.lines.len(), 2);
        assert_eq!(crlf.font_size_pt, lf.font_size_pt);
        assert_eq!(crlf.width_units, lf.width_units);
    }

    #[test]
    fn whitespace_only_segment_keeps_its_line() {
        let align = Alignment::default();
        let m = test_layout(
            "line1\n   \nline3",
            &FontSize::Fixed(10.0),
            true,
            align,
            Overflow::Ellipsis,
            (50.0, 30.0),
        )
        .unwrap();
        assert_eq!(m.lines.len(), 3);
        assert_eq!(m.lines[0], "line1");
        assert_eq!(m.lines[1], "");
        assert_eq!(m.lines[2], "line3");
    }

    #[test]
    fn hard_breaks_survive_when_wrap_is_false() {
        let align = Alignment::default();
        let m = test_layout(
            "line1\nline2",
            &FontSize::Fixed(10.0),
            false,
            align,
            Overflow::Ellipsis,
            (50.0, 30.0),
        )
        .unwrap();
        assert_eq!(m.lines, vec!["line1", "line2"]);
    }

    #[test]
    fn dropped_trailing_blank_line_earns_ellipsis_marker() {
        let align = Alignment::default();
        let face = super::instance(400, 10.0).unwrap();
        let msg_w_pt = super::text_width(&face, "message", 10.0);
        let msg_w_mm = super::pt_to_units(msg_w_pt, "mm");
        let line_1_h_pt = super::block_height(&face, 10.0, 1, VerticalAlign::Top);
        let line_1_h_mm = super::pt_to_units(line_1_h_pt, "mm");

        // Box is wide enough for "message" (msg_w_mm + 0.1) but not "message..."
        // Box is tall enough for 1 line only
        let m = test_layout(
            "message\n",
            &FontSize::Fixed(10.0),
            false,
            align,
            Overflow::Ellipsis,
            (msg_w_mm + 0.1, line_1_h_mm + 0.1),
        )
        .unwrap();

        assert_eq!(m.lines.len(), 1);
        let line = &m.lines[0];
        assert!(line.ends_with("..."), "expected ellipsis on line: {line}");
        assert_ne!(
            line, "message...",
            "at least one character should be removed"
        );
        assert!(line.starts_with('m'));
    }

    #[test]
    fn dropped_leading_blank_line_earns_ellipsis_marker() {
        let align = Alignment::default();
        let face = super::instance(400, 10.0).unwrap();
        let dot_w_pt = super::text_width(&face, "...", 10.0);
        let dot_w_mm = super::pt_to_units(dot_w_pt, "mm");
        let line_1_h_pt = super::block_height(&face, 10.0, 1, VerticalAlign::Top);
        let line_1_h_mm = super::pt_to_units(line_1_h_pt, "mm");

        // Box is tall enough for 1 line, wide enough for "..."
        let m = test_layout(
            "\nmessage",
            &FontSize::Fixed(10.0),
            false,
            align,
            Overflow::Ellipsis,
            (dot_w_mm + 1.0, line_1_h_mm + 0.1),
        )
        .unwrap();

        assert_eq!(m.lines, vec!["..."]);
    }

    #[test]
    fn fully_shown_multiline_value_carries_no_marker() {
        let align = Alignment::default();
        let m = test_layout(
            "line1\nline2",
            &FontSize::Fixed(10.0),
            false,
            align,
            Overflow::Ellipsis,
            (50.0, 30.0),
        )
        .unwrap();
        assert_eq!(m.lines, vec!["line1", "line2"]);
    }

    /// Task 3.3: a center-aligned multiline item at a fixed font_size whose box holds three
    /// metric lines but only two reserved ones keeps two lines and ellipsizes.
    #[test]
    fn layout_text_center_aligned_multiline_line_budget_reserves_overflow() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        // At 10pt in Inter:
        // metric_block(3, 10.0) = 3 * 7.275 + 2 * 6.5 = 34.83pt
        // reserved demand(2, 10.0, Center) = 2 * 7.275 + 6.5 + 4.824 = 25.875pt
        // reserved demand(3, 10.0, Center) = 34.83 + 4.824 = 39.65pt
        // In a 36.0pt box (12.7mm): holds 3 metric lines, but only 2 reserved lines.
        let box_h_mm = super::pt_to_units(36.0, "mm");
        let m = test_layout(
            "Line 1\nLine 2\nLine 3",
            &FontSize::Fixed(10.0),
            true,
            align,
            Overflow::Ellipsis,
            (100.0, box_h_mm),
        )
        .unwrap();
        assert_eq!(
            m.lines.len(),
            2,
            "expected 2 lines kept out of 3 under the reserved budget, got {:?}",
            m.lines
        );
        assert!(
            m.lines[1].ends_with("..."),
            "expected second line to be ellipsized, got {}",
            m.lines[1]
        );
    }

    /// Task 3.4: a center-aligned item with overflow: fail whose metric block fits but whose block plus
    /// reservation does not returns 422 text_does_not_fit, and one whose box cannot hold one line plus
    /// the reservation returns 422 under ellipsis too.
    #[test]
    fn layout_text_center_aligned_refusals() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        // Case 1: 3-line block in a 36.0pt (12.7mm) box with overflow: fail.
        // Metric block (34.83pt) fits within 36.0pt, but reserved demand (39.65pt) does not.
        let box_h_mm = super::pt_to_units(36.0, "mm");
        let err_fail = test_layout(
            "Line 1\nLine 2\nLine 3",
            &FontSize::Fixed(10.0),
            true,
            align.clone(),
            Overflow::Fail,
            (100.0, box_h_mm),
        )
        .expect_err("overflow: fail must reject when block plus reservation exceeds box");
        assert_eq!(err_fail.reason(), Some("text_does_not_fit"));

        // Case 2: 1-line item in a box shorter than one line plus reservation (e.g. 8.0pt = 2.822mm).
        // 1 metric line = 7.275pt (< 8.0pt), but 1 reserved line = 12.099pt (> 8.0pt).
        let short_box_mm = super::pt_to_units(8.0, "mm");
        let err_ellipsis = test_layout(
            "One line",
            &FontSize::Fixed(10.0),
            false,
            align,
            Overflow::Ellipsis,
            (100.0, short_box_mm),
        )
        .expect_err("box shorter than one line plus reservation must error under ellipsis");
        assert_eq!(err_ellipsis.reason(), Some("text_does_not_fit"));
        assert!(
            err_ellipsis
                .message_text()
                .contains("shorter than one line"),
            "got {}",
            err_ellipsis.message_text()
        );
    }

    /// Task 3.5: a center-aligned text with a content height resolves a box taller by the
    /// reservation, and its top-aligned twin is unchanged.
    #[test]
    fn layout_text_center_aligned_content_height_includes_reservation() {
        let center_align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        let top_align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        };
        let m_center = test_layout(
            "Sample text",
            &FontSize::Fixed(10.0),
            false,
            center_align,
            Overflow::Ellipsis,
            (100.0, 50.0),
        )
        .unwrap();
        let m_top = test_layout(
            "Sample text",
            &FontSize::Fixed(10.0),
            false,
            top_align,
            Overflow::Ellipsis,
            (100.0, 50.0),
        )
        .unwrap();

        let face = super::instance(400, 10.0).unwrap();
        let expected_h_pt = super::cap_height(&face, 10.0)
            + super::overflow_em(&face, VerticalAlign::Center) * 10.0;
        let expected_h_mm = super::pt_to_units(expected_h_pt, "mm");

        assert!(
            (m_center.height_units - expected_h_mm).abs() < 1e-4,
            "center content height {} should match expected reserved height {}",
            m_center.height_units,
            expected_h_mm
        );
        assert_eq!(
            m_center.height_units, m_top.height_units,
            "center and top alignments should resolve identical intrinsic height in symmetric font"
        );
    }
}

#[cfg(test)]
mod interpolate_tests {
    use super::interpolate;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::OnceLock;

    fn data() -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("id".to_string(), json!("A1")),
            ("count".to_string(), json!(3)),
        ])
    }

    fn variables() -> BTreeMap<String, String> {
        BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())])
    }

    fn no_datetime() -> crate::datetime_fmt::DateTimeResolver<'static> {
        static EMPTY: OnceLock<BTreeMap<String, String>> = OnceLock::new();
        let formats = EMPTY.get_or_init(BTreeMap::new);
        crate::datetime_fmt::DateTimeResolver {
            formats,
            now: chrono::Local::now(),
        }
    }

    #[test]
    fn substitutes_field_and_variable() {
        let out = interpolate(
            "{vars.qr_base_url}/{id}",
            &data(),
            &variables(),
            &no_datetime(),
            None,
        )
        .unwrap();
        assert_eq!(out, "https://h/i/A1");
    }

    #[test]
    fn stringifies_non_string_field() {
        assert_eq!(
            interpolate("n={count}", &data(), &variables(), &no_datetime(), None).unwrap(),
            "n=3"
        );
    }

    #[test]
    fn literal_braces() {
        assert_eq!(
            interpolate("{{x}}", &data(), &variables(), &no_datetime(), None).unwrap(),
            "{x}"
        );
    }

    #[test]
    fn missing_field_errors() {
        assert!(interpolate("{nope}", &data(), &variables(), &no_datetime(), None).is_err());
    }

    #[test]
    fn missing_variable_errors() {
        assert!(interpolate("{vars.nope}", &data(), &variables(), &no_datetime(), None).is_err());
    }

    #[test]
    fn unmatched_brace_errors() {
        assert!(interpolate("a{id", &data(), &variables(), &no_datetime(), None).is_err());
        assert!(interpolate("a}id", &data(), &variables(), &no_datetime(), None).is_err());
        assert!(interpolate("{bad{token}", &data(), &variables(), &no_datetime(), None).is_err());
    }

    #[test]
    fn interpolate_join_renders_exact_strings() {
        let mut d = data();
        d.insert("tags".to_string(), serde_json::json!(["A", "B"]));
        d.insert("codes".to_string(), serde_json::json!(["1", "true"]));
        d.insert("single".to_string(), serde_json::json!(["ONLY"]));
        d.insert("empty".to_string(), serde_json::json!([]));
        d.insert("bad_elem".to_string(), serde_json::json!(["A", 123]));

        // Multiple elements with separator
        let out = interpolate("{tags:join(', ')}", &d, &variables(), &no_datetime(), None).unwrap();
        assert_eq!(out, "A, B");

        // Pipe separator
        let out = interpolate("{codes:join('|')}", &d, &variables(), &no_datetime(), None).unwrap();
        assert_eq!(out, "1|true");

        // Empty separator
        let out = interpolate("{tags:join('')}", &d, &variables(), &no_datetime(), None).unwrap();
        assert_eq!(out, "AB");

        // Separator containing colons and spaces
        let out =
            interpolate("{tags:join(' : ')}", &d, &variables(), &no_datetime(), None).unwrap();
        assert_eq!(out, "A : B");

        // Single element (separator not added)
        let out = interpolate(
            "{single:join(', ')}",
            &d,
            &variables(),
            &no_datetime(),
            None,
        )
        .unwrap();
        assert_eq!(out, "ONLY");

        // Zero elements
        let out =
            interpolate("{empty:join(', ')}", &d, &variables(), &no_datetime(), None).unwrap();
        assert_eq!(out, "");

        // Text around join token
        let out = interpolate(
            "Tags: [{tags:join(', ')}]",
            &d,
            &variables(),
            &no_datetime(),
            None,
        )
        .unwrap();
        assert_eq!(out, "Tags: [A, B]");

        // Non-string element in array fails with field_value_not_scalar
        let err = interpolate(
            "{bad_elem:join(', ')}",
            &d,
            &variables(),
            &no_datetime(),
            None,
        )
        .unwrap_err();
        assert_eq!(err.code(), "UnsupportedLayoutItem");

        // Array reaching scalar token slot fails with field_value_not_scalar
        let err = interpolate("{tags}", &d, &variables(), &no_datetime(), None).unwrap_err();
        assert_eq!(err.code(), "UnsupportedLayoutItem");
    }
}

#[cfg(test)]
mod measurement_tests {
    use super::{
        cap_height, instance, largest_fitting_font, load_face, overflow_em, pad_em, pad_pt,
        text_width, units_to_pt, FitBox,
    };
    use crate::models::VerticalAlign;

    #[test]
    fn overflow_is_the_ink_outside_the_cap_height_line() {
        let face = instance(400, 14.0).expect("face");
        let top = pad_em(&face, VerticalAlign::Top);
        let bottom = pad_em(&face, VerticalAlign::Bottom);
        // Inter: cap 1490, ascender 1984, descender -494 of 2048. Both pads work out to 494 units —
        // the same number by coincidence, not because they are the same quantity.
        assert!((top - 0.2412).abs() < 0.001, "top pad {top}");
        assert!((bottom - 0.2412).abs() < 0.001, "bottom pad {bottom}");

        // The fit reservation is both overflows: neither one pad, nor the 1.21em band.
        let both = overflow_em(&face, VerticalAlign::Top);
        assert!((both - (top + bottom)).abs() < 1e-6, "overflow {both}");
        assert!((both - 0.4824).abs() < 0.001, "overflow {both}");

        // Center pads nothing (placement is unchanged), but reserves 2 * max(top, bottom) (ADR-0084).
        assert_eq!(pad_em(&face, VerticalAlign::Center), 0.0);
        let center_overflow = overflow_em(&face, VerticalAlign::Center);
        assert!(
            (center_overflow - 2.0 * top.max(bottom)).abs() < 1e-6,
            "center overflow {center_overflow}"
        );
        assert!(
            (center_overflow - 0.4824).abs() < 0.001,
            "center overflow {center_overflow}"
        );

        // pad_pt is the same number in points, for callers with no Face.
        let pt = pad_pt(400, 20.0, VerticalAlign::Bottom).expect("pad_pt");
        assert!((pt - bottom * 20.0).abs() < 1e-4, "pad_pt {pt}");
    }

    /// A height-bound item must leave room for the ink outside the cap-height line: aligned and
    /// centered items reserve ink room in the fitter, while placement pads only aligned edges (ADR-0084).
    #[test]
    fn a_height_bound_fit_reserves_the_overflow() {
        let fit = FitBox {
            width_units: 400.0,
            height_units: 10.0,
            unit: "mm",
        };
        let aligned =
            largest_fitting_font(&["Hxy"], false, 400, VerticalAlign::Bottom, 6.0, 80.0, fit);
        let centered =
            largest_fitting_font(&["Hxy"], false, 400, VerticalAlign::Center, 6.0, 80.0, fit);
        let face = instance(400, aligned).expect("face");
        // In symmetric Inter, both alignments reserve the same 0.4824em, so they fit at the same size
        assert_eq!(
            aligned, centered,
            "symmetric font reserves the same ink depth for bottom and center ({aligned} vs {centered})"
        );
        // Placement pad is 0.0 for center, while bottom is padded
        assert_eq!(pad_em(&face, VerticalAlign::Center), 0.0);
        assert!(pad_em(&face, VerticalAlign::Bottom) > 0.0);

        let need = cap_height(&face, aligned) + overflow_em(&face, VerticalAlign::Bottom) * aligned;
        assert!(
            need <= units_to_pt(10.0, "mm") + 0.5,
            "fitted {aligned}pt needs {need}pt in a {}pt slot",
            units_to_pt(10.0, "mm")
        );
    }

    #[test]
    fn heavier_weight_measures_wider() {
        let regular = instance(400, 14.0).expect("face");
        let bold = instance(700, 14.0).expect("face");
        let (r, b) = (
            text_width(&regular, "Widget A-42", 14.0),
            text_width(&bold, "Widget A-42", 14.0),
        );
        // Measured at 4.0% for this string; assert a floor, not the measurement.
        assert!(
            b >= r * 1.02,
            "bold must measure wider (got {r:.2} vs {b:.2})"
        );
    }

    #[test]
    fn larger_optical_size_measures_narrower() {
        // Same nominal size in both calls; only the opsz coordinate differs, so this isolates the
        // axis rather than the scale.
        let small = instance(400, 14.0).expect("face");
        let large = instance(400, 32.0).expect("face");
        let (s, l) = (
            text_width(&small, "Widget A-42", 14.0),
            text_width(&large, "Widget A-42", 14.0),
        );
        assert!(
            l < s,
            "opsz 32 must measure narrower than opsz 14 (got {s:.2} vs {l:.2})"
        );
    }

    #[test]
    fn a_font_without_the_axes_is_rejected() {
        // A real static face, not corrupt bytes: the check under test is "valid font, no wght/opsz",
        // and a parse failure would satisfy the assertion for the wrong reason.
        let bytes = typst_assets::fonts()
            .find(|bytes| {
                ttf_parser::Face::parse(bytes, 0)
                    .map(|face| face.variation_axes().is_empty())
                    .unwrap_or(false)
            })
            .expect("typst-assets must embed at least one static face");
        let err = load_face(bytes).expect_err("a static font must be rejected");
        // AppError is not Display; its Debug carries the message.
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains("variation axis"),
            "unexpected error: {rendered}"
        );
    }

    #[test]
    fn a_missing_glyph_measures_as_notdef() {
        let face = instance(400, 14.0).expect("face");
        // U+10FFFF is unmapped in every font. It must measure .notdef's advance rather than zero:
        // dropping it would under-measure, and under-measuring is what overflows a clip box.
        let width = text_width(&face, "\u{10FFFF}", 14.0);
        assert!(width > 0.0, "a missing glyph measured as zero width");
    }
}

#[cfg(test)]
mod dynamic_resolution_tests {
    use super::*;
    use crate::models::{DynamicDimension, DynamicValue};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn resolve_dynamic_value_f32_literal_and_ref() {
        let mut data = HashMap::new();
        data.insert("width".to_string(), json!(50.5));
        data.insert("width_str".to_string(), json!("60.0mm"));
        data.insert("width_in".to_string(), json!("2.5in"));

        assert_eq!(
            resolve_dynamic_value_f32(&DynamicValue::Literal(12.0), &data).unwrap(),
            12.0
        );
        assert_eq!(
            resolve_dynamic_value_f32(&DynamicValue::Ref("width".to_string()), &data).unwrap(),
            50.5
        );
        assert_eq!(
            resolve_dynamic_value_f32(&DynamicValue::Ref("width_str".to_string()), &data).unwrap(),
            60.0
        );
        assert_eq!(
            resolve_dynamic_value_f32(&DynamicValue::Ref("width_in".to_string()), &data).unwrap(),
            2.5
        );
    }

    #[test]
    fn resolve_dynamic_value_f32_missing_and_invalid() {
        let mut data = HashMap::new();
        data.insert("bad".to_string(), json!("not_a_number"));

        let err = resolve_dynamic_value_f32(&DynamicValue::Ref("missing".to_string()), &data)
            .unwrap_err();
        assert_eq!(err.code(), "MissingField");

        let err =
            resolve_dynamic_value_f32(&DynamicValue::Ref("bad".to_string()), &data).unwrap_err();
        assert_eq!(err.code(), "InvalidRequest");
    }

    #[test]
    fn resolve_dynamic_value_u16_literal_and_ref() {
        let mut data = HashMap::new();
        data.insert("weight".to_string(), json!(700));
        data.insert("weight_str".to_string(), json!("600"));

        assert_eq!(
            resolve_dynamic_value_u16(&DynamicValue::Literal(400), &data).unwrap(),
            400
        );
        assert_eq!(
            resolve_dynamic_value_u16(&DynamicValue::Ref("weight".to_string()), &data).unwrap(),
            700
        );
        assert_eq!(
            resolve_dynamic_value_u16(&DynamicValue::Ref("weight_str".to_string()), &data).unwrap(),
            600
        );
    }

    #[test]
    fn resolve_dimension_fixed_and_dynamic() {
        let mut data = HashMap::new();
        data.insert("target_w".to_string(), json!(80.0));

        let fixed = DynamicDimension::Fixed(DynamicValue::Ref("target_w".to_string()));
        assert_eq!(resolve_dimension(&fixed, &data).unwrap(), 80.0);

        let dynamic = DynamicDimension::Dynamic {
            min: Some(DynamicValue::Literal(20.0)),
            max: Some(DynamicValue::Ref("target_w".to_string())),
        };
        assert_eq!(resolve_dimension(&dynamic, &data).unwrap(), 80.0);
    }
}
