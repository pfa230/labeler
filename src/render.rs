use crate::errors::AppError;
use crate::models::{Dimension, FontSize, LabelInput, Layout, LayoutItem, Point, TemplateFormat};
use crate::templates::TemplateDefinition;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use typst::layout::PagedDocument;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;
use typst_as_lib::TypstEngine;

pub fn render_single_label(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
) -> Result<Vec<u8>, AppError> {
    let TemplateFormat::Single { width, height } = &template.format else {
        return Err(AppError::unsupported_format(
            "render_label only supports single format",
        ));
    };

    let width_units = resolve_dimension(width)?;
    let height_units = resolve_dimension(height)?;
    let dpi = template.dpi;

    let selected_option = normalize_option(template, option)?;
    let items = select_layout_items(template)?;

    let source = build_typst_source(
        width_units,
        height_units,
        &template.unit,
        items,
        data,
        selected_option,
    )?;

    let engine = TypstEngine::builder()
        .main_file(source)
        .search_fonts_with(typst_font_options())
        .build();

    let warned = engine.compile::<PagedDocument>();
    let doc = warned
        .output
        .map_err(|err| AppError::render_failed(format!("typst compile failed: {err}")))?;

    let page = doc
        .pages
        .first()
        .ok_or_else(|| AppError::render_failed("typst did not produce any pages"))?;

    let pixmap = typst_render::render(page, dpi as f32 / 72.0);
    let png = pixmap
        .encode_png()
        .map_err(|err| AppError::render_failed(format!("failed to encode png: {err}")))?;

    Ok(png)
}

pub fn render_sheet_labels(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    start_slot: u32,
) -> Result<Vec<u8>, AppError> {
    let TemplateFormat::Sheet {
        paper_width,
        paper_height,
        label_width,
        label_height,
        positions,
    } = &template.format
    else {
        return Err(AppError::unsupported_format(
            "render_batch only supports sheet format",
        ));
    };

    let page_width_units = *paper_width;
    let page_height_units = *paper_height;

    let start_slot = start_slot as usize;
    if start_slot > positions.len() {
        return Err(AppError::invalid_request("start_slot is out of range"));
    }
    if start_slot + labels.len() > positions.len() {
        return Err(AppError::invalid_request("not enough sheet positions"));
    }

    let mut source = String::new();
    let page_width = format_length(page_width_units, &template.unit)?;
    let page_height = format_length(page_height_units, &template.unit)?;
    writeln!(
        source,
        "#set page(width: {page_width}, height: {page_height}, margin: 0{unit})",
        unit = template.unit
    )
    .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
    writeln!(source, "#set text(font: \"Inter\")")
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

    for (idx, label) in labels.iter().enumerate() {
        let position = &positions[start_slot + idx];
        let point = position.point();
        let left = point.x;
        let bottom = point.y;
        let width = *label_width;
        let height = *label_height;
        let top = bottom + height;

        let selected_option = normalize_option(template, label.option.as_ref())?;
        let items = select_layout_items(template)?;
        let content = render_items(
            items,
            width,
            height,
            &template.unit,
            &label.data,
            selected_option,
            0,
        )?;

        let dx = format_length(left, &template.unit)?;
        let dy = format_length(page_height_units - top, &template.unit)?;
        let box_width = format_length(width, &template.unit)?;
        let box_height = format_length(height, &template.unit)?;

        writeln!(
            source,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
    }

    let engine = TypstEngine::builder()
        .main_file(source)
        .search_fonts_with(typst_font_options())
        .build();

    let warned = engine.compile::<PagedDocument>();
    let doc = warned
        .output
        .map_err(|err| AppError::render_failed(format!("typst compile failed: {err}")))?;

    let pdf = typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| AppError::render_failed(format!("failed to encode pdf: {err:?}")))?;

    Ok(pdf)
}

