use crate::errors::AppError;
use crate::models::{Dimension, FontSize, Layout, LayoutItem, Point, TemplateFormat};
use crate::templates::TemplateDefinition;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fmt::Write;
use typst::layout::PagedDocument;
use typst_as_lib::TypstEngine;

pub fn render_single_label(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&str>,
    dpi_override: Option<u32>,
) -> Result<Vec<u8>, AppError> {
    let TemplateFormat::Single { width, height } = &template.format else {
        return Err(AppError::unsupported_format("render_label only supports single format"));
    };

    let width_units = resolve_dimension(width)?;
    let height_units = resolve_dimension(height)?;
    let dpi = dpi_override.unwrap_or(template.dpi);

    let items = match &template.layout {
        Layout::Items(items) => items.as_slice(),
        Layout::OptionsLayout(map) => {
            let option = option.ok_or_else(|| {
                AppError::invalid_request("missing option for optioned template")
            })?;
            let allowed = template
                .options
                .as_ref()
                .map(|opts| opts.0.as_slice())
                .unwrap_or(&[]);
            map.get(option)
                .ok_or_else(|| AppError::invalid_option_value(option, allowed))?
        }
    };

    let source = build_typst_source(
        width_units,
        height_units,
        &template.unit,
        items,
        data,
    )?;

    let engine = TypstEngine::builder()
        .main_file(source)
        .search_fonts_with(Default::default())
        .build();

    let warned = engine.compile::<PagedDocument>();
    let doc = warned
        .output
        .map_err(|err| AppError::render_failed(format!("typst compile failed: {err}")))?;

    let page = doc.pages.first().ok_or_else(|| {
        AppError::render_failed("typst did not produce any pages")
    })?;

    let pixmap = typst_render::render(page, dpi as f32 / 72.0);
    let png = pixmap
        .encode_png()
        .map_err(|err| AppError::render_failed(format!("failed to encode png: {err}")))?;

    Ok(png)
}

