use crate::errors::AppError;
use crate::models::{
    Alignment, Dimension, FontSize, HorizontalAlign, Point, QrParams, VerticalAlign,
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
    for px in data.chunks_exact_mut(4) {
        let luma = (77 * px[0] as u32 + 150 * px[1] as u32 + 29 * px[2] as u32) >> 8;
        let v = if luma < 128 { 0u8 } else { 255u8 };
        px[0] = v;
        px[1] = v;
        px[2] = v;
        px[3] = 255;
    }
}

pub(super) fn value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => value.clone(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

/// Substitution-only interpolation (ADR-0010).
///
/// Resolution precedence: `{datetime}` / `{datetime.NAME}` from the resolver first, then
/// `{vars.<key>}` from `variables`, then `{field}` from `data` via `value_to_string`.
/// `{{`/`}}` emit literal braces. An unresolved token or an unmatched brace is an error.
pub(super) fn interpolate(
    template: &str,
    data: &HashMap<String, JsonValue>,
    variables: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<String, AppError> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                    continue;
                }
                let mut token = String::new();
                let mut closed = false;
                for tc in chars.by_ref() {
                    if tc == '}' {
                        closed = true;
                        break;
                    }
                    token.push(tc);
                }
                if !closed {
                    return Err(AppError::invalid_request(
                        Reason::InterpolationSyntax,
                        format!("unterminated '{{' in template '{template}'"),
                    ));
                }
                let resolved = if let Some(dt) = datetime.resolve(&token) {
                    dt?
                } else if let Some(key) = token.strip_prefix("vars.") {
                    variables
                        .get(key)
                        .cloned()
                        .ok_or_else(|| AppError::missing_field(&format!("vars.{key}")))?
                } else {
                    value_to_string(
                        data.get(&token)
                            .ok_or_else(|| AppError::missing_field(&token))?,
                    )
                };
                out.push_str(&resolved);
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
    Ok(out)
}