fn build_typst_source(
    page_width_units: f32,
    page_height_units: f32,
    unit: &str,
    items: &[LayoutItem],
    data: &HashMap<String, JsonValue>,
    selected_option: Option<&BTreeMap<String, String>>,
) -> Result<String, AppError> {
    let mut source = String::new();
    let page_width = format_length(page_width_units, unit)?;
    let page_height = format_length(page_height_units, unit)?;
    writeln!(
        source,
        "#set page(width: {page_width}, height: {page_height}, margin: 0{unit})"
    )
    .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
    writeln!(source, "#set text(font: \"Inter\")")
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

    let items_source = render_items(
        items,
        page_width_units,
        page_height_units,
        unit,
        data,
        selected_option,
        0,
    )?;
    source.push_str(&items_source);

    Ok(source)
}

fn select_layout_items(template: &TemplateDefinition) -> Result<&[LayoutItem], AppError> {
    match &template.layout {
        Layout::Items(items) => Ok(items.as_slice()),
    }
}

fn normalize_option<'a>(
    template: &TemplateDefinition,
    option: Option<&'a BTreeMap<String, String>>,
) -> Result<Option<&'a BTreeMap<String, String>>, AppError> {
    match &template.options {
        Some(options) => {
            if let Some(selection) = option {
                if !options.is_valid_selection(selection) {
                    return Err(AppError::invalid_option_value(selection, options.allowed()));
                }
            }
            Ok(option)
        }
        None => {
            if option.is_some() {
                Err(AppError::invalid_request(
                    "template does not support options",
                ))
            } else {
                Ok(None)
            }
        }
    }
}

