use crate::errors::AppError;
use crate::models::{
    Alignment, ErrorCorrection, FontSize, HorizontalAlign, Overflow, Point, VerticalAlign,
};
use crate::reason::Reason;
use base64::Engine as _;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;
use std::sync::OnceLock;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;
use unicode_script::{Script, UnicodeScript as _};

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
                    return Err(AppError::internal(format!(
                        "unterminated '{{' in template '{template}', which load should have refused"
                    )));
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    out.push('}');
                } else {
                    return Err(AppError::internal(format!(
                        "unmatched '}}' in template '{template}', which load should have refused"
                    )));
                }
            }
            other => out.push(other),
        }
    }
    Ok(())
}

/// Substitution-only interpolation.
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

        let token = crate::interpolation::parse(scanned.raw).map_err(|err| {
            AppError::internal(format!(
                "{err} in template '{template}', which load should have refused"
            ))
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
                        datetime.format(datetime.now, Some(fmt))?
                    }
                    Some(crate::interpolation::Reader::Join(_)) => {
                        return Err(AppError::field_value_not_scalar(inner));
                    }
                    None => datetime.format(datetime.now, None)?,
                }
            }
            crate::interpolation::Source::Vars(key) => {
                if token.reader.is_some() {
                    return Err(AppError::internal(format!(
                        "token '{inner}' passed load but cannot be read"
                    )));
                }
                variables.get(key).cloned().ok_or_else(|| {
                    AppError::reference_unresolved(
                        &format!("vars.{key}"),
                        format!("variable '{key}' is not set"),
                    )
                })?
            }
            crate::interpolation::Source::Bare(name) => {
                if let Some(instant) = instants.and_then(|inst| inst.get(name)) {
                    match token.reader {
                        Some(crate::interpolation::Reader::Format(fmt)) => {
                            datetime.format(*instant, Some(fmt))?
                        }
                        Some(crate::interpolation::Reader::Join(_)) => {
                            return Err(AppError::internal(format!(
                                "token '{inner}' passed load but cannot be read"
                            )));
                        }
                        None => datetime.format(*instant, None)?,
                    }
                } else if let Some(val) = data.get(name) {
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
                            return Err(AppError::internal(format!(
                                "token '{inner}' passed load but cannot be read"
                            )));
                        }
                        None => match val {
                            JsonValue::Array(_) => {
                                return Err(AppError::field_value_not_scalar(name));
                            }
                            other => value_to_string(other),
                        },
                    }
                } else {
                    String::new()
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
            let val = data.get(name).ok_or_else(|| {
                AppError::internal(format!(
                    "parameter '{name}' has no value although load requires its default"
                ))
            })?;
            match val {
                JsonValue::Number(n) => n.as_f64().map(|f| f as f32).ok_or_else(|| {
                    AppError::param_value_invalid(
                        name,
                        None,
                        format!("parameter '{name}' is not a valid number"),
                    )
                }),
                JsonValue::String(s) => s.trim().parse::<f32>().map_err(|_| {
                    AppError::param_value_invalid(
                        name,
                        None,
                        format!("parameter '{name}' is not a valid number"),
                    )
                }),
                _ => Err(AppError::param_value_invalid(
                    name,
                    None,
                    format!("parameter '{name}' is not a valid number"),
                )),
            }
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
                .ok_or_else(|| AppError::internal("dynamic dimension missing min/max"))
        }
    }
}

