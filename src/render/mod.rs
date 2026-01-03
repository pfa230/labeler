mod helpers;

use crate::errors::AppError;
use crate::models::{
    FontSize, LabelInput, Layout, LayoutItem, Point, Position, Size, SizeValue, TemplateFormat,
};
use crate::templates::TemplateDefinition;
use helpers::{
    build_qr_svg, escape_typst_string, fit_text_to_box, format_length, resolve_dimension,
    to_nonbreaking, to_page_coords, typst_alignment, typst_font_options, value_to_string,
};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use typst::layout::PagedDocument;
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
    tracing::debug!(template = %template.id, typst = %source, "render typst source");

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
    writeln!(source, "#set text(font: (\"Inter Variable\", \"Inter\"))")
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
        let context =
            RenderContext::new(width, height, &template.unit, &label.data, selected_option);
        let content = context.render_items(items)?;

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
    tracing::debug!(template = %template.id, typst = %source, "render typst source");

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
    writeln!(source, "#set text(font: (\"Inter Variable\", \"Inter\"))")
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

    let context = RenderContext::new(
        page_width_units,
        page_height_units,
        unit,
        data,
        selected_option,
    );
    let items_source = context.render_items(items)?;
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

struct RenderContext<'a> {
    frame_width_units: f32,
    frame_height_units: f32,
    unit: &'a str,
    data: &'a HashMap<String, JsonValue>,
    selected_option: Option<&'a BTreeMap<String, String>>,
}

#[derive(Clone, Copy)]
struct ItemPlacement<'a> {
    at: &'a Position,
    size: &'a Size,
    max_w: Option<f32>,
    max_h: Option<f32>,
    rotate: Option<f32>,
}

impl<'a> RenderContext<'a> {
    fn new(
        frame_width_units: f32,
        frame_height_units: f32,
        unit: &'a str,
        data: &'a HashMap<String, JsonValue>,
        selected_option: Option<&'a BTreeMap<String, String>>,
    ) -> Self {
        Self {
            frame_width_units,
            frame_height_units,
            unit,
            data,
            selected_option,
        }
    }

    fn render_items(&self, items: &[LayoutItem]) -> Result<String, AppError> {
        let mut out = String::new();

        for item in items {
            match item {
                LayoutItem::Text {
                    name,
                    at,
                    size,
                    max_w,
                    max_h,
                    rotate,
                    font_size,
                    multiline,
                    alignment,
                } => {
                    let placement = ItemPlacement {
                        at,
                        size,
                        max_w: *max_w,
                        max_h: *max_h,
                        rotate: *rotate,
                    };
                    self.render_text_item(
                        &mut out, name, placement, font_size, *multiline, alignment,
                    )?;
                }
                LayoutItem::Qr {
                    name,
                    at,
                    size,
                    max_w,
                    max_h,
                    rotate,
                    params,
                } => {
                    let placement = ItemPlacement {
                        at,
                        size,
                        max_w: *max_w,
                        max_h: *max_h,
                        rotate: *rotate,
                    };
                    self.render_qr_item(&mut out, name, placement, params)?;
                }
                LayoutItem::Line {
                    at,
                    size,
                    max_w,
                    max_h,
                    rotate,
                    thickness,
                } => {
                    let placement = ItemPlacement {
                        at,
                        size,
                        max_w: *max_w,
                        max_h: *max_h,
                        rotate: *rotate,
                    };
                    self.render_line_item(&mut out, placement, *thickness)?;
                }
                LayoutItem::Rectangle {
                    at,
                    size,
                    max_w,
                    max_h,
                    rotate,
                    thickness,
                    rounded,
                } => {
                    let placement = ItemPlacement {
                        at,
                        size,
                        max_w: *max_w,
                        max_h: *max_h,
                        rotate: *rotate,
                    };
                    self.render_rectangle_item(&mut out, placement, *thickness, *rounded)?;
                }
                LayoutItem::Container {
                    at,
                    size,
                    max_w,
                    max_h,
                    rotate,
                    option,
                    items,
                } => {
                    let placement = ItemPlacement {
                        at,
                        size,
                        max_w: *max_w,
                        max_h: *max_h,
                        rotate: *rotate,
                    };
                    self.render_container_item(&mut out, placement, option, items)?;
                }
            }
        }

        Ok(out)
    }