pub(super) fn resolve_dimension(dimension: &Dimension) -> Result<f32, AppError> {
    match dimension {
        Dimension::Fixed(value) => Ok(*value),
        Dimension::Dynamic { min, max } => max
            .or(*min)
            .ok_or_else(|| AppError::unsupported_format("dynamic dimension missing min/max")),
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

    let mut renderer = code.render::<svg::Color>();
    renderer.quiet_zone(false);
    if let Some(params) = params {
        if let Some(module_size) = params.module_size {
            if module_size > 0.0 {
                let target = (module_size * code.width() as f32).ceil() as u32;
                renderer.min_dimensions(target, target);
            }
        }
        if let Some(quiet_zone) = params.quiet_zone {
            renderer.quiet_zone(quiet_zone > 0.0);
        }
    }

    Ok(renderer.build())
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

/// Largest font in [min_size, max_size] (0.5pt steps) at which `text` fits the box, else min_size.
pub(super) fn largest_fitting_font(
    text: &str,
    multiline: bool,
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
        if text_fits(&face, text, multiline, size, width_pt, height_pt, vertical) {
            return size;
        }
        size -= 0.5;
    }
    min_size
}

pub(super) fn fit_text_to_box(
    text: &str,
    multiline: bool,
    weight: u16,
    vertical: VerticalAlign,
    min_size: f32,
    max_size: f32,
    fit: FitBox,
) -> Result<(f32, String), AppError> {
    let fitted = largest_fitting_font(text, multiline, weight, vertical, min_size, max_size, fit);
    let face = instance(weight, fitted)?;
    let face = &face;
    let width_pt = units_to_pt(fit.width_units, fit.unit);
    let height_pt = units_to_pt(fit.height_units, fit.unit);
    if text_fits(face, text, multiline, fitted, width_pt, height_pt, vertical) {
        if multiline {
            let lines = wrap_text(face, text, fitted, width_pt);
            return Ok((fitted, lines.join("\n")));
        }
        return Ok((fitted, text.to_string()));
    }
    let trimmed = if multiline {
        trim_multiline(face, text, fitted, width_pt, height_pt, vertical)
    } else {
        trim_single_line(face, text, fitted, width_pt)
    };
    Ok((fitted, trimmed))
}

fn text_fits(
    face: &ttf_parser::Face,
    text: &str,
    multiline: bool,
    size: f32,
    width_pt: f32,
    height_pt: f32,
    vertical: VerticalAlign,
) -> bool {
    if multiline {
        let lines = wrap_text(face, text, size, width_pt);
        block_height(face, size, lines.len(), vertical) <= height_pt + 0.01
    } else {
        text_width(face, text, size) <= width_pt + 0.01
            && block_height(face, size, 1, vertical) <= height_pt + 0.01
    }
}

fn trim_single_line(face: &ttf_parser::Face, text: &str, size: f32, width_pt: f32) -> String {
    const ELLIPSIS: &str = "...";
    let mut out = text.to_string();
    if text_width(face, &out, size) <= width_pt {
        return out;
    }
    let ellipsis_width = text_width(face, ELLIPSIS, size);
    if ellipsis_width > width_pt {
        return ELLIPSIS.to_string();
    }
    while !out.is_empty() && text_width(face, &format!("{out}{ELLIPSIS}"), size) > width_pt {
        out.pop();
    }
    format!("{out}{ELLIPSIS}")
}

fn trim_multiline(
    face: &ttf_parser::Face,
    text: &str,
    size: f32,
    width_pt: f32,
    height_pt: f32,
    vertical: VerticalAlign,
) -> String {
    const ELLIPSIS: &str = "...";
    // Inverse of block_height, with the edge reservation taken off the top:
    // n*cap + (n-1)*leading <= H - overflow  =>  n <= (H - overflow + leading) / (cap + leading)
    let max_lines = ((height_pt - overflow_em(face, vertical) * size + leading(size))
        / (cap_height(face, size) + leading(size)))
    .floor()
    .max(1.0) as usize;
    let mut lines = wrap_text(face, text, size, width_pt);
    if lines.len() <= max_lines {
        return lines.join("\n");
    }
    lines.truncate(max_lines);
    let last = lines.last_mut().unwrap();
    let ellipsis_width = text_width(face, ELLIPSIS, size);
    if ellipsis_width > width_pt {
        *last = ELLIPSIS.to_string();
    } else {
        while !last.is_empty() && text_width(face, &format!("{last}{ELLIPSIS}"), size) > width_pt {
            last.pop();
        }
        *last = format!("{last}{ELLIPSIS}");
    }
    lines.join("\n")
}

fn wrap_text(face: &ttf_parser::Face, text: &str, size: f32, width_pt: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let space_width = text_width(face, " ", size);
    let paragraphs: Vec<&str> = text.split('\n').collect();
    for paragraph in paragraphs {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0.0;
        for word in paragraph.split_whitespace() {
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

/// What the *fitter* holds back so the edge the text is not aligned to cannot clip either: a
/// top-aligned block pushed down by its top pad still has descenders near the slot floor. Both
/// overflows, so it is neither `pad_em` nor the 1.21em band — the cap-height line covers the rest.
fn overflow_em(face: &ttf_parser::Face, vertical: VerticalAlign) -> f32 {
    match vertical {
        VerticalAlign::Top | VerticalAlign::Bottom => {
            ascent_overflow_em(face) + descent_overflow_em(face)
        }
        VerticalAlign::Center => 0.0,
    }
}

/// Typst's default paragraph leading, 0.65em (`model/par.rs`). Sits *between* lines only.
fn leading(size: f32) -> f32 {
    size * 0.65
}

/// Height of an `n`-line block as Typst stacks it: leading between lines, not after the last one
/// (typst-layout `collect.rs` pushes it only when `i > 0`). A fused per-line constant — which is what
/// a "line height" is — overshoots by one leading per block and shrinks text that would have fit.
fn block_height(face: &ttf_parser::Face, size: f32, lines: usize, vertical: VerticalAlign) -> f32 {
    let n = lines.max(1) as f32;
    // The overflow is read off *this* face, already instanced at the candidate size: the metrics move
    // with the opsz axis, so a ratio captured earlier would belong to a different instance.
    n * cap_height(face, size) + (n - 1.0) * leading(size) + overflow_em(face, vertical) * size
}

#[cfg(test)]
pub(crate) fn block_height_for_test(weight: u16, size: f32, lines: usize) -> f32 {
    let face = instance(weight, size).expect("face");
    block_height(&face, size, lines, VerticalAlign::Center)
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

#[derive(Debug, Clone)]
pub(super) struct MeasuredText {
    pub font: f32,
    pub lines: Vec<String>,
    pub width: f32,
}

/// Auto-length fit: choose the font, return the (possibly wrapped and ellipsized) lines and the
/// natural width (clamped to the budget). Width is in template units.
///
/// When `multiline` is `false`, only the first input line is considered; the result is always
/// one line (ellipsized on overflow). When `multiline` is `true`, the text is word-wrapped to the
/// budget width, the largest font that fits the height is chosen, and overflow lines are ellipsized.
pub(super) fn fit_text_auto_length(
    raw_text: &str,
    font_size: &FontSize,
    multiline: bool,
    weight: u16,
    vertical: VerticalAlign,
    fit: FitBox,
) -> Result<MeasuredText, AppError> {
    let (budget_w_units, unit) = (fit.width_units, fit.unit);
    let budget_pt = units_to_pt(budget_w_units, unit);
    let height_pt = units_to_pt(fit.height_units, unit);

    if !multiline {
        let line = to_nonbreaking(raw_text.lines().next().unwrap_or(""));
        let size = match font_size {
            FontSize::Fixed(s) => *s,
            FontSize::Range { min, max } => {
                largest_fitting_font(&line, false, weight, vertical, *min, *max, fit)
            }
        };
        let face = instance(weight, size)?;
        let face = &face;
        if text_fits(face, &line, false, size, budget_pt, height_pt, vertical) {
            let w = pt_to_units(text_width(face, &line, size), unit).min(budget_w_units);
            return Ok(MeasuredText {
                font: size,
                lines: vec![line],
                width: w,
            });
        }
        let trimmed = trim_single_line(face, &line, size, budget_pt);
        return Ok(MeasuredText {
            font: size,
            lines: vec![trimmed],
            width: budget_w_units,
        });
    }

    // Multiline: pick the font (shrink to fit the height with wrapping), then produce the final lines.
    let size = match font_size {
        FontSize::Fixed(s) => *s,
        FontSize::Range { min, max } => {
            largest_fitting_font(raw_text, true, weight, vertical, *min, *max, fit)
        }
    };
    let face = instance(weight, size)?;
    let (lines, max_w_pt) = wrap_lines_fit(&face, raw_text, size, budget_pt, height_pt, vertical);
    let width = pt_to_units(max_w_pt, unit).min(budget_w_units);
    Ok(MeasuredText {
        font: size,
        lines,
        width,
    })
}

/// Wrap `text` to `width_pt`, keep only the lines that fit `height_pt` (ellipsizing the last on
/// overflow), and NBSP-treat each kept line so the renderer cannot re-break it. Wrapping happens on
/// real spaces first (NBSP is not whitespace, so it must be applied after wrapping). Returns the
/// NBSP-treated lines plus the longest-line width in points, measured on the raw (pre-NBSP) lines so
/// the width never depends on NBSP and space sharing an advance in the measurement font.
fn wrap_lines_fit(
    face: &ttf_parser::Face,
    text: &str,
    size: f32,
    width_pt: f32,
    height_pt: f32,
    vertical: VerticalAlign,
) -> (Vec<String>, f32) {
    const ELLIPSIS: &str = "...";
    // Inverse of block_height, as in trim_multiline, minus the edge reservation.
    let max_lines = ((height_pt - overflow_em(face, vertical) * size + leading(size))
        / (cap_height(face, size) + leading(size)))
    .floor()
    .max(1.0) as usize;
    let mut lines = wrap_text(face, text, size, width_pt);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let last = lines.last_mut().unwrap();
        let ellipsis_width = text_width(face, ELLIPSIS, size);
        if ellipsis_width > width_pt {
            *last = ELLIPSIS.to_string();
        } else {
            while !last.is_empty()
                && text_width(face, &format!("{last}{ELLIPSIS}"), size) > width_pt
            {
                last.pop();
            }
            *last = format!("{last}{ELLIPSIS}");
        }
    }
    let max_w_pt = lines
        .iter()
        .map(|l| text_width(face, l, size))
        .fold(0.0_f32, f32::max);
    let lines = lines.into_iter().map(|l| to_nonbreaking(&l)).collect();
    (lines, max_w_pt)
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
        // grays: 0, 64, 127 (->black), 128, 200, 255 (->white). RGBA, opaque.
        let mut data = vec![
            0, 0, 0, 255, 64, 64, 64, 255, 127, 127, 127, 255, 128, 128, 128, 255, 200, 200, 200,
            255, 255, 255, 255, 255,
        ];
        binarize_rgba(&mut data);
        for (i, px) in data.chunks_exact(4).enumerate() {
            assert!(px[3] == 255, "alpha forced opaque");
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
    use super::{fit_text_auto_length, largest_fitting_font, FitBox};
    use crate::models::FontSize;
    use crate::models::VerticalAlign;

    #[test]
    fn largest_fitting_font_picks_max_then_steps_down() {
        assert_eq!(
            largest_fitting_font(
                "Hi",
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
                "A long label that cannot fit",
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

    #[test]
    fn auto_length_short_text_is_content_width() {
        let m = fit_text_auto_length(
            "Hi",
            &FontSize::Range {
                min: 6.0,
                max: 20.0,
            },
            false,
            400,
            VerticalAlign::Center,
            FitBox {
                width_units: 200.0,
                height_units: 50.0,
                unit: "mm",
            },
        )
        .unwrap();
        assert_eq!(m.font, 20.0);
        assert!(m.width > 0.0 && m.width < 200.0);
        assert_eq!(m.lines, vec!["Hi".to_string()]);
    }

    #[test]
    fn auto_length_overflow_ellipsizes_at_min_and_uses_budget() {
        let m = fit_text_auto_length(
            "An extremely long label that cannot possibly fit even at the minimum font size",
            &FontSize::Range {
                min: 6.0,
                max: 20.0,
            },
            false,
            400,
            VerticalAlign::Center,
            FitBox {
                width_units: 8.0,
                height_units: 3.0,
                unit: "mm",
            },
        )
        .unwrap();
        assert_eq!(m.font, 6.0);
        assert_eq!(m.lines.len(), 1);
        assert!(m.lines[0].ends_with("...") || m.lines[0].ends_with('\u{2026}'));
        assert!((m.width - 8.0).abs() < 0.01);
    }

    #[test]
    fn auto_length_fixed_font_no_shrink() {
        let m = fit_text_auto_length(
            "Hi",
            &FontSize::Fixed(12.0),
            false,
            400,
            VerticalAlign::Center,
            FitBox {
                width_units: 200.0,
                height_units: 50.0,
                unit: "mm",
            },
        )
        .unwrap();
        assert_eq!(m.font, 12.0);
        assert_eq!(m.lines, vec!["Hi".to_string()]);
    }

    #[test]
    fn auto_length_multiline_wraps_and_width_is_longest_line() {
        // Long text, narrow budget, tall box: should wrap to >1 line; width <= budget.
        let m = fit_text_auto_length(
            "alpha bravo charlie delta",
            &FontSize::Range {
                min: 6.0,
                max: 10.0,
            },
            true,
            400,
            VerticalAlign::Center,
            FitBox { width_units: 20.0, height_units: // budget width units (mm)
            20.0, unit: // box height units (mm): room for several lines
            "mm" },
        )
        .unwrap();
        assert!(m.lines.len() >= 2, "expected wrapping, got {:?}", m.lines);
        assert!(m.width <= 20.0 + 0.01);
        // each line is NBSP-treated (no ASCII space)
        assert!(
            m.lines.iter().all(|l| !l.contains(' ')),
            "lines must be NBSP-joined: {:?}",
            m.lines
        );
    }

    #[test]
    fn auto_length_multiline_short_text_is_single_line() {
        let m = fit_text_auto_length(
            "Hi",
            &FontSize::Range {
                min: 6.0,
                max: 10.0,
            },
            true,
            400,
            VerticalAlign::Center,
            FitBox {
                width_units: 50.0,
                height_units: 20.0,
                unit: "mm",
            },
        )
        .unwrap();
        assert_eq!(m.lines.len(), 1);
    }

    #[test]
    fn auto_length_multiline_overflow_ellipsizes_last_line() {
        // Many words, short height: at min font only a few lines fit; last is ellipsized.
        let m = fit_text_auto_length(
            "one two three four five six seven eight nine ten eleven twelve",
            &FontSize::Range { min: 6.0, max: 6.0 }, // fixed-ish via min==max to force overflow
            true,
            400,
            VerticalAlign::Center,
            FitBox { width_units: 12.0, height_units: // narrow
            6.0, unit: // short height: few lines
            "mm" },
        )
        .unwrap();
        assert!(
            m.lines.last().unwrap().contains("..."),
            "last line should ellipsize: {:?}",
            m.lines
        );
    }

    #[test]
    fn auto_length_multiline_empty_input_is_ok() {
        // Empty input must not panic. `wrap_text` yields a single blank line, so the result is one
        // empty line with zero width.
        let m = fit_text_auto_length(
            "",
            &FontSize::Range {
                min: 6.0,
                max: 10.0,
            },
            true,
            400,
            VerticalAlign::Center,
            FitBox {
                width_units: 50.0,
                height_units: 20.0,
                unit: "mm",
            },
        )
        .unwrap();
        assert_eq!(m.lines, vec![String::new()]);
        assert_eq!(m.width, 0.0);
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
        )
        .unwrap();
        assert_eq!(out, "https://h/i/A1");
    }

    #[test]
    fn stringifies_non_string_field() {
        assert_eq!(
            interpolate("n={count}", &data(), &variables(), &no_datetime()).unwrap(),
            "n=3"
        );
    }

    #[test]
    fn literal_braces() {
        assert_eq!(
            interpolate("{{x}}", &data(), &variables(), &no_datetime()).unwrap(),
            "{x}"
        );
    }

    #[test]
    fn missing_field_errors() {
        assert!(interpolate("{nope}", &data(), &variables(), &no_datetime()).is_err());
    }

    #[test]
    fn missing_variable_errors() {
        assert!(interpolate("{vars.nope}", &data(), &variables(), &no_datetime()).is_err());
    }

    #[test]
    fn unmatched_brace_errors() {
        assert!(interpolate("a{id", &data(), &variables(), &no_datetime()).is_err());
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

        // Center reserves and pads nothing (spec Decision 2).
        assert_eq!(pad_em(&face, VerticalAlign::Center), 0.0);
        assert_eq!(overflow_em(&face, VerticalAlign::Center), 0.0);

        // pad_pt is the same number in points, for callers with no Face.
        let pt = pad_pt(400, 20.0, VerticalAlign::Bottom).expect("pad_pt");
        assert!((pt - bottom * 20.0).abs() < 1e-4, "pad_pt {pt}");
    }

    /// A height-bound item must leave room for the ink outside the cap-height line, so an aligned
    /// item fits at a smaller size than a centered one in the same slot (#124).
    #[test]
    fn a_height_bound_aligned_fit_reserves_the_overflow() {
        let fit = FitBox {
            width_units: 400.0,
            height_units: 10.0,
            unit: "mm",
        };
        let aligned =
            largest_fitting_font("Hxy", false, 400, VerticalAlign::Bottom, 6.0, 80.0, fit);
        let centered =
            largest_fitting_font("Hxy", false, 400, VerticalAlign::Center, 6.0, 80.0, fit);
        assert!(
            aligned < centered,
            "bottom-aligned must reserve the overflow and land smaller ({aligned} vs {centered})"
        );
        let face = instance(400, aligned).expect("face");
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
