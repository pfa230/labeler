use crate::errors::AppError;
use crate::models::{Alignment, Dimension, HorizontalAlign, Point, QrParams, VerticalAlign};
use fontdue::Font;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde_json::Value as JsonValue;
use std::sync::OnceLock;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;

pub(super) fn value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => value.clone(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
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

pub(super) fn build_qr_svg(
    payload: &[u8],
    params: &Option<QrParams>,
    bounds: &crate::models::Box,
    unit: &str,
    page_height_units: f32,
) -> Result<(String, String, String, String, String), AppError> {
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
                "qr error_correction must be one of L, M, Q, H",
            )),
        })
        .transpose()?
        .unwrap_or(EcLevel::M);

    let code = QrCode::with_error_correction_level(payload, ecc)
        .map_err(|err| AppError::render_failed(format!("qr generation failed: {err}")))?;

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

    let svg_xml = renderer.build();

    let (x1, y1, x2, y2) = (bounds.0[0], bounds.0[1], bounds.0[2], bounds.0[3]);
    let left = x1.min(x2);
    let right = x1.max(x2);
    let bottom = y1.min(y2);
    let top = y1.max(y2);
    let width = right - left;
    let height = top - bottom;

    let dx = format_length(left, unit)?;
    let dy = format_length(page_height_units - top, unit)?;
    let box_width = format_length(width, unit)?;
    let box_height = format_length(height, unit)?;

    Ok((svg_xml, box_width, box_height, dx, dy))
}

pub(super) fn to_page_coords(point: &Point, page_height_units: f32) -> (f32, f32) {
    (point.x, page_height_units - point.y)
}

pub(super) fn typst_font_options() -> TypstKitFontOptions {
    TypstKitFontOptions::default().include_dirs(["fonts"])
}

fn inter_font() -> Result<&'static Font, AppError> {
    static FONT: OnceLock<Font> = OnceLock::new();
    if let Some(font) = FONT.get() {
        return Ok(font);
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fonts")
        .join("InterVariable.ttf");
    let bytes = std::fs::read(&path)
        .map_err(|err| AppError::render_failed(format!("failed to read font: {err}")))?;
    let font = Font::from_bytes(bytes, fontdue::FontSettings::default())
        .map_err(|err| AppError::render_failed(format!("failed to parse font: {err}")))?;
    FONT.set(font)
        .map_err(|_| AppError::render_failed("failed to cache font"))?;
    Ok(FONT.get().expect("font initialized"))
}

pub(super) fn fit_text_to_box(
    text: &str,
    multiline: bool,
    min_size: f32,
    max_size: f32,
    width_units: f32,
    height_units: f32,
    unit: &str,
) -> Result<(f32, String), AppError> {
    let font = inter_font()?;
    let width_pt = units_to_pt(width_units, unit);
    let height_pt = units_to_pt(height_units, unit);
    let step = 0.5f32;
    let mut size = max_size;
    while size >= min_size - f32::EPSILON {
        if text_fits(font, text, multiline, size, width_pt, height_pt) {
            if multiline {
                let lines = wrap_text(font, text, size, width_pt);
                return Ok((size, lines.join("\n")));
            }
            return Ok((size, text.to_string()));
        }
        size -= step;
    }
    let size = min_size;
    let trimmed = if multiline {
        trim_multiline(font, text, size, width_pt, height_pt)
    } else {
        trim_single_line(font, text, size, width_pt)
    };
    Ok((size, trimmed))
}

fn text_fits(
    font: &Font,
    text: &str,
    multiline: bool,
    size: f32,
    width_pt: f32,
    height_pt: f32,
) -> bool {
    if multiline {
        let lines = wrap_text(font, text, size, width_pt);
        let line_height = line_height(font, size);
        line_height * lines.len() as f32 <= height_pt + 0.01
    } else {
        let width = text_width(font, text, size);
        let line_height = line_height(font, size);
        width <= width_pt + 0.01 && line_height <= height_pt + 0.01
    }
}

fn trim_single_line(font: &Font, text: &str, size: f32, width_pt: f32) -> String {
    const ELLIPSIS: &str = "...";
    let mut out = text.to_string();
    if text_width(font, &out, size) <= width_pt {
        return out;
    }
    let ellipsis_width = text_width(font, ELLIPSIS, size);
    if ellipsis_width > width_pt {
        return ELLIPSIS.to_string();
    }
    while !out.is_empty() && text_width(font, &format!("{out}{ELLIPSIS}"), size) > width_pt {
        out.pop();
    }
    format!("{out}{ELLIPSIS}")
}

fn trim_multiline(font: &Font, text: &str, size: f32, width_pt: f32, height_pt: f32) -> String {
    const ELLIPSIS: &str = "...";
    let line_height = line_height(font, size);
    let max_lines = (height_pt / line_height).floor().max(1.0) as usize;
    let mut lines = wrap_text(font, text, size, width_pt);
    if lines.len() <= max_lines {
        return lines.join("\n");
    }
    lines.truncate(max_lines);
    let last = lines.last_mut().unwrap();
    let ellipsis_width = text_width(font, ELLIPSIS, size);
    if ellipsis_width > width_pt {
        *last = ELLIPSIS.to_string();
    } else {
        while !last.is_empty() && text_width(font, &format!("{last}{ELLIPSIS}"), size) > width_pt {
            last.pop();
        }
        *last = format!("{last}{ELLIPSIS}");
    }
    lines.join("\n")
}

fn wrap_text(font: &Font, text: &str, size: f32, width_pt: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let space_width = text_width(font, " ", size);
    let paragraphs: Vec<&str> = text.split('\n').collect();
    for paragraph in paragraphs {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0.0;
        for word in paragraph.split_whitespace() {
            let word_width = text_width(font, word, size);
            if current.is_empty() {
                if word_width <= width_pt {
                    current.push_str(word);
                    current_width = word_width;
                } else {
                    let mut chunk = String::new();
                    let mut chunk_width = 0.0;
                    for ch in word.chars() {
                        let ch_width = text_width(font, &ch.to_string(), size);
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
                        let ch_width = text_width(font, &ch.to_string(), size);
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

fn text_width(font: &Font, text: &str, size: f32) -> f32 {
    text.chars()
        .map(|ch| font.metrics(ch, size).advance_width)
        .sum()
}

fn line_height(font: &Font, size: f32) -> f32 {
    font.horizontal_line_metrics(size)
        .map(|metrics| metrics.new_line_size)
        .unwrap_or(size * 1.2)
}

fn units_to_pt(value: f32, unit: &str) -> f32 {
    match unit {
        "in" => value * 72.0,
        "mm" => value * 72.0 / 25.4,
        _ => value,
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