    fn render_text_item(
        &self,
        out: &mut String,
        name: &str,
        placement: ItemPlacement<'_>,
        font_size: &FontSize,
        multiline: bool,
        alignment: &crate::models::Alignment,
    ) -> Result<(), AppError> {
        let raw_text = value_to_string(
            self.data
                .get(name)
                .ok_or_else(|| AppError::missing_field(name))?,
        );
        let text = if multiline {
            raw_text
        } else {
            raw_text.lines().next().unwrap_or("").to_string()
        };
        let text = if multiline {
            text
        } else {
            to_nonbreaking(&text)
        };

        let (width, box_height_units) =
            self.resolve_size(placement.size, placement.max_w, placement.max_h, false)?;
        let point = placement.at.point();
        let left = point.x;
        let bottom = point.y;
        let top = bottom + box_height_units;
        let (size, text) = match font_size {
            FontSize::Fixed(size) => (*size, text),
            FontSize::Range { min, max } => fit_text_to_box(
                &text,
                multiline,
                *min,
                *max,
                width,
                box_height_units,
                self.unit,
            )?,
        };
        let text = escape_typst_string(&text);
        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(box_height_units, self.unit)?;

        let align = typst_alignment(alignment);
        let content = format!("#align({align})[#text(\"{text}\", size: {size}pt)]");
        let content = self.wrap_rotation(content, placement.rotate);
        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_qr_item(
        &self,
        out: &mut String,
        name: &str,
        placement: ItemPlacement<'_>,
        params: &Option<crate::models::QrParams>,
    ) -> Result<(), AppError> {
        let payload = value_to_string(
            self.data
                .get(name)
                .ok_or_else(|| AppError::missing_field(name))?,
        );
        let (width, height) =
            self.resolve_size(placement.size, placement.max_w, placement.max_h, false)?;
        let point = placement.at.point();
        let left = point.x;
        let bottom = point.y;
        let top = bottom + height;
        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(height, self.unit)?;
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
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_line_item(
        &self,
        out: &mut String,
        placement: ItemPlacement<'_>,
        thickness: f32,
    ) -> Result<(), AppError> {
        let (dx_units, dy_units) =
            self.resolve_line_delta(placement.size, placement.max_w, placement.max_h)?;
        let start_point = placement.at.point();
        let end_point = Point {
            x: start_point.x + dx_units,
            y: start_point.y + dy_units,
        };
        let (start_x, start_y) = to_page_coords(&start_point, self.frame_height_units);
        let (end_x, end_y) = to_page_coords(&end_point, self.frame_height_units);
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let start_x = format_length(start_x, self.unit)?;
        let start_y = format_length(start_y, self.unit)?;
        let dx = format_length(dx, self.unit)?;
        let dy = format_length(dy, self.unit)?;
        let zero = format_length(0.0, self.unit)?;
        let stroke = format_length(thickness, self.unit)?;

        let content =
            format!("#line(start: ({zero}, {zero}), end: ({dx}, {dy}), stroke: {stroke})");
        let content = self.wrap_rotation(content, placement.rotate);
        writeln!(
            out,
            "#place(top + left, dx: {start_x}, dy: {start_y})[{content}]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_rectangle_item(
        &self,
        out: &mut String,
        placement: ItemPlacement<'_>,
        thickness: f32,
        rounded: bool,
    ) -> Result<(), AppError> {
        let (width, height) =
            self.resolve_size(placement.size, placement.max_w, placement.max_h, false)?;
        let point = placement.at.point();
        let left = point.x;
        let bottom = point.y;
        let top = bottom + height;
        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(height, self.unit)?;
        let stroke = format_length(thickness, self.unit)?;
        let radius = if rounded {
            format_length(thickness * 2.0, self.unit)?
        } else {
            format_length(0.0, self.unit)?
        };

        let content = format!(
            "#rect(width: {box_width}, height: {box_height}, stroke: {stroke}, radius: {radius})"
        );
        let content = self.wrap_rotation(content, placement.rotate);
        writeln!(out, "#place(top + left, dx: {dx}, dy: {dy})[{content}]").map_err(|err| {
            AppError::render_failed(format!("failed to build typst source: {err}"))
        })?;

        Ok(())
    }

    fn render_container_item(
        &self,
        out: &mut String,
        placement: ItemPlacement<'_>,
        option: &Option<BTreeMap<String, String>>,
        items: &[LayoutItem],
    ) -> Result<(), AppError> {
        if let Some(option) = option {
            if let Some(selected_option) = self.selected_option {
                let matches = option
                    .iter()
                    .all(|(name, value)| selected_option.get(name) == Some(value));
                if !matches {
                    return Ok(());
                }
            }
        }
        let (width, height) =
            self.resolve_size(placement.size, placement.max_w, placement.max_h, true)?;
        let point = placement.at.point();
        let left = point.x;
        let bottom = point.y;
        let top = bottom + height;

        let context = RenderContext::new(width, height, self.unit, self.data, self.selected_option);
        let child_source = context.render_items(items)?;
        let content = self.wrap_rotation(child_source, placement.rotate);

        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(height, self.unit)?;

        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn resolve_size(
        &self,
        size: &Size,
        max_w: Option<f32>,
        max_h: Option<f32>,
        allow_auto_fill: bool,
    ) -> Result<(f32, f32), AppError> {
        let fallback = if allow_auto_fill {
            Some((self.frame_width_units, self.frame_height_units))
        } else {
            None
        };
        let width =
            self.resolve_size_value(&size.0[0], max_w, fallback.map(|value| value.0), "width")?;
        let height =
            self.resolve_size_value(&size.0[1], max_h, fallback.map(|value| value.1), "height")?;
        Ok((width, height))
    }

    fn resolve_size_value(
        &self,
        value: &SizeValue,
        max: Option<f32>,
        fallback: Option<f32>,
        label: &str,
    ) -> Result<f32, AppError> {
        match value {
            SizeValue::Value(value) => {
                if *value <= 0.0 {
                    return Err(AppError::unsupported_layout_item(format!(
                        "size {label} must be greater than 0"
                    )));
                }
                Ok(*value)
            }
            SizeValue::Auto(_) => {
                let resolved = max.or(fallback).ok_or_else(|| {
                    AppError::unsupported_layout_item(format!(
                        "size {label} is auto but no max_{label} provided"
                    ))
                })?;
                if resolved <= 0.0 {
                    return Err(AppError::unsupported_layout_item(format!(
                        "max_{label} must be greater than 0"
                    )));
                }
                Ok(resolved)
            }
        }
    }

    fn resolve_line_delta(
        &self,
        size: &Size,
        max_w: Option<f32>,
        max_h: Option<f32>,
    ) -> Result<(f32, f32), AppError> {
        let fallback = Some((self.frame_width_units, self.frame_height_units));
        let dx =
            self.resolve_line_value(&size.0[0], max_w, fallback.map(|value| value.0), "width")?;
        let dy =
            self.resolve_line_value(&size.0[1], max_h, fallback.map(|value| value.1), "height")?;
        Ok((dx, dy))
    }

    fn resolve_line_value(
        &self,
        value: &SizeValue,
        max: Option<f32>,
        fallback: Option<f32>,
        label: &str,
    ) -> Result<f32, AppError> {
        match value {
            SizeValue::Value(value) => Ok(*value),
            SizeValue::Auto(_) => {
                let resolved = max.or(fallback).ok_or_else(|| {
                    AppError::unsupported_layout_item(format!(
                        "size {label} is auto but no max_{label} provided"
                    ))
                })?;
                if resolved <= 0.0 {
                    return Err(AppError::unsupported_layout_item(format!(
                        "max_{label} must be greater than 0"
                    )));
                }
                Ok(resolved)
            }
        }
    }

    fn wrap_rotation(&self, content: String, rotate: Option<f32>) -> String {
        if let Some(rotate) = rotate {
            format!("#rotate({rotate}deg)[{content}]")
        } else {
            content
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{render_sheet_labels, render_single_label};
    use crate::models::{
        Alignment, Dimension, FontSize, LabelInput, Layout, LayoutItem, Options, Position,
        SheetPosition, Size, SizeValue, TemplateFormat,
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
                at: Position([0.0, 0.0]),
                size: Size([SizeValue::Value(20.0), SizeValue::Value(5.0)]),
                max_w: None,
                max_h: None,
                rotate: None,
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
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                    font_size: FontSize::Fixed(10.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: "code".to_string(),
                    at: Position([20.0, 0.0]),
                    size: Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                    params: None,
                },
                LayoutItem::Line {
                    at: Position([0.0, 1.0]),
                    size: Size([SizeValue::Value(30.0), SizeValue::Value(0.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                    thickness: 0.2,
                },
                LayoutItem::Rectangle {
                    at: Position([0.5, 1.5]),
                    size: Size([SizeValue::Value(29.0), SizeValue::Value(18.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
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
                at: Position([0.0, 0.0]),
                size: Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                max_w: None,
                max_h: None,
                rotate: None,
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
