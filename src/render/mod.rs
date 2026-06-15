mod helpers;

use crate::errors::AppError;
use crate::models::{
    Fit, FontSize, LabelInput, Layout, LayoutItem, Placement, Position, Size, SizeValue,
    TemplateFormat,
};
use crate::templates::TemplateDefinition;
use helpers::{
    assets_root, build_qr_svg, escape_typst_string, fit_text_to_box, format_length,
    parse_image_data_uri, resolve_dimension, resolve_image_asset, to_nonbreaking, to_page_coords,
    typst_alignment, typst_font_options, value_to_string,
};
use serde_json::Value as JsonValue;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use typst::layout::PagedDocument;
use typst_as_lib::TypstEngine;

#[derive(Default)]
struct ImageCollector {
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
        .map_err(|err| AppError::render_failed(format!("typst compile failed: {err}")))
}

fn compile_single_doc(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
) -> Result<PagedDocument, AppError> {
    let TemplateFormat::Single { width, height } = &template.format else {
        return Err(AppError::unsupported_format(
            "render_label only supports single format",
        ));
    };

    let width_units = resolve_dimension(width)?;
    let height_units = resolve_dimension(height)?;

    let selected_option = normalize_option(template, option)?;
    let items = select_layout_items(template)?;

    let images = RefCell::new(ImageCollector::default());
    let source = build_typst_source(
        width_units,
        height_units,
        &template.unit,
        items,
        data,
        selected_option,
        &images,
    )?;
    tracing::debug!(template = %template.id, typst = %source, "render typst source");

    compile_paged(source, images.into_inner().files)
}

pub fn render_single_label(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
) -> Result<Vec<u8>, AppError> {
    let doc = compile_single_doc(template, data, option)?;
    let page = doc
        .pages
        .first()
        .ok_or_else(|| AppError::render_failed("typst did not produce any pages"))?;

    let pixmap = typst_render::render(page, template.dpi as f32 / 72.0);
    pixmap
        .encode_png()
        .map_err(|err| AppError::render_failed(format!("failed to encode png: {err}")))
}

pub fn render_single_label_pdf(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
) -> Result<Vec<u8>, AppError> {
    let doc = compile_single_doc(template, data, option)?;
    typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| AppError::render_failed(format!("failed to encode pdf: {err:?}")))
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

    let images = RefCell::new(ImageCollector::default());

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
            &images,
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

    let doc = compile_paged(source, images.into_inner().files)?;

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
    images: &RefCell<ImageCollector>,
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
        images,
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
    images: &'a RefCell<ImageCollector>,
}

impl<'a> RenderContext<'a> {
    fn new(
        frame_width_units: f32,
        frame_height_units: f32,
        unit: &'a str,
        data: &'a HashMap<String, JsonValue>,
        selected_option: Option<&'a BTreeMap<String, String>>,
        images: &'a RefCell<ImageCollector>,
    ) -> Self {
        Self {
            frame_width_units,
            frame_height_units,
            unit,
            data,
            selected_option,
            images,
        }
    }

    fn render_items(&self, items: &[LayoutItem]) -> Result<String, AppError> {
        let mut out = String::new();

        for item in items {
            match item {
                LayoutItem::Text {
                    name,
                    value,
                    placement,
                    font_size,
                    multiline,
                    alignment,
                } => {
                    let text = self.resolve_item_text("text", name.as_deref(), value.as_deref())?;
                    self.render_text_item(
                        &mut out, text, placement, font_size, *multiline, alignment,
                    )?;
                }
                LayoutItem::Qr {
                    name,
                    value,
                    placement,
                    params,
                } => {
                    let payload =
                        self.resolve_item_text("qr", name.as_deref(), value.as_deref())?;
                    self.render_qr_item(&mut out, payload, placement, params)?;
                }
                LayoutItem::Image {
                    name,
                    src,
                    placement,
                    fit,
                } => {
                    self.render_image_item(&mut out, name, src, placement, fit)?;
                }
                LayoutItem::Line { at, to, thickness } => {
                    self.render_line_item(&mut out, at, to, *thickness)?;
                }
                LayoutItem::Container {
                    placement,
                    option,
                    frame,
                    padding,
                    items,
                } => {
                    self.render_container_item(&mut out, placement, option, frame, padding, items)?;
                }
            }
        }

        Ok(out)
    }

    fn resolve_item_text(
        &self,
        kind: &str,
        name: Option<&str>,
        value: Option<&str>,
    ) -> Result<String, AppError> {
        match (name, value) {
            (Some(name), _) => Ok(value_to_string(
                self.data
                    .get(name)
                    .ok_or_else(|| AppError::missing_field(name))?,
            )),
            (None, Some(_)) => Err(AppError::render_failed(format!(
                "{kind} value interpolation is not yet supported"
            ))),
            (None, None) => Err(AppError::render_failed(format!(
                "{kind} item has neither name nor value"
            ))),
        }
    }