fn render_items(
    items: &[LayoutItem],
    frame_width_units: f32,
    frame_height_units: f32,
    unit: &str,
    data: &HashMap<String, JsonValue>,
    selected_option: Option<&BTreeMap<String, String>>,
    parent_rotation: u16,
) -> Result<String, AppError> {
    let mut out = String::new();

    for item in items {
        match item {
            LayoutItem::Text {
                name,
                bounds,
                font_size,
                multiline,
                alignment,
                ..
            } => {
                let size = match font_size {
                    FontSize::Fixed(size) => *size,
                    FontSize::Range { min: _, max } => *max,
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
                let dy = format_length(frame_height_units - top, unit)?;
                let box_width = format_length(width, unit)?;
                let box_height = format_length(box_height_units, unit)?;

                let align = typst_alignment(alignment);
                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[#align({align})[#text(\"{text}\", size: {size}pt)]]]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
            LayoutItem::Qr {
                name,
                bounds,
                params,
            } => {
                let payload = value_to_string(
                    data.get(name)
                        .ok_or_else(|| AppError::missing_field(name))?,
                );
                let (svg_xml, box_width, box_height, dx, dy) =
                    build_qr_svg(payload.as_bytes(), params, bounds, unit, frame_height_units)?;
                let svg_xml = escape_typst_string(&svg_xml);

                writeln!(
                    out,
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
                let (start_x, start_y) = to_page_coords(start, frame_height_units);
                let (end_x, end_y) = to_page_coords(end, frame_height_units);
                let dx = end_x - start_x;
                let dy = end_y - start_y;
                let start_x = format_length(start_x, unit)?;
                let start_y = format_length(start_y, unit)?;
                let dx = format_length(dx, unit)?;
                let dy = format_length(dy, unit)?;
                let zero = format_length(0.0, unit)?;
                let stroke = format_length(*thickness, unit)?;

                writeln!(
                    out,
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
                let dy = format_length(frame_height_units - top, unit)?;
                let box_width = format_length(width, unit)?;
                let box_height = format_length(height, unit)?;
                let stroke = format_length(*thickness, unit)?;
                let radius = if *rounded {
                    format_length(thickness * 2.0, unit)?
                } else {
                    format_length(0.0, unit)?
                };

                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#rect(width: {box_width}, height: {box_height}, stroke: {stroke}, radius: {radius})]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
            LayoutItem::Container {
                bounds,
                option,
                rotation,
                items,
            } => {
                if let Some(option) = option {
                    if let Some(selected_option) = selected_option {
                        let matches = option
                            .iter()
                            .all(|(name, value)| selected_option.get(name) == Some(value));
                        if !matches {
                            continue;
                        }
                    }
                }
                let (left, bottom, right, top) = if let Some(bounds) = bounds {
                    let (x1, y1, x2, y2) = (bounds.0[0], bounds.0[1], bounds.0[2], bounds.0[3]);
                    (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2))
                } else {
                    (0.0, 0.0, frame_width_units, frame_height_units)
                };
                let width = right - left;
                let height = top - bottom;

                let effective_rotation = rotation.unwrap_or(parent_rotation);
                let delta_rotation =
                    ((effective_rotation as i32 - parent_rotation as i32).rem_euclid(360)) as u16;

                let child_source = render_items(
                    items,
                    width,
                    height,
                    unit,
                    data,
                    selected_option,
                    effective_rotation,
                )?;
                let mut content = child_source;
                if delta_rotation != 0 {
                    content = format!("#rotate({delta_rotation}deg)[{content}]");
                }

                let dx = format_length(left, unit)?;
                let dy = format_length(frame_height_units - top, unit)?;
                let box_width = format_length(width, unit)?;
                let box_height = format_length(height, unit)?;

                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
        }
    }

    Ok(out)
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

fn to_page_coords(point: &Point, page_height_units: f32) -> (f32, f32) {
    (point.x, page_height_units - point.y)
}

fn typst_font_options() -> TypstKitFontOptions {
    TypstKitFontOptions::default().include_dirs(["fonts"])
}

fn typst_alignment(alignment: &crate::models::Alignment) -> String {
    use crate::models::{HorizontalAlign, VerticalAlign};
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

#[cfg(test)]
mod tests {
    use super::{render_sheet_labels, render_single_label};
    use crate::models::{
        Alignment, Box, Dimension, FontSize, LabelInput, Layout, LayoutItem, Options, Point,
        SheetPosition, TemplateFormat,
    };
    use crate::templates::TemplateDefinition;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

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
            options: Some(Options(BTreeMap::from([(
                "variant".to_string(),
                vec!["default".to_string()],
            )]))),
            layout: Layout::Items(vec![LayoutItem::Text {
                name: "message".to_string(),
                bounds: Box([0.0, 0.0, 20.0, 5.0]),
                font_size: FontSize::Fixed(10.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };

        let data = HashMap::from([("message".to_string(), json!("Hello"))]);
        let selection = BTreeMap::from([("variant".to_string(), "default".to_string())]);
        let png = render_single_label(&template, &data, Some(&selection)).expect("render label");

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
            options: Some(Options(BTreeMap::from([(
                "variant".to_string(),
                vec!["default".to_string()],
            )]))),
            layout: Layout::Items(vec![
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
            ]),
            version: None,
        };

        let data = HashMap::from([
            ("message".to_string(), json!("Hello")),
            ("code".to_string(), json!("QR-123")),
        ]);
        let selection = BTreeMap::from([("variant".to_string(), "default".to_string())]);
        let png =
            render_single_label(&template, &data, Some(&selection)).expect("render label with qr");

        assert!(!png.is_empty(), "rendered PNG is empty");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_sheet_labels_produces_pdf() {
        let template = TemplateDefinition {
            id: "sheet".to_string(),
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
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: "message".to_string(),
                bounds: Box([0.0, 0.0, 10.0, 5.0]),
                font_size: FontSize::Fixed(10.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };

        let labels = vec![LabelInput {
            data: HashMap::from([("message".to_string(), json!("Hello"))]),
            option: None,
        }];

        let pdf = render_sheet_labels(&template, &labels, 0).expect("render sheet");

        assert!(!pdf.is_empty(), "rendered PDF is empty");
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
    }
}