pub(super) fn format_length(value: f32, unit: &str) -> Result<String, AppError> {
    let unit = match unit {
        "mm" | "in" => unit,
        _ => return Err(AppError::internal("unknown unit")),
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

pub(super) fn qr_code(
    payload: &[u8],
    error_correction: ErrorCorrection,
) -> Result<QrCode, AppError> {
    let level = match error_correction {
        ErrorCorrection::L => EcLevel::L,
        ErrorCorrection::M => EcLevel::M,
        ErrorCorrection::Q => EcLevel::Q,
        ErrorCorrection::H => EcLevel::H,
    };
    QrCode::with_error_correction_level(payload, level).map_err(|err| {
        AppError::unsupported_layout_item(
            Reason::QrPayloadInvalid,
            format!("qr generation failed: {err}"),
        )
    })
}

pub(super) fn build_qr_svg(
    payload: &[u8],
    error_correction: ErrorCorrection,
    quiet_zone: f32,
) -> Result<String, AppError> {
    let code = qr_code(payload, error_correction)?;
    let mut renderer = code.render::<svg::Color>();
    renderer.quiet_zone(false);
    let svg = renderer.build();

    if quiet_zone > 0.0 {
        let w = code.width() as f32;
        let total = w + 2.0 * quiet_zone;
        let neg_qz = -quiet_zone;
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
    let face = ttf_parser::Face::parse(bytes, 0)
        .map_err(|err| AppError::internal(format!("failed to parse font: {err}")))?;
    // `set_variation` reports success for any variable face even when no axis matches the tag, so it
    // cannot serve as the check. Verify up front: a font without these axes would measure silently
    // unweighted, which is the bug this measurement path exists to remove (#96).
    for tag in [WGHT, OPSZ] {
        if !face
            .variation_axes()
            .into_iter()
            .any(|axis| axis.tag == tag)
        {
            return Err(AppError::internal(format!(
                "measurement font lacks the '{tag}' variation axis"
            )));
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
    let bytes = std::fs::read(&path)
        .map_err(|err| AppError::internal(format!("failed to read font: {err}")))?;
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
pub(super) fn instance(weight: u16, size_pt: f32) -> Result<rustybuzz::Face<'static>, AppError> {
    let mut face = load_face(font_bytes()?)?;
    face.set_variation(WGHT, f32::from(weight));
    face.set_variation(OPSZ, size_pt);
    Ok(rustybuzz::Face::from_face(face))
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

/// The renderer's fit tolerance. Every judgement of height compares against `H + FIT_EPS_PT`, the
/// metric cutoffs before any shaping included, or one of them refuses a block another accepts.
const FIT_EPS_PT: f32 = 0.01;

fn fits_height(needed_pt: f32, height_pt: f32) -> bool {
    needed_pt <= height_pt + FIT_EPS_PT
}

/// The width half of the same judgement, shared by the fit and the line budget so a line the fit
/// accepts as it stands is never shortened by the budget.
fn fits_width(face: &ttf_parser::Face, line: &str, size_pt: f32, width_pt: f32) -> bool {
    text_width(face, line, size_pt) <= width_pt + FIT_EPS_PT
}

/// A block of lines judged against its box at one size.
struct Judgement {
    ink: BlockInk,
    fits: bool,
}

/// Judge `lines` as broken. `None` when they cannot fit whatever their ink: a line is over-wide, or
/// the metric block alone overflows the height. Nothing is shaped then, which is what keeps a
/// ten-thousand-line value from being measured when its metric block already says no (Decision 4).
fn judge_lines(
    face: &rustybuzz::Face<'_>,
    lines: &[String],
    size_pt: f32,
    line_spacing: Option<f32>,
    bounds_pt: (f32, f32),
    vertical: VerticalAlign,
) -> Option<Judgement> {
    let (width_pt, height_pt) = bounds_pt;
    if !lines
        .iter()
        .all(|line| fits_width(face, line, size_pt, width_pt))
    {
        return None;
    }
    let metric_h = metric_block_height(face, size_pt, lines.len(), line_spacing);
    if !fits_height(metric_h, height_pt) {
        return None;
    }
    let pitch = line_pitch(size_pt, resolve_line_spacing(line_spacing));
    let ink = block_ink(
        &line_inks(face, lines, size_pt),
        cap_height(face, size_pt),
        pitch,
    );
    let fits = fits_height(metric_h + ink.reserve(vertical), height_pt);
    Some(Judgement { ink, fits })
}

fn text_fits(
    face: &rustybuzz::Face<'_>,
    segments: &[&str],
    wrap: bool,
    size_pt: f32,
    line_spacing: Option<f32>,
    bounds_pt: (f32, f32),
    vertical: VerticalAlign,
) -> bool {
    let lines = break_lines(face, segments, wrap, size_pt, bounds_pt.0);
    judge_lines(face, &lines, size_pt, line_spacing, bounds_pt, vertical).is_some_and(|j| j.fits)
}

/// Largest font in [min_size, max_size] (0.5pt steps) at which `text` fits the box, else min_size.
pub(super) fn largest_fitting_font(
    segments: &[&str],
    wrap: bool,
    weight: u16,
    line_spacing: Option<f32>,
    vertical: VerticalAlign,
    range: (f32, f32),
    fit: FitBox,
) -> f32 {
    let (min_size, max_size) = range;
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
        // opsz tracks the size Typst would render this candidate at, and the ink moves with it.
        face.set_variation(OPSZ, size);
        if text_fits(
            &face,
            segments,
            wrap,
            size,
            line_spacing,
            (width_pt, height_pt),
            vertical,
        ) {
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
    pub line_spacing: Option<f32>,
    /// The emitted block's outer ink above its metric top and below its last baseline, in points:
    /// what `top` and `bottom` inset by, measured on the lines in `lines`.
    pub a: f32,
    pub d: f32,
}

#[derive(Debug, Clone)]
pub(super) struct TextLayoutItem<'a> {
    pub raw_text: &'a str,
    pub font_size: &'a FontSize,
    pub font_weight: Option<u16>,
    pub wrap: bool,
    pub line_spacing: Option<f32>,
    pub alignment: Alignment,
    pub overflow: Overflow,
}

pub(super) fn layout_text(
    item: TextLayoutItem<'_>,
    box_size: (f32, f32),
    unit: &str,
    path: &str,
) -> Result<TextFit, AppError> {
    let weight = item.font_weight.unwrap_or(400);

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
                item.line_spacing,
                vertical,
                (*min, *max),
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
    let judged = judge_lines(
        &face,
        &raw_lines,
        chosen_size,
        item.line_spacing,
        (width_pt, height_pt),
        vertical,
    );

    let (emitted_raw, ink) = match judged {
        Some(Judgement {
            fits: true, ink, ..
        }) => (raw_lines, ink),
        judged => match item.overflow {
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
                line_budget(
                    &face,
                    &raw_lines,
                    judged.is_some(),
                    chosen_size,
                    line_pitch(chosen_size, resolve_line_spacing(item.line_spacing)),
                    (width_pt, height_pt),
                    vertical,
                )
                .ok_or_else(|| {
                    AppError::unsupported_layout_item(
                        Reason::TextDoesNotFit,
                        format!(
                            "at {path}: no leading run of lines fits box height {}{unit} at font size {chosen_size}pt",
                            box_size.1
                        ),
                    )
                })?
            }
        },
    };

    // Step 4: Intrinsic metrics and emission
    let emitted_count = emitted_raw.len();
    let block_h_pt = if emitted_count == 0 {
        0.0
    } else {
        metric_block_height(&face, chosen_size, emitted_count, item.line_spacing)
            + ink.reserve(vertical)
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
        line_spacing: item.line_spacing,
        a: ink.a,
        d: ink.d,
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
            current.push_str(word);
            current_width = word_width;
            continue;
        }

        if current_width + space_width + word_width <= width_pt {
            current.push(' ');
            current.push_str(word);
            current_width += space_width + word_width;
        } else {
            lines.push(current);
            current = word.to_string();
            current_width = word_width;
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

/// Typst's line box runs cap-height to baseline (`text/mod.rs` top/bottom edge defaults).
pub(super) fn cap_height(face: &ttf_parser::Face, size: f32) -> f32 {
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

/// How far one line's ink rises above its baseline and falls below it, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LineInk {
    pub rise: f32,
    pub fall: f32,
}

/// A block's outer ink (design Decision 1): `a` above its metric top and `d` below its last
/// baseline, in points, each floored at zero. Ink between the two is inside the metric box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct BlockInk {
    pub a: f32,
    pub d: f32,
}

impl BlockInk {
    /// What the fit holds back for this ink. `top` and `bottom` inset the aligned edge and push the
    /// far edge's ink the same way, so they need both; `center` splits the slack evenly, so each side
    /// must absorb its own ink alone (#245).
    pub(super) fn reserve(self, vertical: VerticalAlign) -> f32 {
        match vertical {
            VerticalAlign::Top | VerticalAlign::Bottom => self.a + self.d,
            VerticalAlign::Center => 2.0 * self.a.max(self.d),
        }
    }
}

/// Running maxima of a block's ink terms over its lines so far: the highest `rise_i − baseline_i`
/// and the deepest `baseline_i + fall_i`, with `baseline_i` measured down from the metric top.
#[derive(Debug, Clone, Copy, Default)]
struct InkExtent {
    above: f32,
    depth: Option<f32>,
}

impl InkExtent {
    fn with_line(self, ink: Option<LineInk>, baseline: f32) -> Self {
        match ink {
            None => self,
            Some(ink) => InkExtent {
                above: self.above.max(ink.rise - baseline),
                depth: Some(
                    self.depth
                        .map_or(baseline + ink.fall, |d| d.max(baseline + ink.fall)),
                ),
            },
        }
    }

    /// The outer ink of a block whose last baseline, and so whose metric block, is `last_baseline`.
    fn block(self, last_baseline: f32) -> BlockInk {
        BlockInk {
            a: self.above.max(0.0),
            d: self.depth.map_or(0.0, |d| (d - last_baseline).max(0.0)),
        }
    }
}

/// Depth of line `i`'s baseline (1-based) below the block's metric top: `metric_block(i, s)`.
fn baseline(cap: f32, pitch: f32, i: usize) -> f32 {
    cap + (i as f32 - 1.0) * pitch
}

fn block_ink(inks: &[Option<LineInk>], cap: f32, pitch: f32) -> BlockInk {
    inks.iter()
        .enumerate()
        .fold(InkExtent::default(), |ext, (idx, ink)| {
            ext.with_line(*ink, baseline(cap, pitch, idx + 1))
        })
        .block(baseline(cap, pitch, inks.len().max(1)))
}

/// Each line's ink as Typst draws the block (design Decision 2), in order. Typst collects an
/// item's lines as one paragraph, joined by `\n` (`typst-layout` `src/inline/collect.rs`), runs
/// BiDi and script segmentation over the whole of it, shapes each run as one buffer, and only then
/// cuts it into lines, so a line's glyphs can depend on its neighbours: a run of characters of no
/// specific script takes the script of the first line after it that has one. This reproduces that
/// pipeline rather than shaping lines apart. `None` for a line that draws no ink.
pub(super) fn line_inks(
    face: &rustybuzz::Face<'_>,
    lines: &[impl AsRef<str>],
    size_pt: f32,
) -> Vec<Option<LineInk>> {
    if lines.iter().all(|line| line.as_ref().is_empty()) {
        return vec![None; lines.len()];
    }
    #[cfg(test)]
    SHAPED_LINES.with(|count| {
        count.set(count.get() + lines.iter().filter(|l| !l.as_ref().is_empty()).count())
    });

    // The text the emitter writes: its spaces are no-break spaces, which shape as their own glyph.
    let forms: Vec<String> = lines.iter().map(|l| to_nonbreaking(l.as_ref())).collect();
    let text = forms.join("\n");
    let runs = shape_paragraph(face, &text);

    let scale = size_pt / face.units_per_em() as f32;
    let mut first_run = 0;
    let mut start = 0;
    let mut inks = Vec::with_capacity(forms.len());
    for form in &forms {
        // A mandatory break's line runs up to its `\n`, which is trimmed before shaping
        // (`linebreak.rs` `Breakpoint::trim`, `line.rs` `collect_range`).
        let line = start..start + form.len();
        start = line.end + 1;
        while runs.get(first_run).is_some_and(|run| run.end <= line.start) {
            first_run += 1;
        }
        let mut extent: Option<(f32, f32)> = None;
        for run in runs[first_run..]
            .iter()
            .take_while(|run| run.start < line.end)
        {
            let sliced = line.start.max(run.start)..line.end.min(run.end);
            if sliced.is_empty() {
                continue;
            }
            let reshaped;
            let glyphs = if run.start == sliced.start && sliced.end == run.end {
                &run.glyphs[..]
            } else if let Some(glyphs) = run.slice_safe_to_break(&text, sliced.clone()) {
                glyphs
            } else {
                // `ShapedText::reshape`: a run cut where it is not safe to break is shaped again,
                // on the line's piece alone and with the run's direction.
                reshaped = shape_run(face, &text, sliced, run.ltr);
                &reshaped.glyphs[..]
            };
            for (bottom, top) in glyphs.iter().filter_map(|g| g.ink) {
                extent = Some(extent.map_or((bottom, top), |(b, t)| (b.min(bottom), t.max(top))));
            }
        }
        inks.push(extent.map(|(bottom, top)| LineInk {
            rise: top * scale,
            fall: -bottom * scale,
        }));
    }
    inks
}

/// The `ellipsis` line budget (Decision 4): the emitted form of the largest leading run of lines
/// that fits, with its outer ink, or `None` when no run does. The emitted form of `k` lines is the
/// first `k` with every over-wide line shortened in place and, when `k < n`, the marker on line `k`.
///
/// Fitting is not monotonic in `k`, so every `k` is tried downward from the last line whose baseline
/// the box admits, and each try measures its own emitted block, because a line's ink can depend on
/// the lines after it. No line past that cutoff is measured, and the search stops within
/// (largest reservation) / pitch tries, since every step down frees a pitch of height: the work
/// grows with the line count, not its square. `whole_judged` says the block as broken was already
/// measured and refused, so `k = n` needs no second measurement.
fn line_budget(
    face: &rustybuzz::Face<'_>,
    raw_lines: &[String],
    whole_judged: bool,
    size_pt: f32,
    pitch: f32,
    bounds_pt: (f32, f32),
    vertical: VerticalAlign,
) -> Option<(Vec<String>, BlockInk)> {
    let (width_pt, height_pt) = bounds_pt;
    let n = raw_lines.len();
    let cap = cap_height(face, size_pt);
    let k_metric = (1..=n)
        .take_while(|&k| fits_height(baseline(cap, pitch, k), height_pt))
        .count();
    let forms: Vec<String> = raw_lines[..k_metric]
        .iter()
        .map(|raw| {
            if fits_width(face, raw, size_pt, width_pt) {
                raw.clone()
            } else {
                ellipsize(face, raw, size_pt, width_pt)
            }
        })
        .collect();

    for k in (1..=k_metric).rev() {
        if k == n && whole_judged {
            continue;
        }
        let mut emitted = forms[..k].to_vec();
        if k < n {
            emitted[k - 1] = ellipsize(face, &raw_lines[k - 1], size_pt, width_pt);
        }
        let ink = block_ink(&line_inks(face, &emitted, size_pt), cap, pitch);
        if fits_height(baseline(cap, pitch, k) + ink.reserve(vertical), height_pt) {
            return Some((emitted, ink));
        }
    }
    None
}

#[cfg(test)]
thread_local! {
    static SHAPED_LINES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many lines `shape_line_ink` has shaped on this thread since the last call (task 2.2). Per
/// thread, because the test harness runs tests concurrently and layout never leaves the caller's.
#[cfg(test)]
pub(super) fn take_shaped_lines() -> usize {
    SHAPED_LINES.with(|count| count.replace(0))
}

/// The vertical extent of a glyph's outline in font units, `(bottom, top)`, exact for the instance
/// `face` holds. `ttf_parser::Face::glyph_bounding_box` truncates a variable instance's bounds to
/// integers, which drops the bold `É`'s accent from 1939.7 units to 1939: 0.025 pt at 72 pt, past
/// the fit's 0.01 pt tolerance. Tracing the outline keeps the fraction, and each curve's own
/// extremum counts, not its control points. `None` for a glyph with no outline, such as a space.
pub(super) fn glyph_ink(face: &ttf_parser::Face, glyph: ttf_parser::GlyphId) -> Option<(f32, f32)> {
    struct Extent {
        last: f32,
        bottom: f32,
        top: f32,
    }
    impl Extent {
        fn add(&mut self, y: f32) {
            self.bottom = self.bottom.min(y);
            self.top = self.top.max(y);
        }
        /// Where a curve's derivative `a·t² + b·t + c` vanishes inside the segment. The roots use
        /// the form that does not cancel when `a` is small beside `b`.
        fn add_turning_points(&mut self, (a, b, c): (f32, f32, f32), at: impl Fn(f32) -> f32) {
            let disc = b * b - 4.0 * a * c;
            let roots = if a == 0.0 {
                [(b != 0.0).then(|| -c / b), None]
            } else if disc < 0.0 {
                [None, None]
            } else {
                let q = -0.5 * (b + b.signum() * disc.sqrt());
                [(q != 0.0).then(|| q / a), (q != 0.0).then(|| c / q)]
            };
            for t in roots.into_iter().flatten().filter(|t| *t > 0.0 && *t < 1.0) {
                self.add(at(t));
            }
        }
    }
    impl ttf_parser::OutlineBuilder for Extent {
        fn move_to(&mut self, _: f32, y: f32) {
            self.add(y);
            self.last = y;
        }
        fn line_to(&mut self, _: f32, y: f32) {
            self.add(y);
            self.last = y;
        }
        fn quad_to(&mut self, _: f32, y1: f32, _: f32, y: f32) {
            let p0 = self.last;
            self.add_turning_points((0.0, 2.0 * (p0 - 2.0 * y1 + y), 2.0 * (y1 - p0)), |t| {
                let u = 1.0 - t;
                u * u * p0 + 2.0 * u * t * y1 + t * t * y
            });
            self.add(y);
            self.last = y;
        }
        fn curve_to(&mut self, _: f32, y1: f32, _: f32, y2: f32, _: f32, y: f32) {
            let p0 = self.last;
            let (q0, q1, q2) = (y1 - p0, y2 - y1, y - y2);
            self.add_turning_points((q0 - 2.0 * q1 + q2, 2.0 * (q1 - q0), q0), |t| {
                let u = 1.0 - t;
                u * u * u * p0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y
            });
            self.add(y);
            self.last = y;
        }
        fn close(&mut self) {}
    }
    let mut extent = Extent {
        last: 0.0,
        bottom: f32::INFINITY,
        top: f32::NEG_INFINITY,
    };
    face.outline_glyph(glyph, &mut extent)?;
    (extent.bottom <= extent.top).then_some((extent.bottom, extent.top))
}

/// Typst's `is_generic_script`, copied verbatim (`typst-layout` `src/inline/shaping.rs`).
fn is_generic_script(script: Script) -> bool {
    matches!(script, Script::Unknown | Script::Common | Script::Inherited)
}

/// Typst's `is_compatible`, copied verbatim.
fn is_compatible(a: Script, b: Script) -> bool {
    is_generic_script(a) || is_generic_script(b) || a == b
}

/// One glyph as Typst's `shape_segment` leaves it: the text it stands for, whether a line may begin
/// or end at it without reshaping, and its outline's vertical extent in font units, moved by its
/// shaped `y_offset`.
#[derive(Debug, Clone, Copy)]
struct ShapedGlyph {
    start: usize,
    end: usize,
    safe_to_break: bool,
    ink: Option<(f32, f32)>,
}

/// A run of the paragraph shaped as one buffer: Typst's `ShapedText`, reduced to what ink needs.
struct ShapedRun {
    start: usize,
    end: usize,
    ltr: bool,
    glyphs: Vec<ShapedGlyph>,
}

impl ShapedRun {
    /// Typst's `ShapedText::slice_safe_to_break`, copied: the glyphs standing for `range` when both
    /// of its ends are safe to break.
    fn slice_safe_to_break(&self, text: &str, range: Range<usize>) -> Option<&[ShapedGlyph]> {
        let (mut start, mut end) = (range.start, range.end);
        if !self.ltr {
            std::mem::swap(&mut start, &mut end);
        }
        let left = self.find_safe_to_break(text, start)?;
        let right = self.find_safe_to_break(text, end)?;
        Some(&self.glyphs[left..right])
    }

    /// Typst's `ShapedText::find_safe_to_break`, copied.
    fn find_safe_to_break(&self, text: &str, text_index: usize) -> Option<usize> {
        let len = self.glyphs.len();
        if text_index == self.start {
            return Some(if self.ltr { 0 } else { len });
        } else if text_index == self.end {
            return Some(if self.ltr { len } else { 0 });
        }
        let found = self.glyphs.binary_search_by(|g| {
            let ordering = g.start.cmp(&text_index);
            if self.ltr {
                ordering
            } else {
                ordering.reverse()
            }
        });
        let mut idx = match found {
            Ok(idx) => idx,
            // A `\n` has no glyph, and breaking before one is safe.
            Err(idx) => {
                return (idx > 0
                    && self.glyphs[idx - 1].end == text_index
                    && text[text_index..].starts_with('\n'))
                .then_some(idx);
            }
        };
        let dec = if self.ltr {
            usize::checked_sub
        } else {
            usize::checked_add
        };
        while let Some(next) = dec(idx, 1) {
            if self.glyphs.get(next).is_none_or(|g| g.start != text_index) {
                break;
            }
            idx = next;
        }
        self.glyphs[idx]
            .safe_to_break
            .then_some(idx + usize::from(!self.ltr))
    }
}

/// Typst shapes no run of only newlines, tabs or default ignorables (`shape_segment`).
fn draws_nothing(text: &str) -> bool {
    text.chars()
        .all(|c| c == '\n' || c == '\t' || typst::text::is_default_ignorable(c))
}

/// Typst's `shape_range` over a whole paragraph (`src/inline/shaping.rs`): BiDi levels, then runs of
/// one level and one script, where a character of no specific script joins the run around it, each
/// run shaped on its own. `Smart::Auto` script throughout: labeler never sets `text.script`.
fn shape_paragraph(face: &rustybuzz::Face<'_>, text: &str) -> Vec<ShapedRun> {
    let bidi = unicode_bidi::BidiInfo::new(text, Some(unicode_bidi::Level::ltr()));
    let mut runs = Vec::new();
    let mut prev_level = unicode_bidi::Level::ltr();
    let mut prev_script = Script::Unknown;
    let mut cursor = 0;
    for i in 0..text.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        let level = bidi.levels[i];
        let curr_script = text[i..]
            .chars()
            .next()
            .map_or(Script::Unknown, |c| c.script());
        if level != prev_level || !is_compatible(curr_script, prev_script) {
            if cursor < i {
                runs.push(shape_run(face, text, cursor..i, prev_level.is_ltr()));
            }
            cursor = i;
            prev_level = level;
            prev_script = curr_script;
        } else if is_generic_script(prev_script) {
            prev_script = curr_script;
        }
    }
    runs.push(shape_run(
        face,
        text,
        cursor..text.len(),
        prev_level.is_ltr(),
    ));
    runs
}

/// Typst's `shape_segment` on `text[range]`, with the bundled font as the only family. The buffer
/// takes Typst's default language, `en` (labeler sets none), and no features: `features()` adds one
/// only when a text setting departs from HarfBuzz's defaults, and the emitted source sets none.
fn shape_run(face: &rustybuzz::Face<'_>, text: &str, range: Range<usize>, ltr: bool) -> ShapedRun {
    let base = range.start;
    let segment = &text[range.clone()];
    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    if !draws_nothing(segment) {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(segment);
        buffer.set_language(rustybuzz::Language::from_str("en").expect("valid language tag"));
        buffer.set_direction(if ltr {
            rustybuzz::Direction::LeftToRight
        } else {
            rustybuzz::Direction::RightToLeft
        });
        buffer.guess_segment_properties();
        buffer.set_flags(rustybuzz::BufferFlags::REMOVE_DEFAULT_IGNORABLES);
        let shaped = rustybuzz::shape(face, &[], buffer);
        let (infos, positions) = (shaped.glyph_infos(), shaped.glyph_positions());

        let glyph = |i: usize| {
            // The glyph's text runs to the next cluster in logical order.
            let step: isize = if ltr { 1 } else { -1 };
            let mut k = i;
            let end = loop {
                match k
                    .checked_add_signed(step)
                    .and_then(|n| infos.get(n).map(|g| (n, g)))
                {
                    None => break base + segment.len(),
                    Some((n, next)) if next.cluster == infos[i].cluster => k = n,
                    Some((_, next)) => break base + next.cluster as usize,
                }
            };
            // The outline is the instance `set_variation` selected, not the default master. A glyph
            // with no outline, such as a space, contributes nothing.
            let offset = positions[i].y_offset as f32;
            ShapedGlyph {
                start: base + infos[i].cluster as usize,
                end,
                safe_to_break: !infos[i].unsafe_to_break(),
                ink: glyph_ink(face, ttf_parser::GlyphId(infos[i].glyph_id as u16))
                    .map(|(bottom, top)| (bottom + offset, top + offset)),
            }
        };

        let mut i = 0;
        while i < infos.len() {
            if infos[i].glyph_id != 0 {
                glyphs.push(glyph(i));
            } else {
                // A sequence the font lacks: Typst trims the half-shaped cluster before it and
                // shapes the sequence again with the next family. Newlines, tabs and ignorables
                // draw nothing there either. Anything else is drawn by a fallback font this does not
                // read (#254), so it measures as this font's `.notdef`.
                let k = i;
                while infos.get(i + 1).is_some_and(|info| info.glyph_id == 0) {
                    i += 1;
                }
                let start = infos[if ltr { k } else { i }].cluster as usize;
                let end = if ltr {
                    i.checked_add(1)
                } else {
                    k.checked_sub(1)
                }
                .and_then(|last| infos.get(last))
                .map_or(segment.len(), |info| info.cluster as usize);
                let remove = base + start..base + end;
                while glyphs.last().is_some_and(|g| remove.contains(&g.start)) {
                    glyphs.pop();
                }
                if !draws_nothing(&segment[start..end]) {
                    glyphs.extend((k..=i).map(glyph));
                }
            }
            i += 1;
        }
    }
    ShapedRun {
        start: range.start,
        end: range.end,
        ltr,
        glyphs,
    }
}

/// One line's ink as Typst draws it on its own: `line_inks` for a one-line block.
#[cfg(test)]
pub(super) fn shape_line_ink(
    face: &rustybuzz::Face<'_>,
    line: &str,
    size_pt: f32,
) -> Option<LineInk> {
    line_inks(face, &[line], size_pt).pop().flatten()
}

/// The outer ink of `lines` set at `pitch_pt`, each line shaped on `face` at `size_pt`.
#[cfg(test)]
pub(super) fn measure_block_ink(
    face: &rustybuzz::Face<'_>,
    lines: &[impl AsRef<str>],
    size_pt: f32,
    pitch_pt: f32,
) -> BlockInk {
    block_ink(
        &line_inks(face, lines, size_pt),
        cap_height(face, size_pt),
        pitch_pt,
    )
}

pub(super) const DEFAULT_LINE_SPACING: f32 = 1.2;

pub(super) fn resolve_line_spacing(line_spacing: Option<f32>) -> f32 {
    line_spacing.unwrap_or(DEFAULT_LINE_SPACING)
}

pub(super) fn line_pitch(size: f32, line_spacing: f32) -> f32 {
    line_spacing * size
}

pub(super) fn derived_leading(face: &ttf_parser::Face, size: f32, line_spacing: f32) -> f32 {
    line_pitch(size, line_spacing) - cap_height(face, size)
}

pub(super) fn derived_leading_pt(
    weight: u16,
    size: f32,
    line_spacing: Option<f32>,
) -> Result<f32, AppError> {
    let face = instance(weight, size)?;
    Ok(derived_leading(
        &face,
        size,
        resolve_line_spacing(line_spacing),
    ))
}

/// Height of an `n`-line metric block as Typst stacks it: `cap_height(s) + (n - 1) * pitch(s)`.
fn metric_block_height(
    face: &ttf_parser::Face,
    size: f32,
    lines: usize,
    line_spacing: Option<f32>,
) -> f32 {
    if lines == 0 {
        return 0.0;
    }
    let n = lines as f32;
    let pitch = line_pitch(size, resolve_line_spacing(line_spacing));
    cap_height(face, size) + (n - 1.0) * pitch
}

/// The reserved demand: the metric block height Typst lays out plus the reservation for the ink
/// `lines` carry, for the item's vertical alignment. Layout judges through `judge_lines`, which
/// keeps the per-line ink for the line budget; this is the same sum for tests that want a number.
#[cfg(test)]
pub(super) fn block_height(
    face: &rustybuzz::Face<'_>,
    size: f32,
    lines: &[impl AsRef<str>],
    line_spacing: Option<f32>,
    vertical: VerticalAlign,
) -> f32 {
    let pitch = line_pitch(size, resolve_line_spacing(line_spacing));
    let ink = measure_block_ink(face, lines, size, pitch);
    metric_block_height(face, size, lines.len(), line_spacing) + ink.reserve(vertical)
}

#[cfg(test)]
pub(crate) fn block_height_for_test(weight: u16, size: f32, lines: usize) -> f32 {
    let face = instance(weight, size).expect("face");
    metric_block_height(&face, size, lines, None)
}

#[cfg(test)]
pub(crate) fn block_height_with_spacing_for_test(
    weight: u16,
    size: f32,
    lines: usize,
    line_spacing: Option<f32>,
) -> f32 {
    let face = instance(weight, size).expect("face");
    metric_block_height(&face, size, lines, line_spacing)
}

#[cfg(test)]
pub(crate) fn block_height_with_align_for_test(
    weight: u16,
    size: f32,
    lines: &[impl AsRef<str>],
    vertical: VerticalAlign,
) -> f32 {
    let face = instance(weight, size).expect("face");
    block_height(&face, size, lines, None, vertical)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn block_height_with_align_and_spacing_for_test(
    weight: u16,
    size: f32,
    lines: &[impl AsRef<str>],
    line_spacing: Option<f32>,
    vertical: VerticalAlign,
) -> f32 {
    let face = instance(weight, size).expect("face");
    block_height(&face, size, lines, line_spacing, vertical)
}

#[cfg(test)]
pub(crate) fn text_width_for_test(weight: u16, size: f32, text: &str) -> f32 {
    let face = instance(weight, size).expect("face");
    text_width(&face, text, size)
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

    fn from_mime(mime: &str, item_path: &str) -> Result<Self, AppError> {
        match mime.trim() {
            "image/png" => Ok(ImageFmt::Png),
            "image/jpeg" => Ok(ImageFmt::Jpg),
            "image/svg+xml" => Ok(ImageFmt::Svg),
            other => Err(AppError::unsupported_layout_item(
                Reason::ImageFormatUnsupported,
                format!("at {item_path}: unsupported image type '{other}'"),
            )),
        }
    }

    fn from_path(path: &str, item_path: &str) -> Result<Self, AppError> {
        let ext = path.rsplit('.').next().map(|e| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("png") => Ok(ImageFmt::Png),
            Some("jpg") | Some("jpeg") => Ok(ImageFmt::Jpg),
            Some("svg") => Ok(ImageFmt::Svg),
            _ => Err(AppError::unsupported_layout_item(
                Reason::ImageFormatUnsupported,
                format!("at {item_path}: unsupported image extension for '{path}'"),
            )),
        }
    }
}

pub(super) fn assets_root() -> PathBuf {
    crate::resolve_dir(std::env::var_os("LABELER_CONFIG_DIR"), "/config").join("assets")
}

/// Decodes the data URI the image at `item_path` reads; every failure names that path.
pub(super) fn parse_image_data_uri(
    value: &str,
    item_path: &str,
) -> Result<(Vec<u8>, ImageFmt), AppError> {
    let invalid = |msg: &str| {
        AppError::unsupported_layout_item(
            Reason::ImageDataInvalid,
            format!("at {item_path}: {msg}"),
        )
    };
    let rest = value
        .strip_prefix("data:")
        .ok_or_else(|| invalid("image data must be a base64 data URI"))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| invalid("malformed image data URI"))?;
    let mut params = meta.split(';');
    let mime = params.next().unwrap_or("");
    if !params.any(|p| p.eq_ignore_ascii_case("base64")) {
        return Err(invalid("image data URI must be base64-encoded"));
    }
    let fmt = ImageFmt::from_mime(mime, item_path)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|_| invalid("image data is not valid base64"))?;
    Ok((bytes, fmt))
}

/// Reads the asset the image at `item_path` names; every failure names that path.
pub(super) fn resolve_image_asset(
    root: &Path,
    src: &str,
    item_path: &str,
) -> Result<(Vec<u8>, ImageFmt), AppError> {
    let fmt = ImageFmt::from_path(src, item_path)?;
    let failed = |reason: Reason, msg: String| {
        AppError::unsupported_layout_item(reason, format!("at {item_path}: {msg}"))
    };
    let canon_root = root.canonicalize().map_err(|_| {
        failed(
            Reason::AssetsDirUnavailable,
            "assets directory is not available".to_string(),
        )
    })?;
    let candidate = canon_root.join(src);
    let canon = candidate.canonicalize().map_err(|_| {
        failed(
            Reason::ImageAssetMissing,
            format!("image asset not found: {src}"),
        )
    })?;
    if !canon.starts_with(&canon_root) {
        return Err(failed(
            Reason::ImageAssetPathEscapes,
            "image asset path escapes the assets directory".to_string(),
        ));
    }
    let bytes = std::fs::read(&canon).map_err(|_| {
        failed(
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
        let (bytes, fmt) = parse_image_data_uri(&uri, "layout[0]").expect("parse");
        assert!(!bytes.is_empty());
        assert_eq!(fmt.ext(), "png");
    }

    #[test]
    fn parse_data_uri_rejects_non_data_uri() {
        assert!(parse_image_data_uri("not-a-data-uri", "layout[0]").is_err());
    }

    #[test]
    fn parse_data_uri_rejects_bad_base64() {
        assert!(
            parse_image_data_uri("data:image/png;base64,@@@not base64@@@", "layout[0]").is_err()
        );
    }

    #[test]
    fn parse_data_uri_rejects_unsupported_mime() {
        let uri = format!("data:image/gif;base64,{PNG_1X1_B64}");
        assert!(parse_image_data_uri(&uri, "layout[0]").is_err());
    }

    #[test]
    fn resolve_asset_reads_file_under_root() {
        let dir = unique_dir("ok");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(PNG_1X1_B64)
            .unwrap();
        fs::write(dir.join("logo.png"), &bytes).unwrap();
        let (got, fmt) = resolve_image_asset(&dir, "logo.png", "layout[0]").expect("resolve");
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
        assert!(resolve_image_asset(&root, &rel, "layout[0]").is_err());
        fs::remove_file(&secret).ok();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_asset_missing_file_errors() {
        let dir = unique_dir("missing");
        assert!(resolve_image_asset(&dir, "nope.png", "layout[0]").is_err());
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
                None,
                VerticalAlign::Center,
                (6.0, 20.0),
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
                None,
                VerticalAlign::Center,
                (6.0, 20.0),
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
                line_spacing: None,
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

    /// *A box too short for one line cannot be shortened*: shortening a line cannot buy vertical
    /// room, so `ellipsis` refuses once no leading run fits, which here is every run. There is no
    /// separate one-line floor: the refusal is the search coming up empty.
    #[test]
    fn layout_text_ellipsis_refuses_a_box_shorter_than_one_line() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        for overflow in [Overflow::Ellipsis, Overflow::Fail] {
            let err = test_layout(
                "ABC",
                &FontSize::Fixed(20.0),
                false,
                align.clone(),
                overflow,
                (40.0, 2.0),
            )
            .expect_err("a box shorter than one line cannot be shortened");
            assert_eq!(err.reason(), Some("text_does_not_fit"));
        }
        let err = test_layout(
            "ABC",
            &FontSize::Fixed(20.0),
            false,
            align,
            Overflow::Ellipsis,
            (40.0, 2.0),
        )
        .expect_err("refused");
        assert!(
            err.message_text().contains("no leading run of lines fits"),
            "got {}",
            err.message_text()
        );
    }

    /// An over-wide line is shortened in place, wherever it sits in the block. Every
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
            "W W",
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
        // One cap height at the 10 pt maximum, with no reservation: a blank line carries no ink.
        let cap = super::pt_to_units(1490.0 / 2048.0 * 10.0, "mm");
        assert!((m.height_units - cap).abs() < 1e-5, "{} mm", m.height_units);
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
        let line_1_h_pt = super::block_height(&face, 10.0, &["message"], None, VerticalAlign::Top);
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
        let line_1_h_pt = super::block_height(&face, 10.0, &["message"], None, VerticalAlign::Top);
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

    #[test]
    fn layout_text_center_aligned_refusals() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center,
        };
        // Case 1: 3-line block with descenders where block plus reservation exceeds box.
        let face = super::instance(400, 20.0).unwrap();
        let box_h_pt = super::metric_block_height(&face, 20.0, 3, None);
        let box_h_mm = super::pt_to_units(box_h_pt, "mm");
        let err_fail = test_layout(
            "gyp\ngyp\ngyp",
            &FontSize::Fixed(20.0),
            true,
            align.clone(),
            Overflow::Fail,
            (100.0, box_h_mm),
        )
        .expect_err("overflow: fail must reject when block plus reservation exceeds box");
        assert_eq!(err_fail.reason(), Some("text_does_not_fit"));

        // Case 2: 1-line item in a box shorter than one line (e.g. 5.0pt).
        let short_box_mm = super::pt_to_units(5.0, "mm");
        let err_ellipsis = test_layout(
            "One line",
            &FontSize::Fixed(20.0),
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
                .contains("no leading run of lines fits"),
            "got {}",
            err_ellipsis.message_text()
        );
    }

    /// Task 1.1: A `wrap: true` text item with font_size range whose word is too wide at max
    /// but fits whole in range: assert it renders whole on one line at the largest such size.
    #[test]
    fn layout_text_over_wide_word_shrinks_whole_instead_of_breaking() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        };
        let word = "Refrigeration";
        let face_10 = super::instance(400, 10.0).unwrap();
        let w_10 = super::text_width(&face_10, word, 10.0);
        let box_w = super::pt_to_units(w_10 + 0.1, "mm");

        let m = test_layout(
            word,
            &FontSize::Range {
                min: 6.0,
                max: 20.0,
            },
            true,
            align,
            Overflow::Fail,
            (box_w, 20.0),
        )
        .expect("word should fit after shrinking");

        assert_eq!(
            m.lines,
            vec!["Refrigeration".to_string()],
            "word must be kept whole on one line, not chunked across lines"
        );
        assert_eq!(m.font_size_pt, 10.0);
    }

    /// Task 1.2: A `wrap: true` text item at min font_size with an over-wide word:
    /// ellipsis shortens with `...` on a single line; fail returns text_does_not_fit.
    #[test]
    fn layout_text_over_wide_word_at_floor_ellipsis_and_fail() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        };
        let word = "Refrigeration";
        let face_10 = super::instance(400, 10.0).unwrap();
        let dot_w = super::text_width(&face_10, "...", 10.0);
        let word_w = super::text_width(&face_10, word, 10.0);
        // Box is wider than "..." but narrower than "Refrigeration" at 10pt (which is min)
        let box_w = super::pt_to_units((dot_w + word_w) / 2.0, "mm");

        // 1. Ellipsis: renders shortened form with "..."
        let m = test_layout(
            word,
            &FontSize::Range {
                min: 10.0,
                max: 20.0,
            },
            true,
            align.clone(),
            Overflow::Ellipsis,
            (box_w, 20.0),
        )
        .expect("should produce shortened form with ellipsis");

        assert_eq!(m.font_size_pt, 10.0);
        assert_eq!(
            m.lines.len(),
            1,
            "expected 1 shortened line, got {:?}",
            m.lines
        );
        assert!(
            m.lines[0].ends_with("..."),
            "expected line to end with '...', got {:?}",
            m.lines[0]
        );

        // 2. Fail: returns text_does_not_fit
        let err = test_layout(
            word,
            &FontSize::Range {
                min: 10.0,
                max: 20.0,
            },
            true,
            align,
            Overflow::Fail,
            (box_w, 20.0),
        )
        .expect_err("fail policy must return error when word overflows at min");
        assert_eq!(err.reason(), Some("text_does_not_fit"));
    }

    /// Task 1.3: A `wrap: true` text item with fixed font_size and an over-wide word:
    /// ellipsis shortens with marker, fail returns text_does_not_fit, and box narrower
    /// than marker fails under ellipsis.
    #[test]
    fn layout_text_over_wide_word_at_fixed_size_overflow_outcomes() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        };
        let word = "Refrigeration";
        let face_10 = super::instance(400, 10.0).unwrap();
        let dot_w = super::text_width(&face_10, "...", 10.0);
        let word_w = super::text_width(&face_10, word, 10.0);
        let box_w = super::pt_to_units((dot_w + word_w) / 2.0, "mm");

        // 1. Ellipsis: shortens with marker instead of splitting
        let m = test_layout(
            word,
            &FontSize::Fixed(10.0),
            true,
            align.clone(),
            Overflow::Ellipsis,
            (box_w, 20.0),
        )
        .expect("fixed font ellipsis should shorten");
        assert_eq!(m.lines.len(), 1, "expected 1 line, got {:?}", m.lines);
        assert!(
            m.lines[0].ends_with("..."),
            "expected line to end with '...', got {:?}",
            m.lines[0]
        );

        // 2. Fail: returns text_does_not_fit
        let err_fail = test_layout(
            word,
            &FontSize::Fixed(10.0),
            true,
            align.clone(),
            Overflow::Fail,
            (box_w, 20.0),
        )
        .expect_err("fail policy must return text_does_not_fit");
        assert_eq!(err_fail.reason(), Some("text_does_not_fit"));

        // 3. Narrower than '...' under ellipsis: fails with text_does_not_fit
        let narrow_box_w = super::pt_to_units(dot_w / 2.0, "mm");
        let err_narrow = test_layout(
            word,
            &FontSize::Fixed(10.0),
            true,
            align,
            Overflow::Ellipsis,
            (narrow_box_w, 20.0),
        )
        .expect_err("box narrower than marker must fail under ellipsis");
        assert_eq!(err_narrow.reason(), Some("text_does_not_fit"));
    }

    /// Task 1.4: A `wrap: true` text item where the first line is over-wide and the last fits
    /// with no line dropped: assert the marker sits on the shortened first line and the last line
    /// is emitted untouched.
    #[test]
    fn layout_text_over_wide_first_line_shortened_in_place_and_last_line_intact() {
        let align = Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        };
        let face = super::instance(400, 10.0).unwrap();
        let dot_w = super::text_width(&face, "...", 10.0);
        let long_word_w = super::text_width(&face, "Refrigeration", 10.0);
        let ok_w = super::text_width(&face, "ok", 10.0);
        // Box is wide enough for "ok" and "...", but narrower than "Refrigeration"
        let box_w = super::pt_to_units(ok_w.max(dot_w) + 2.0, "mm");
        assert!(super::units_to_pt(box_w, "mm") < long_word_w);

        // Box height is tall enough for 2 lines at 10pt
        let h_2_lines_pt = super::block_height(
            &face,
            10.0,
            &["Refrigeration", "ok"],
            None,
            VerticalAlign::Top,
        );
        let box_h = super::pt_to_units(h_2_lines_pt + 2.0, "mm");

        let m = test_layout(
            "Refrigeration ok",
            &FontSize::Fixed(10.0),
            true,
            align,
            Overflow::Ellipsis,
            (box_w, box_h),
        )
        .expect("layout should succeed with in-place shortening");

        assert_eq!(
            m.lines.len(),
            2,
            "expected exactly 2 lines (first shortened, second intact), got {:?}",
            m.lines
        );
        assert!(
            m.lines[0].ends_with("..."),
            "first line must be shortened in place with '...', got {:?}",
            m.lines[0]
        );
        assert_eq!(
            m.lines[1], "ok",
            "second line must be emitted untouched as 'ok'"
        );
    }

    fn top() -> Alignment {
        Alignment {
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        }
    }

    /// Inter's cap height at `size`: 1490 of 2048 units.
    fn cap_pt(size: f32) -> f32 {
        1490.0 / 2048.0 * size
    }

    /// *A value of ten thousand lines is shortened like a value of three*: the budget shapes the
    /// lines a run can hold and one marker form per run tried, and nothing shapes a block its
    /// metric height already refuses.
    #[test]
    fn a_value_of_ten_thousand_lines_is_shortened_like_a_value_of_three() {
        let value = vec!["HELIX"; 10_000].join("\n");
        let segments: Vec<&str> = value.split('\n').collect();
        let two_lines = cap_pt(10.0) + 12.0 + 1.0;
        let face = super::instance(400, 10.0).expect("face");
        super::take_shaped_lines();
        assert!(!super::text_fits(
            &face,
            &segments,
            false,
            10.0,
            None,
            (super::units_to_pt(100.0, "mm"), two_lines),
            VerticalAlign::Top,
        ));
        assert_eq!(
            super::take_shaped_lines(),
            0,
            "text_fits shaped a block its metric refuses"
        );

        let fit = test_layout(
            &value,
            &FontSize::Fixed(10.0),
            false,
            top(),
            Overflow::Ellipsis,
            (100.0, super::pt_to_units(two_lines, "mm")),
        )
        .expect("two lines fit");
        let shaped = super::take_shaped_lines();
        assert_eq!(fit.lines, ["HELIX", "HELIX..."]);
        assert!(shaped <= 4, "the budget shaped {shaped} lines");

        // A pitch small enough that every baseline lies in the box, which is still too short for
        // the last line's `g`: each line is shaped once for the fit and the budget reuses it.
        let value = vec!["Hg"; 10_000].join("\n");
        let metric = cap_pt(10.0) + 9_999.0 * 5.0;
        let fit = layout_text(
            super::TextLayoutItem {
                raw_text: &value,
                font_size: &FontSize::Fixed(10.0),
                font_weight: None,
                wrap: false,
                line_spacing: Some(0.5),
                alignment: top(),
                overflow: Overflow::Ellipsis,
            },
            (100.0, super::pt_to_units(metric + 1.0, "mm")),
            "mm",
            "layout[0]",
        )
        .expect("a shorter run fits");
        let shaped = super::take_shaped_lines();
        assert_eq!(fit.lines.len(), 9_999);
        assert_eq!(fit.lines[9_998], "Hg...");
        assert!(
            (10_000..=20_000).contains(&shaped),
            "{shaped} lines shaped for 10,000"
        );
    }

    /// *The metric cutoff honours the fit tolerance*: 20 pt `HELIX` has a 14.55078125 pt metric
    /// block and no ink outside it, so both cutoffs sit exactly where the fit comparison does.
    #[test]
    fn the_metric_cutoff_honours_the_fit_tolerance() {
        for overflow in [Overflow::Fail, Overflow::Ellipsis] {
            let fit = |height_pt: f32| {
                test_layout(
                    "HELIX",
                    &FontSize::Fixed(20.0),
                    false,
                    top(),
                    overflow,
                    (100.0, super::pt_to_units(height_pt, "mm")),
                )
            };
            let inside = fit(cap_pt(20.0) - 0.005).expect("0.005 pt short fits within tolerance");
            assert_eq!(inside.lines, ["HELIX"]);
            let err = fit(cap_pt(20.0) - 0.015).expect_err("0.015 pt short is refused");
            assert_eq!(err.reason(), Some("text_does_not_fit"));
        }

        // The same boundary inside the line budget. Line 1 is too wide, so the fit declines the
        // block as broken and only the budget's cutoff decides whether line 2 can be kept. Neither
        // run inks outside its metric box: the marker's dots sit a pitch above the last baseline.
        let budget = |height_pt: f32| {
            test_layout(
                "HELIXHELIXHELIXHELIX\nHELIX",
                &FontSize::Fixed(20.0),
                false,
                top(),
                Overflow::Ellipsis,
                (30.0, super::pt_to_units(height_pt, "mm")),
            )
            .expect("a run fits")
        };
        let two_lines = cap_pt(20.0) + 24.0;
        let inside = budget(two_lines - 0.005);
        assert_eq!(inside.lines.len(), 2, "{:?}", inside.lines);
        assert!(inside.lines[0].ends_with("..."));
        assert_eq!(inside.lines[1], "HELIX");
        let outside = budget(two_lines - 0.015);
        assert_eq!(outside.lines.len(), 1, "{:?}", outside.lines);
        assert!(outside.lines[0].ends_with("..."));
    }

    /// A tab draws nothing in Typst, which shapes no run of tabs or default ignorables and sends
    /// an unmapped tab inside a run back through that same rule, so it reserves nothing:
    /// bottom-aligned `HELIX` over a tab, or with one inside it, is not inset.
    #[test]
    fn a_line_of_tabs_carries_no_ink() {
        for value in ["HELIX\n\t", "HELIX\tX"] {
            let fit = test_layout(
                value,
                &FontSize::Fixed(20.0),
                false,
                Alignment {
                    horizontal: HorizontalAlign::Left,
                    vertical: VerticalAlign::Bottom,
                },
                Overflow::Fail,
                (100.0, 30.0),
            )
            .expect("fits");
            assert_eq!((fit.a, fit.d), (0.0, 0.0), "{value:?}");
        }
    }

    /// *A height-bound centred item picks a larger size*: `HELIX` inks nothing outside its metric
    /// box, so 20 pt fits a 16 pt box that the font bands held to 13 pt.
    #[test]
    fn a_height_bound_centred_item_picks_a_larger_size() {
        let fit = test_layout(
            "HELIX",
            &FontSize::Range {
                min: 10.0,
                max: 20.0,
            },
            false,
            Alignment {
                horizontal: HorizontalAlign::Center,
                vertical: VerticalAlign::Center,
            },
            Overflow::Ellipsis,
            (200.0, super::pt_to_units(16.0, "mm")),
        )
        .expect("fits");
        assert_eq!(fit.font_size_pt, 20.0);
    }

    /// *A box's verdict follows the ink the value carries*: one cap height holds `HELIX` and
    /// refuses `Égypt`, and shortening never trims characters to shed an accent or a descender.
    #[test]
    fn a_boxs_verdict_follows_the_ink_the_value_carries() {
        let cap_box = (100.0, super::pt_to_units(cap_pt(20.0), "mm"));
        test_layout(
            "HELIX",
            &FontSize::Fixed(20.0),
            false,
            top(),
            Overflow::Fail,
            cap_box,
        )
        .expect("HELIX fits one cap height");
        for overflow in [Overflow::Fail, Overflow::Ellipsis] {
            let err = test_layout(
                "Égypt",
                &FontSize::Fixed(20.0),
                false,
                top(),
                overflow,
                cap_box,
            )
            .expect_err("Égypt's accent and descender do not fit one cap height");
            assert_eq!(err.reason(), Some("text_does_not_fit"));
        }
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
    fn an_absent_parameter_reads_as_empty() {
        assert_eq!(
            interpolate(
                "[{nope}|{gone:short_date}|{tags:join(', ')}]",
                &data(),
                &variables(),
                &no_datetime(),
                None
            )
            .unwrap(),
            "[||]"
        );
    }

    #[test]
    fn missing_variable_errors() {
        let err =
            interpolate("{vars.nope}", &data(), &variables(), &no_datetime(), None).unwrap_err();
        assert_eq!(err.code(), "TemplateInvalid");
        assert_eq!(err.reason(), Some("reference_unresolved"));
        assert_eq!(err.details().unwrap()["field"], "vars.nope");
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
        cap_height, glyph_ink, instance, largest_fitting_font, line_pitch, load_face,
        measure_block_ink, resolve_line_spacing, shape_line_ink, text_width, typo_ascender,
        units_to_pt, BlockInk, FitBox,
    };
    use crate::models::VerticalAlign;

    const UPEM: f32 = 2048.0;
    /// `É`'s accent above cap height at wght 400, opsz 20: 1928 − 1490 units [fontTools].
    const E_ACCENT_20: f32 = (1928.0 - 1490.0) / UPEM * 20.0;

    /// The reservation is the ink the lines carry outside the cap-height-to-baseline box, read
    /// from the instance: `HELIX` carries none, `É` its accent and nothing below, and `Édgy` its
    /// accent above and its descenders below.
    #[test]
    fn overflow_is_the_ink_outside_the_cap_height_line() {
        let face = instance(400, 20.0).expect("face");
        let pitch = line_pitch(20.0, resolve_line_spacing(None));
        let helix = measure_block_ink(&face, &["HELIX"], 20.0, pitch);
        assert_eq!(helix, BlockInk { a: 0.0, d: 0.0 });
        for vertical in [
            VerticalAlign::Top,
            VerticalAlign::Bottom,
            VerticalAlign::Center,
        ] {
            assert_eq!(helix.reserve(vertical), 0.0);
        }

        let e = measure_block_ink(&face, &["É"], 20.0, pitch);
        assert!((e.a - E_ACCENT_20).abs() < 1e-3, "É inks {} pt above", e.a);
        assert_eq!(e.d, 0.0);
        assert_eq!(e.reserve(VerticalAlign::Top), e.a);
        assert_eq!(e.reserve(VerticalAlign::Bottom), e.a);
        assert_eq!(e.reserve(VerticalAlign::Center), 2.0 * e.a);

        let edgy = measure_block_ink(&face, &["Édgy"], 20.0, pitch);
        assert!(
            (edgy.a - E_ACCENT_20).abs() < 1e-3,
            "Édgy inks {} pt above",
            edgy.a
        );
        // g alone falls 432–442 units across instances [fontTools]; y may fall further.
        assert!(
            edgy.d >= 432.0 / UPEM * 20.0,
            "Édgy inks {} pt below",
            edgy.d
        );
        assert_eq!(edgy.reserve(VerticalAlign::Top), edgy.a + edgy.d);
        assert_eq!(edgy.reserve(VerticalAlign::Bottom), edgy.a + edgy.d);
        assert_eq!(
            edgy.reserve(VerticalAlign::Center),
            2.0 * edgy.a.max(edgy.d)
        );
    }

    /// A height-bound item settles at the largest 0.5 pt step whose block plus reservation fits,
    /// the reservation measured at that step's instance. `Hxy` inks below its baseline only, so
    /// `bottom` reserves that depth once and `center` twice, and `center` settles smaller.
    #[test]
    fn a_height_bound_fit_reserves_the_overflow() {
        let fit = FitBox {
            width_units: 400.0,
            height_units: 10.0,
            unit: "mm",
        };
        let height = units_to_pt(10.0, "mm");
        let settle =
            |vertical| largest_fitting_font(&["Hxy"], false, 400, None, vertical, (6.0, 80.0), fit);
        let need = |vertical, size: f32| {
            let face = instance(400, size).expect("face");
            let pitch = line_pitch(size, resolve_line_spacing(None));
            cap_height(&face, size)
                + measure_block_ink(&face, &["Hxy"], size, pitch).reserve(vertical)
        };
        for vertical in [VerticalAlign::Bottom, VerticalAlign::Center] {
            let size = settle(vertical);
            assert!(
                need(vertical, size) <= height + 0.01,
                "{vertical:?} at {size} pt"
            );
            assert!(
                need(vertical, size + 0.5) > height + 0.01,
                "{vertical:?} at {size} pt"
            );
        }
        assert!(settle(VerticalAlign::Center) < settle(VerticalAlign::Bottom));
    }

    /// Task 2.3: one line's ink as Typst shapes it, against the fontTools/HarfBuzz figures in the
    /// design, at the instance each figure was taken from.
    #[test]
    fn line_shaping_ink_figures_match_design_context() {
        let size = 20.0;
        let units = |pt: f32| pt * UPEM / size;
        let face_400 = instance(400, size).expect("face");
        let face_700 = instance(700, size).expect("face");
        let ink = |face, line| shape_line_ink(face, line, size).expect("ink");

        let helix = ink(&face_400, "HELIX");
        assert_eq!(
            helix.rise,
            cap_height(&face_400, size),
            "HELIX rises to cap height"
        );
        assert_eq!(helix.fall, 0.0);

        assert!((units(ink(&face_400, "É").rise) - 1928.0).abs() < 0.1);
        assert!((units(ink(&face_700, "É").rise) - 1939.7).abs() < 0.1);
        assert!(ink(&face_400, "Ǻ").rise > typo_ascender(&face_400) / UPEM * size);
        assert!((432.0..=442.0).contains(&units(ink(&face_400, "g").fall)));
        assert!(ink(&face_400, "...").fall > 0.0);
        // Composition: HarfBuzz composes E + U+0301 into the precomposed glyph.
        assert_eq!(ink(&face_400, "E\u{0301}"), ink(&face_400, "É"));
        // Segmentation: shaped as one buffer the acute stays at 1535; Typst shapes H́ on its own.
        // HarfBuzz reports integer extents; the traced outline puts this acute at 1928.33.
        assert!((units(ink(&face_400, "αH\u{0301}").rise) - 1928.0).abs() < 0.5);

        // Contextual forms: between digits the colon becomes `colon.case`, raised clear of the
        // baseline, where `colon`'s lower dot overshoots below it. So `1:1` inks no lower than `11`.
        let ink_of =
            |c| glyph_ink(&face_400, face_400.glyph_index(c).expect("mapped")).expect("ink");
        assert!(ink_of(':').0 < ink_of('1').0);
        assert_eq!(ink(&face_400, "1:1").fall, ink(&face_400, "11").fall);
    }

    /// Task 2.4: only the block's outer ink counts. In `Hg\nÉH` the descender sits above the last
    /// baseline and the accent below the metric top; swapped, both leave the metric box.
    #[test]
    fn block_measurement_outer_ink() {
        let size = 20.0;
        let face = instance(400, size).expect("face");
        let pitch = line_pitch(size, resolve_line_spacing(None));

        let inner = measure_block_ink(&face, &["Hg", "ÉH"], size, pitch);
        assert_eq!(inner, BlockInk { a: 0.0, d: 0.0 });

        let outer = measure_block_ink(&face, &["ÉH", "Hg"], size, pitch);
        let g = shape_line_ink(&face, "g", size).expect("ink");
        assert!((outer.a - E_ACCENT_20).abs() < 1e-3, "a = {}", outer.a);
        assert!(
            (outer.d - g.fall).abs() < 1e-4,
            "d = {}, g falls {}",
            outer.d,
            g.fall
        );
        assert_eq!(outer.reserve(VerticalAlign::Top), outer.a + outer.d);
        assert_eq!(
            outer.reserve(VerticalAlign::Center),
            2.0 * outer.a.max(outer.d)
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
        data.insert("width_str".to_string(), json!(" 60.0 "));

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
    }

    #[test]
    fn resolve_dynamic_value_f32_missing_and_invalid() {
        let mut data = HashMap::new();
        data.insert("bad".to_string(), json!("not_a_number"));

        let err = resolve_dynamic_value_f32(&DynamicValue::Ref("missing".to_string()), &data)
            .unwrap_err();
        assert_eq!(err.code(), "Internal");

        let err =
            resolve_dynamic_value_f32(&DynamicValue::Ref("bad".to_string()), &data).unwrap_err();
        assert_eq!(err.code(), "InvalidRequest");
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