    fn render_text_item(
        &self,
        out: &mut String,
        raw_text: String,
        placement: &Placement,
        font_size: &FontSize,
        multiline: bool,
        alignment: &crate::models::Alignment,
    ) -> Result<(), AppError> {
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
            self.resolve_size(&placement.size, placement.max_w, placement.max_h, false)?;
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
        payload: String,
        placement: &Placement,
        params: &Option<crate::models::QrParams>,
    ) -> Result<(), AppError> {
        let (width, height) =
            self.resolve_size(&placement.size, placement.max_w, placement.max_h, false)?;
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

    fn render_image_item(
        &self,
        out: &mut String,
        name: &Option<String>,
        src: &Option<String>,
        placement: &Placement,
        fit: &Fit,
    ) -> Result<(), AppError> {
        let (bytes, fmt) = match (src, name) {
            (Some(src), _) => resolve_image_asset(&assets_root(), src)?,
            (_, Some(name)) => {
                let value = self
                    .data
                    .get(name)
                    .ok_or_else(|| AppError::missing_field(name))?;
                parse_image_data_uri(&value_to_string(value))?
            }
            (None, None) => {
                return Err(AppError::unsupported_layout_item(
                    "image requires src or name",
                ))
            }
        };
        let (width, height) =
            self.resolve_size(&placement.size, placement.max_w, placement.max_h, false)?;
        let point = placement.at.point();
        let left = point.x;
        let bottom = point.y;
        let top = bottom + height;
        let vpath = self.images.borrow_mut().add(fmt.ext(), bytes);
        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(height, self.unit)?;
        let content = format!(
            "#image(\"{vpath}\", width: {box_width}, height: {box_height}, fit: \"{fit}\")",
            fit = fit.as_typst()
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
        at: &Position,
        to: &Position,
        thickness: f32,
    ) -> Result<(), AppError> {
        let (start_x, start_y) = to_page_coords(&at.point(), self.frame_height_units);
        let (end_x, end_y) = to_page_coords(&to.point(), self.frame_height_units);
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
        writeln!(
            out,
            "#place(top + left, dx: {start_x}, dy: {start_y})[{content}]"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

        Ok(())
    }

    fn render_container_item(
        &self,
        out: &mut String,
        placement: &Placement,
        option: &Option<BTreeMap<String, String>>,
        frame: &Option<crate::models::Frame>,
        padding: &crate::models::Padding,
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
            self.resolve_size(&placement.size, placement.max_w, placement.max_h, true)?;
        let point = placement.at.point();
        let left = point.x;
        let bottom = point.y;
        let top = bottom + height;

        let inner_width = width - padding.left - padding.right;
        let inner_height = height - padding.top - padding.bottom;
        let context = RenderContext::new(
            inner_width,
            inner_height,
            self.unit,
            self.data,
            self.selected_option,
            self.images,
        );
        let child_source = context.render_items(items)?;
        let content = if padding == &crate::models::Padding::ZERO {
            child_source
        } else {
            let pad_left = format_length(padding.left, self.unit)?;
            let pad_top = format_length(padding.top, self.unit)?;
            format!("#place(top + left, dx: {pad_left}, dy: {pad_top})[{child_source}]")
        };
        let content = self.wrap_rotation(content, placement.rotate);

        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(height, self.unit)?;

        if let Some(frame) = frame {
            let stroke = format_length(frame.thickness, self.unit)?;
            let radius = if frame.rounded {
                format_length(frame.thickness * 2.0, self.unit)?
            } else {
                format_length(0.0, self.unit)?
            };
            let frame_content = format!(
                "#rect(width: {box_width}, height: {box_height}, stroke: {stroke}, radius: {radius})"
            );
            let frame_content = self.wrap_rotation(frame_content, placement.rotate);
            writeln!(
                out,
                "#place(top + left, dx: {dx}, dy: {dy})[{frame_content}]"
            )
            .map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
        }

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
    use super::{render_sheet_labels, render_single_label, render_single_label_pdf};
    use crate::models::{
        Alignment, Dimension, Fit, FontSize, Frame, LabelInput, Layout, LayoutItem, Options,
        Padding, Placement, Position, SheetPosition, Size, SizeValue, TemplateFormat,
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
                name: Some("message".to_string()),
                value: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(5.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
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
                    name: Some("message".to_string()),
                    value: None,
                    placement: Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    font_size: FontSize::Fixed(10.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: Some("code".to_string()),
                    value: None,
                    placement: Placement {
                        at: Position([20.0, 0.0]),
                        size: Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    params: None,
                },
                LayoutItem::Line {
                    at: Position([0.0, 1.0]),
                    to: Position([30.0, 1.0]),
                    thickness: 0.2,
                },
                LayoutItem::Container {
                    placement: Placement {
                        at: Position([0.5, 1.5]),
                        size: Size([SizeValue::Value(29.0), SizeValue::Value(18.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    option: None,
                    frame: Some(Frame {
                        thickness: 0.2,
                        rounded: true,
                    }),
                    padding: Padding::ZERO,
                    items: Vec::new(),
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
                name: Some("message".to_string()),
                value: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
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

    const PNG_1X1_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

    fn image_single_template() -> TemplateDefinition {
        TemplateDefinition {
            id: "img".to_string(),
            name: "Img".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0),
                height: Dimension::Fixed(20.0),
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Image {
                name: Some("logo".to_string()),
                src: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                fit: Fit::Contain,
            }]),
            version: None,
        }
    }

    #[test]
    fn render_single_label_with_image_produces_png() {
        let template = image_single_template();
        let data = HashMap::from([(
            "logo".to_string(),
            json!(format!("data:image/png;base64,{PNG_1X1_B64}")),
        )]);
        let png = render_single_label(&template, &data, None).expect("render image");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_image_missing_data_errors() {
        let template = image_single_template();
        let data = HashMap::new();
        assert!(render_single_label(&template, &data, None).is_err());
    }

    #[test]
    fn render_image_invalid_base64_errors() {
        let template = image_single_template();
        let data = HashMap::from([(
            "logo".to_string(),
            json!("data:image/png;base64,@@@not-base64@@@"),
        )]);
        assert!(render_single_label(&template, &data, None).is_err());
    }

    #[test]
    fn render_sheet_labels_with_image_produces_pdf() {
        let template = TemplateDefinition {
            id: "sheetimg".to_string(),
            name: "Sheet".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Sheet {
                paper_width: 20.0,
                paper_height: 20.0,
                label_width: 20.0,
                label_height: 20.0,
                positions: vec![SheetPosition([0.0, 0.0])],
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Image {
                name: Some("logo".to_string()),
                src: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                fit: Fit::Contain,
            }]),
            version: None,
        };
        let labels = vec![LabelInput {
            data: HashMap::from([(
                "logo".to_string(),
                json!(format!("data:image/png;base64,{PNG_1X1_B64}")),
            )]),
            option: None,
        }];
        let pdf = render_sheet_labels(&template, &labels, 0).expect("render sheet image");
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
    }

    fn image_single_template_with_src(src: &str) -> TemplateDefinition {
        let mut template = image_single_template();
        template.layout = Layout::Items(vec![LayoutItem::Image {
            name: None,
            src: Some(src.to_string()),
            placement: Placement {
                at: Position([0.0, 0.0]),
                size: Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            fit: Fit::Contain,
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
        let png = render_single_label(&template, &data, None).expect("render svg");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_static_image_src() {
        use base64::Engine as _;
        use std::time::{SystemTime, UNIX_EPOCH};
        let mut dir = std::env::temp_dir();
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("labeler_render_assets_{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(PNG_1X1_B64)
            .unwrap();
        std::fs::write(dir.join("logo.png"), &bytes).unwrap();
        std::env::set_var("LABELER_ASSETS_DIR", &dir);

        let data = HashMap::new();
        let png = render_single_label(&image_single_template_with_src("logo.png"), &data, None)
            .expect("render static src");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // A missing asset is rejected at render time.
        assert!(
            render_single_label(&image_single_template_with_src("missing.png"), &data, None)
                .is_err()
        );

        std::env::remove_var("LABELER_ASSETS_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_single_label_produces_pdf() {
        let template = TemplateDefinition {
            id: "pdf".to_string(),
            name: "Pdf".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(20.0),
                height: Dimension::Fixed(10.0),
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: Some("message".to_string()),
                value: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(5.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(10.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        let data = HashMap::from([("message".to_string(), json!("Hello"))]);
        let pdf = render_single_label_pdf(&template, &data, None).expect("render pdf");
        assert!(pdf.starts_with(b"%PDF"), "missing PDF header");
    }

    #[test]
    fn starter_tape_templates_render() {
        let registry =
            crate::templates::TemplateRegistry::load_from_dir("templates").expect("load templates");
        let data = HashMap::from([
            ("message".to_string(), json!("Hello world")),
            ("code".to_string(), json!("QR-1")),
        ]);
        for id in ["brother12mm", "brother18mm", "brother24mm"] {
            let template = registry.get(id).unwrap_or_else(|| panic!("template {id}"));
            let png = render_single_label(template, &data, None).expect("render tape");
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        }
    }
}