fn build_typst_source(
    page_width_units: f32,
    page_height_units: f32,
    unit: &str,
    items: &[LayoutItem],
    data: &HashMap<String, JsonValue>,
) -> Result<String, AppError> {
    let mut source = String::new();
    let page_width = format_length(page_width_units, unit)?;
    let page_height = format_length(page_height_units, unit)?;
    writeln!(
        source,
        "#set page(width: {page_width}, height: {page_height}, margin: 0{unit})"
    )
    .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

    for item in items {
        match item {
            LayoutItem::Text {
                name,
                bounds,
                font_size,
                multiline,
                ..
            } => {
                let FontSize::Fixed(size) = font_size else {
                    return Err(AppError::unsupported_layout_item(
                        "text font_size must be fixed",
                    ));
                };
                let raw_text = value_to_string(
                    data.get(name)
                        .ok_or_else(|| AppError::missing_field(name))?,
                );
                let text = if *multiline {
                    raw_text
                } else {
                    raw_text.lines().next().unwrap_or("").to_string()
                };
                let text = if *multiline {
                    text
                } else {
                    to_nonbreaking(&text)
                };
                let text = escape_typst_string(&text);

                let (x1, y1, x2, y2) = (bounds.0[0], bounds.0[1], bounds.0[2], bounds.0[3]);
                let left = x1.min(x2);
                let right = x1.max(x2);
                let bottom = y1.min(y2);
                let top = y1.max(y2);
                let width = right - left;
                let box_height_units = top - bottom;
                let dx = format_length(left, unit)?;
                let dy = format_length(page_height_units - top, unit)?;
                let box_width = format_length(width, unit)?;
                let box_height = format_length(box_height_units, unit)?;

                writeln!(
                    source,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[#text(\"{text}\", size: {size}pt)]]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
            LayoutItem::Qr { name, bounds, params } => {
                let payload = value_to_string(
                    data.get(name)
                        .ok_or_else(|| AppError::missing_field(name))?,
                );
                let (svg_xml, box_width, box_height, dx, dy) =
                    build_qr_svg(payload.as_bytes(), params, bounds, unit, page_height_units)?;
                let svg_xml = escape_typst_string(&svg_xml);

                writeln!(
                    source,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[#image(bytes(\"{svg_xml}\"), format: \"svg\", width: {box_width}, height: {box_height}, fit: \"contain\")]]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
            LayoutItem::Line {
                start,
                end,
                thickness,
            } => {
                let (start_x, start_y) = to_page_coords(start, page_height_units);
                let (end_x, end_y) = to_page_coords(end, page_height_units);
                let dx = end_x - start_x;
                let dy = end_y - start_y;
                let start_x = format_length(start_x, unit)?;
                let start_y = format_length(start_y, unit)?;
                let dx = format_length(dx, unit)?;
                let dy = format_length(dy, unit)?;
                let zero = format_length(0.0, unit)?;
                let stroke = format_length(*thickness, unit)?;

                writeln!(
                    source,
                    "#place(top + left, dx: {start_x}, dy: {start_y})[#line(start: ({zero}, {zero}), end: ({dx}, {dy}), stroke: {stroke})]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
            LayoutItem::Rectangle {
                bounds,
                thickness,
                rounded,
            } => {
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
                let stroke = format_length(*thickness, unit)?;
                let radius = if *rounded {
                    format_length(thickness * 2.0, unit)?
                } else {
                    format_length(0.0, unit)?
                };

                writeln!(
                    source,
                    "#place(top + left, dx: {dx}, dy: {dy})[#rect(width: {box_width}, height: {box_height}, stroke: {stroke}, radius: {radius})]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
        }
    }

    Ok(source)
}

fn value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => value.clone(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

fn resolve_dimension(dimension: &Dimension) -> Result<f32, AppError> {
    match dimension {
        Dimension::Fixed(value) => Ok(*value),
        Dimension::Dynamic { min, max } => max
            .or(*min)
            .ok_or_else(|| AppError::unsupported_format("dynamic dimension missing min/max")),
    }
}

fn format_length(value: f32, unit: &str) -> Result<String, AppError> {
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

fn escape_typst_string(value: &str) -> String {
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

fn to_nonbreaking(value: &str) -> String {
    value.replace(' ', "\u{00A0}")
}

fn build_qr_svg(
    payload: &[u8],
    params: &Option<crate::models::QrParams>,
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

fn to_page_coords(point: &Point, page_height_units: f32) -> (f32, f32) {
    (point.x, page_height_units - point.y)
}

#[cfg(test)]
mod tests {
    use super::render_single_label;
    use crate::models::{
        Alignment, Box, Dimension, FontSize, Layout, LayoutItem, Options, Point, TemplateFormat,
    };
    use crate::templates::TemplateDefinition;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn render_single_label_produces_png() {
        let template = TemplateDefinition {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test template".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0),
                height: Dimension::Fixed(10.0),
            },
            options: Some(Options(vec!["default".to_string()])),
            layout: Layout::OptionsLayout(HashMap::from([(
                "default".to_string(),
                vec![LayoutItem::Text {
                    name: "message".to_string(),
                    bounds: Box([0.0, 0.0, 20.0, 5.0]),
                    font_size: FontSize::Fixed(10.0),
                    multiline: false,
                    alignment: Alignment::default(),
                }],
            )])),
            version: None,
        };

        let data = HashMap::from([("message".to_string(), json!("Hello"))]);
        let png = render_single_label(&template, &data, Some("default"), None)
            .expect("render label");

        assert!(!png.is_empty(), "rendered PNG is empty");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_single_label_with_qr_produces_png() {
        let template = TemplateDefinition {
            id: "test_qr".to_string(),
            name: "Test QR".to_string(),
            description: "Test template with qr".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(30.0),
                height: Dimension::Fixed(20.0),
            },
            options: Some(Options(vec!["default".to_string()])),
            layout: Layout::OptionsLayout(HashMap::from([(
                "default".to_string(),
                vec![
                    LayoutItem::Text {
                        name: "message".to_string(),
                        bounds: Box([0.0, 0.0, 20.0, 20.0]),
                        font_size: FontSize::Fixed(10.0),
                        multiline: false,
                        alignment: Alignment::default(),
                    },
                    LayoutItem::Qr {
                        name: "code".to_string(),
                        bounds: Box([20.0, 0.0, 30.0, 10.0]),
                        params: None,
                    },
                    LayoutItem::Line {
                        start: Point { x: 0.0, y: 1.0 },
                        end: Point { x: 30.0, y: 1.0 },
                        thickness: 0.2,
                    },
                    LayoutItem::Rectangle {
                        bounds: Box([0.5, 1.5, 29.5, 19.5]),
                        thickness: 0.2,
                        rounded: true,
                    },
                ],
            )])),
            version: None,
        };

        let data = HashMap::from([
            ("message".to_string(), json!("Hello")),
            ("code".to_string(), json!("QR-123")),
        ]);
        let png = render_single_label(&template, &data, Some("default"), None)
            .expect("render label with qr");

        assert!(!png.is_empty(), "rendered PNG is empty");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
