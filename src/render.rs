use crate::errors::AppError;
use crate::models::{Dimension, FontSize, Layout, LayoutItem, TemplateFormat};
use crate::templates::TemplateDefinition;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fmt::Write;
use typst::layout::PagedDocument;
use typst_as_lib::TypstEngine;

pub fn render_single_label(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: &str,
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
        Layout::OptionsLayout(map) => map
            .get(option)
            .ok_or_else(|| AppError::invalid_option_value(option, &template.options.0))?,
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
                    "#place(top + left, dx: {dx}, dy: {dy})[#block(width: {box_width}, height: {box_height})[#text(\"{text}\", size: {size}pt)]]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
            }
            _ => {
                return Err(AppError::unsupported_layout_item(
                    "only text items are supported for now",
                ));
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

#[cfg(test)]
mod tests {
    use super::render_single_label;
    use crate::models::{
        Alignment, Box, Dimension, FontSize, Layout, LayoutItem, Options, TemplateFormat,
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
            options: Options(vec!["default".to_string()]),
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
        let png = render_single_label(&template, &data, "default", None)
            .expect("render label");

        assert!(!png.is_empty(), "rendered PNG is empty");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
