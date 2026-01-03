mod helpers;

use crate::errors::AppError;
use crate::models::{FontSize, LabelInput, Layout, LayoutItem, Point, TemplateFormat};
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
        let context = RenderContext::new(
            width,
            height,
            &template.unit,
            &label.data,
            selected_option,
            0,
        );
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
        0,
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
    parent_rotation: u16,
}

impl<'a> RenderContext<'a> {
    fn new(
        frame_width_units: f32,
        frame_height_units: f32,
        unit: &'a str,
        data: &'a HashMap<String, JsonValue>,
        selected_option: Option<&'a BTreeMap<String, String>>,
        parent_rotation: u16,
    ) -> Self {
        Self {
            frame_width_units,
            frame_height_units,
            unit,
            data,
            selected_option,
            parent_rotation,
        }
    }

    fn render_items(&self, items: &[LayoutItem]) -> Result<String, AppError> {
        let mut out = String::new();

        for item in items {
            match item {
                LayoutItem::Text {
                    name,
                    bounds,
                    font_size,
                    multiline,
                    alignment,
                } => {
                    self.render_text_item(
                        &mut out, name, bounds, font_size, *multiline, alignment,
                    )?;
                }
                LayoutItem::Qr {
                    name,
                    bounds,
                    params,
                } => {
                    self.render_qr_item(&mut out, name, bounds, params)?;
                }
                LayoutItem::Line {
                    start,
                    end,
                    thickness,
                } => {
                    self.render_line_item(&mut out, start, end, *thickness)?;
                }
                LayoutItem::Rectangle {
                    bounds,
                    thickness,
                    rounded,
                } => {
                    self.render_rectangle_item(&mut out, bounds, *thickness, *rounded)?;
                }
                LayoutItem::Container {
                    bounds,
                    option,
                    rotation,
                    items,
                } => {
                    self.render_container_item(&mut out, bounds, option, rotation, items)?;
                }
            }
        }

        Ok(out)
    }

    fn render_text_item(
        &self,
        out: &mut String,
        name: &str,
        bounds: &crate::models::Box,
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

        let (x1, y1, x2, y2) = (bounds.0[0], bounds.0[1], bounds.0[2], bounds.0[3]);
        let left = x1.min(x2);
        let right = x1.max(x2);
        let bottom = y1.min(y2);
        let top = y1.max(y2);
        let width = right - left;
        let box_height_units = top - bottom;
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
        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[#align({align})[#text(\"{text}\", size: {size}pt)]]]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_qr_item(
        &self,
        out: &mut String,
        name: &str,
        bounds: &crate::models::Box,
        params: &Option<crate::models::QrParams>,
    ) -> Result<(), AppError> {
        let payload = value_to_string(
            self.data
                .get(name)
                .ok_or_else(|| AppError::missing_field(name))?,
        );
        let (svg_xml, box_width, box_height, dx, dy) = build_qr_svg(
            payload.as_bytes(),
            params,
            bounds,
            self.unit,
            self.frame_height_units,
        )?;
        let svg_xml = escape_typst_string(&svg_xml);

        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[#image(bytes(\"{svg_xml}\"), format: \"svg\", width: {box_width}, height: {box_height}, fit: \"contain\")]]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_line_item(
        &self,
        out: &mut String,
        start: &Point,
        end: &Point,
        thickness: f32,
    ) -> Result<(), AppError> {
        let (start_x, start_y) = to_page_coords(start, self.frame_height_units);
        let (end_x, end_y) = to_page_coords(end, self.frame_height_units);
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let start_x = format_length(start_x, self.unit)?;
        let start_y = format_length(start_y, self.unit)?;
        let dx = format_length(dx, self.unit)?;
        let dy = format_length(dy, self.unit)?;
        let zero = format_length(0.0, self.unit)?;
        let stroke = format_length(thickness, self.unit)?;

        writeln!(
            out,
            "#place(top + left, dx: {start_x}, dy: {start_y})[#line(start: ({zero}, {zero}), end: ({dx}, {dy}), stroke: {stroke})]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_rectangle_item(
        &self,
        out: &mut String,
        bounds: &crate::models::Box,
        thickness: f32,
        rounded: bool,
    ) -> Result<(), AppError> {
        let (x1, y1, x2, y2) = (bounds.0[0], bounds.0[1], bounds.0[2], bounds.0[3]);
        let left = x1.min(x2);
        let right = x1.max(x2);
        let bottom = y1.min(y2);
        let top = y1.max(y2);
        let width = right - left;
        let height = top - bottom;
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

        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#rect(width: {box_width}, height: {box_height}, stroke: {stroke}, radius: {radius})]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_container_item(
        &self,
        out: &mut String,
        bounds: &Option<crate::models::Box>,
        option: &Option<BTreeMap<String, String>>,
        rotation: &Option<u16>,
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
        let (left, bottom, right, top) = if let Some(bounds) = bounds {
            let (x1, y1, x2, y2) = (bounds.0[0], bounds.0[1], bounds.0[2], bounds.0[3]);
            (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2))
        } else {
            (0.0, 0.0, self.frame_width_units, self.frame_height_units)
        };
        let width = right - left;
        let height = top - bottom;

        let effective_rotation = rotation.unwrap_or(self.parent_rotation);
        let delta_rotation =
            ((effective_rotation as i32 - self.parent_rotation as i32).rem_euclid(360)) as u16;

        let context = RenderContext::new(
            width,
            height,
            self.unit,
            self.data,
            self.selected_option,
            effective_rotation,
        );
        let child_source = context.render_items(items)?;
        let mut content = child_source;
        if delta_rotation != 0 {
            content = format!("#rotate({delta_rotation}deg)[{content}]");
        }

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
