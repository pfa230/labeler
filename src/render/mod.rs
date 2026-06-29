mod helpers;

pub const MAX_RENDER_DPI: u32 = 1200;

use crate::errors::AppError;
use crate::models::{
    Dimension, Fit, FontSize, LabelInput, Layout, LayoutItem, Placement, Position, Rotation, Size,
    SizeValue, TemplateFormat,
};
use crate::templates::TemplateDefinition;
use helpers::{
    assets_root, binarize_rgba, build_qr_svg, escape_typst_string, fit_text_auto_length,
    fit_text_to_box, format_length, interpolate, line_height_units, parse_image_data_uri,
    resolve_dimension, resolve_image_asset, to_nonbreaking, to_page_coords, typst_alignment,
    typst_font_options, value_to_string, MeasuredText,
};
use serde_json::Value as JsonValue;
use std::cell::{Cell, RefCell};
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
    env: &RenderEnv,
) -> Result<PagedDocument, AppError> {
    if !matches!(template.format, TemplateFormat::Single { .. }) {
        return Err(AppError::unsupported_format(
            "render_label only supports single format",
        ));
    }
    compile_label_doc(template, data, option, env)
}

/// Compile a single label for any template: a `Single` uses its width/height; a `Sheet`
/// renders one slot at label_width/label_height. Shared by `compile_single_doc` (after its
/// Single-only guard) and the thumbnail path.
fn compile_label_doc(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    env: &RenderEnv,
) -> Result<PagedDocument, AppError> {
    let unit = &template.unit;
    let selected_option = normalize_option(template, option)?;
    let items = select_layout_items(template)?;
    let images = RefCell::new(ImageCollector::default());

    // Resolve initial width/height; Dynamic single may be overridden after measurement.
    let (mut width_units, height_units) = match &template.format {
        TemplateFormat::Single { width, height, .. } => {
            (resolve_dimension(width)?, resolve_dimension(height)?)
        }
        TemplateFormat::Sheet {
            label_width,
            label_height,
            ..
        } => (*label_width, *label_height),
    };

    // For dynamic-width single templates, run a measurement pass and clamp the page width.
    let measured: Vec<MeasuredText>;
    let cursor_cell: Cell<usize>;

    if let TemplateFormat::Single {
        width: Dimension::Dynamic { min, max },
        ..
    } = &template.format
    {
        let max_w =
            max.ok_or_else(|| AppError::unsupported_format("dynamic single width requires max"))?;
        let min_w =
            min.ok_or_else(|| AppError::unsupported_format("dynamic single width requires min"))?;
        let mut m: Vec<MeasuredText> = Vec::new();
        {
            let probe = RenderContext::new(
                (max_w, height_units),
                unit,
                data,
                selected_option,
                env,
                &images,
                None,
            );
            let content_extent = probe.measure(items, max_w, &mut m)?;
            width_units = content_extent.clamp(min_w, max_w);
        }
        measured = m;
        cursor_cell = Cell::new(0usize);
    } else {
        measured = Vec::new();
        cursor_cell = Cell::new(0usize);
    }

    let auto_length = if measured.is_empty() {
        None
    } else {
        Some(AutoLength {
            texts: &measured,
            cursor: &cursor_cell,
        })
    };

    let mut source = String::new();
    let page_width = format_length(width_units, unit)?;
    let page_height = format_length(height_units, unit)?;
    writeln!(
        source,
        "#set page(width: {page_width}, height: {page_height}, margin: 0{unit})"
    )
    .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
    writeln!(source, "#set text(font: (\"Inter Variable\", \"Inter\"))")
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

    let context = RenderContext::new(
        (width_units, height_units),
        unit,
        data,
        selected_option,
        env,
        &images,
        auto_length,
    );
    source.push_str(&context.render_items(items)?);
    // Assert we consumed exactly the texts we measured.
    if cursor_cell.get() != measured.len() && !measured.is_empty() {
        return Err(AppError::render_failed(format!(
            "auto-length cursor mismatch: consumed {} of {} measured texts",
            cursor_cell.get(),
            measured.len()
        )));
    }
    tracing::debug!(template = %template.id, typst = %source, "render typst source");
    compile_paged(source, images.into_inner().files)
}

/// Render a single representative label to PNG. For sheets, renders one slot.
pub fn render_thumbnail_png(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let doc = compile_label_doc(template, data, option, &env)?;
    let page = doc
        .pages
        .first()
        .ok_or_else(|| AppError::render_failed("typst did not produce any pages"))?;
    let pixmap = typst_render::render(page, template.dpi as f32 / 72.0);
    pixmap
        .encode_png()
        .map_err(|err| AppError::render_failed(format!("failed to encode png: {err}")))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorMode {
    #[default]
    Color,
    BiLevel,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ImageRenderOptions {
    pub color_mode: ColorMode,
    pub resolution_dpi: Option<u32>,
}

pub fn render_single_label(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    render_single_label_image(
        template,
        data,
        option,
        settings,
        datetime,
        ImageRenderOptions::default(),
    )
}

pub fn render_single_label_image(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
    opts: ImageRenderOptions,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let doc = compile_single_doc(template, data, option, &env)?;
    let page = doc
        .pages
        .first()
        .ok_or_else(|| AppError::render_failed("typst did not produce any pages"))?;

    let dpi = opts.resolution_dpi.unwrap_or(template.dpi);
    let mut pixmap = typst_render::render(page, dpi as f32 / 72.0);
    if opts.color_mode == ColorMode::BiLevel {
        binarize_rgba(pixmap.data_mut());
    }
    pixmap
        .encode_png()
        .map_err(|err| AppError::render_failed(format!("failed to encode png: {err}")))
}

pub fn render_single_label_pdf(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let doc = compile_single_doc(template, data, option, &env)?;
    typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| AppError::render_failed(format!("failed to encode pdf: {err:?}")))
}

pub fn render_sheet_pages(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    start_slot: u32,
    settings: &BTreeMap<String, String>,
    datetime: &crate::datetime_fmt::DateTimeResolver,
) -> Result<Vec<u8>, AppError> {
    let env = RenderEnv { settings, datetime };
    let TemplateFormat::Sheet {
        paper_width,
        paper_height,
        label_width,
        label_height,
        positions,
    } = &template.format
    else {
        return Err(AppError::unsupported_format(
            "render_sheet_pages only supports sheet format",
        ));
    };

    let start_slot = start_slot as usize;
    if start_slot >= positions.len() && !labels.is_empty() {
        return Err(AppError::invalid_request("start_slot is out of range"));
    }

    let page_width_units = *paper_width;
    let page_height_units = *paper_height;
    let unit = &template.unit;
    let items = select_layout_items(template)?;

    let slots_per_page = positions.len();
    let mut placements: Vec<(usize, usize)> = Vec::with_capacity(labels.len());
    let mut slot = start_slot;
    let mut page = 0usize;
    for _ in labels {
        if slot >= slots_per_page {
            page += 1;
            slot = 0;
        }
        placements.push((page, slot));
        slot += 1;
    }
    let page_count = placements.last().map(|(p, _)| p + 1).unwrap_or(1);

    let images = RefCell::new(ImageCollector::default());

    let mut rendered: Vec<String> = Vec::with_capacity(labels.len());
    let mut failures: Vec<crate::errors::BatchFailure> = Vec::new();
    for (idx, lbl) in labels.iter().enumerate() {
        let selected_option = match normalize_option(template, lbl.option.as_ref()) {
            Ok(opt) => opt,
            Err(err) => {
                failures.push(crate::errors::BatchFailure {
                    index: idx,
                    code: err.code(),
                    message: err.message_text(),
                });
                rendered.push(String::new());
                continue;
            }
        };
        let context = RenderContext::new(
            (*label_width, *label_height),
            unit,
            &lbl.data,
            selected_option,
            &env,
            &images,
            None,
        );
        match context.render_items(items) {
            Ok(content) => rendered.push(content),
            Err(err) => {
                failures.push(crate::errors::BatchFailure {
                    index: idx,
                    code: err.code(),
                    message: err.message_text(),
                });
                rendered.push(String::new());
            }
        }
    }
    if !failures.is_empty() {
        return Err(AppError::batch_invalid(failures));
    }

    let mut source = String::new();
    let page_w = format_length(page_width_units, unit)?;
    let page_h = format_length(page_height_units, unit)?;
    for p in 0..page_count {
        if p == 0 {
            writeln!(
                source,
                "#set page(width: {page_w}, height: {page_h}, margin: 0{unit})"
            )
            .map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
            writeln!(source, "#set text(font: (\"Inter Variable\", \"Inter\"))").map_err(
                |err| AppError::render_failed(format!("failed to build typst source: {err}")),
            )?;
        } else {
            writeln!(source, "#pagebreak()").map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
        }
        for (idx, (lp, ls)) in placements.iter().enumerate() {
            if *lp != p {
                continue;
            }
            let point = positions[*ls].point();
            let top = point.y + *label_height;
            let dx = format_length(point.x, unit)?;
            let dy = format_length(page_height_units - top, unit)?;
            let bw = format_length(*label_width, unit)?;
            let bh = format_length(*label_height, unit)?;
            writeln!(
                source,
                "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {bw}, height: {bh}, clip: true)[{}]]",
                rendered[idx]
            )
            .map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
        }
    }
    tracing::debug!(template = %template.id, typst = %source, "render typst source");

    let doc = compile_paged(source, images.into_inner().files)?;
    typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| AppError::render_failed(format!("failed to encode pdf: {err:?}")))
}

/// Count rendered PDF pages by counting "/Type /Page" objects (excluding the "/Type /Pages" tree
/// node). Used by pagination tests.
pub fn count_pdf_pages(pdf: &[u8]) -> usize {
    let needle = b"/Type /Page";
    let mut count = 0usize;
    let mut i = 0;
    while let Some(pos) = pdf[i..].windows(needle.len()).position(|w| w == needle) {
        let at = i + pos;
        let after = at + needle.len();
        if pdf.get(after) != Some(&b's') {
            count += 1;
        }
        i = after;
    }
    count
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

/// State threaded through the render pass for dynamic-width auto-length labels.
/// Both fields are `None` for fixed-width labels and sheets.
struct AutoLength<'a> {
    texts: &'a [MeasuredText],
    cursor: &'a Cell<usize>,
}

/// Render-time environment: the variables map and the datetime resolver, passed together through
/// every render call so related configuration travels as a unit.
struct RenderEnv<'a> {
    settings: &'a BTreeMap<String, String>,
    datetime: &'a crate::datetime_fmt::DateTimeResolver<'a>,
}

struct RenderContext<'a> {
    frame_width_units: f32,
    frame_height_units: f32,
    unit: &'a str,
    data: &'a HashMap<String, JsonValue>,
    selected_option: Option<&'a BTreeMap<String, String>>,
    env: &'a RenderEnv<'a>,
    images: &'a RefCell<ImageCollector>,
    auto_length: Option<AutoLength<'a>>,
}

impl<'a> RenderContext<'a> {
    fn new(
        frame: (f32, f32),
        unit: &'a str,
        data: &'a HashMap<String, JsonValue>,
        selected_option: Option<&'a BTreeMap<String, String>>,
        env: &'a RenderEnv<'a>,
        images: &'a RefCell<ImageCollector>,
        auto_length: Option<AutoLength<'a>>,
    ) -> Self {
        Self {
            frame_width_units: frame.0,
            frame_height_units: frame.1,
            unit,
            data,
            selected_option,
            env,
            images,
            auto_length,
        }
    }

    /// Walk items computing content right-extent and recording auto-width text fits (pre-order).
    /// `budget_w` is the available width: page max at the top frame, inner width inside a container.
    fn measure(
        &self,
        items: &[LayoutItem],
        budget_w: f32,
        out: &mut Vec<MeasuredText>,
    ) -> Result<f32, AppError> {
        let mut extent = 0.0f32;
        for item in items {
            let right = match item {
                LayoutItem::Text {
                    name,
                    value,
                    placement,
                    font_size,
                    multiline,
                    ..
                } => {
                    let text = self.resolve_item_text("text", name.as_deref(), value.as_deref())?;
                    let at = placement.at.point();
                    let size_w = &placement.size.0[0];
                    let box_h = placement.size.0[1]
                        .value()
                        .unwrap_or(self.frame_height_units - at.y);
                    if size_w.is_auto() {
                        let budget = (budget_w - at.x).max(0.0);
                        let m = fit_text_auto_length(
                            &text, font_size, *multiline, budget, box_h, self.unit,
                        )?;
                        let w = m.width;
                        out.push(m);
                        at.x + w
                    } else {
                        at.x + size_w.value().unwrap_or(0.0)
                    }
                }
                LayoutItem::Qr { placement, .. } | LayoutItem::Image { placement, .. } => {
                    let at_x = placement.at.point().x;
                    let w = placement.size.0[0]
                        .value()
                        .unwrap_or((budget_w - at_x).max(0.0));
                    at_x + w
                }
                LayoutItem::Line { at, to, .. } => at.point().x.max(to.point().x),
                LayoutItem::Container {
                    placement,
                    option,
                    padding,
                    items,
                    ..
                } => {
                    if let Some(opt) = option {
                        if let Some(sel) = self.selected_option {
                            let matches = opt.iter().all(|(n, v)| sel.get(n) == Some(v));
                            if !matches {
                                continue;
                            }
                        }
                    }
                    let at_x = placement.at.point().x;
                    let size_w = &placement.size.0[0];
                    if size_w.is_auto() {
                        // auto-width container: width determined by children + padding
                        let inner_budget =
                            ((budget_w - at_x) - padding.left - padding.right).max(0.0);
                        let inner_h = (self.frame_height_units
                            - placement.at.point().y
                            - padding.top
                            - padding.bottom)
                            .max(0.0);
                        let ctx = RenderContext::new(
                            (inner_budget, inner_h),
                            self.unit,
                            self.data,
                            self.selected_option,
                            self.env,
                            self.images,
                            None,
                        );
                        let child_extent = ctx.measure(items, inner_budget, out)?;
                        at_x + padding.left + child_extent + padding.right
                    } else {
                        // fixed-width container: right extent is at.x + explicit width
                        let explicit_w = size_w.value().unwrap_or(0.0);
                        let rotation = placement
                            .rotate
                            .and_then(Rotation::from_degrees)
                            .unwrap_or(Rotation::R0);
                        // Rotated containers are self-contained (explicit size, no auto descendants);
                        // their author-space children must not be measured in physical-horizontal
                        // terms, so do not recurse into them (#98).
                        if !rotation.is_rotated() {
                            let inner_w = (explicit_w - padding.left - padding.right).max(0.0);
                            let inner_h = {
                                let size_h = &placement.size.0[1];
                                let explicit_h = size_h
                                    .value()
                                    .unwrap_or(self.frame_height_units - placement.at.point().y);
                                (explicit_h - padding.top - padding.bottom).max(0.0)
                            };
                            let ctx = RenderContext::new(
                                (inner_w, inner_h),
                                self.unit,
                                self.data,
                                self.selected_option,
                                self.env,
                                self.images,
                                None,
                            );
                            ctx.measure(items, inner_w, out)?;
                        }
                        at_x + explicit_w
                    }
                }
            };
            extent = extent.max(right);
        }
        Ok(extent)
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
            (None, Some(value)) => {
                interpolate(value, self.data, self.env.settings, self.env.datetime)
            }
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

        let point = placement.at.point();
        let left = point.x;

        // When auto-length is active and this text item has auto width, consume the next measured fit.
        if let Some(al) = &self.auto_length {
            if placement.size.0[0].is_auto() {
                let idx = al.cursor.get();
                let m = al.texts.get(idx).ok_or_else(|| {
                    AppError::render_failed(format!("auto-length cursor overrun at index {idx}"))
                })?;
                al.cursor.set(idx + 1);

                // The text's allotted vertical slot (`size` height or the remaining frame height).
                let slot_h = self.resolve_size_value(
                    &placement.size.0[1],
                    placement.max_h,
                    Some(self.frame_height_units - point.y),
                    "height",
                )?;
                let line_h = line_height_units(m.font, self.unit)?;
                let n = m.lines.len() as f32;
                let block_h = line_h * n;
                let slot_top = self.frame_height_units - point.y - slot_h;
                use crate::models::VerticalAlign;
                let dy_units = match alignment.vertical {
                    VerticalAlign::Top => slot_top,
                    VerticalAlign::Bottom => slot_top + (slot_h - block_h).max(0.0),
                    VerticalAlign::Center => slot_top + ((slot_h - block_h) / 2.0).max(0.0),
                };
                let body = m
                    .lines
                    .iter()
                    .map(|l| format!("#text(\"{}\", size: {}pt)", escape_typst_string(l), m.font))
                    .collect::<Vec<_>>()
                    .join("#linebreak()");
                // Derive the horizontal keyword directly; `typst_alignment` returns a combined
                // "vertical + horizontal" String, not a tuple, so do NOT use it here.
                use crate::models::HorizontalAlign;
                let halign = match alignment.horizontal {
                    HorizontalAlign::Left => "left",
                    HorizontalAlign::Center => "center",
                    HorizontalAlign::Right => "right",
                };
                let inner = format!("#align({halign})[{body}]");
                let dx = format_length(left, self.unit)?;
                let dy = format_length(dy_units, self.unit)?;
                let box_width = format_length(m.width, self.unit)?;
                let box_height = format_length(block_h, self.unit)?;
                let content = self.wrap_rotation(inner, placement.rotate);
                writeln!(
                    out,
                    "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{content}]]"
                )
                .map_err(|err| {
                    AppError::render_failed(format!("failed to build typst source: {err}"))
                })?;
                return Ok(());
            }
        }

        let (width, box_height_units) =
            self.resolve_size(&placement.size, placement.max_w, placement.max_h, false)?;
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
        let point = placement.at.point();
        let left = point.x;
        let rotation = placement
            .rotate
            .and_then(Rotation::from_degrees)
            .unwrap_or(Rotation::R0);

        if !rotation.is_rotated() {
            // R0: unchanged path (output byte-identical to before).
            // On a dynamic-width (auto-length) label, an auto-width container must span only
            // the remaining width from its left edge, not the full frame width. This matches the
            // measurement pass which budgets (budget_w - at.x) - padding for the container.
            let width = if self.auto_length.is_some() && placement.size.0[0].is_auto() {
                (self.frame_width_units - left).max(0.0)
            } else {
                self.resolve_size(&placement.size, placement.max_w, placement.max_h, true)?
                    .0
            };
            let height = self
                .resolve_size(&placement.size, placement.max_w, placement.max_h, true)?
                .1;
            let bottom = point.y;
            let top = bottom + height;

            let inner_width = width - padding.left - padding.right;
            let inner_height = height - padding.top - padding.bottom;
            let child_auto_length = self.auto_length.as_ref().map(|al| AutoLength {
                texts: al.texts,
                cursor: al.cursor,
            });
            let context = RenderContext::new(
                (inner_width, inner_height),
                self.unit,
                self.data,
                self.selected_option,
                self.env,
                self.images,
                child_auto_length,
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

            return Ok(());
        }

        // Rotated path (R90/R180/R270). Validation guarantees an explicit size and no auto here.
        let width = self
            .resolve_size(&placement.size, placement.max_w, placement.max_h, true)?
            .0;
        let height = self
            .resolve_size(&placement.size, placement.max_w, placement.max_h, true)?
            .1;
        let bottom = point.y;
        let top = bottom + height;

        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(height, self.unit)?;

        // Author canvas: full physical box, swapped for 90/270. Padding is author-space.
        let (canvas_w, canvas_h) = if rotation.swaps_axes() {
            (height, width)
        } else {
            (width, height)
        };
        let content_w = canvas_w - padding.left - padding.right;
        let content_h = canvas_h - padding.top - padding.bottom;

        // No auto_length under rotation (validation forbids auto descendants).
        let context = RenderContext::new(
            (content_w, content_h),
            self.unit,
            self.data,
            self.selected_option,
            self.env,
            self.images,
            None,
        );
        let child_source = context.render_items(items)?;

        let canvas_w_len = format_length(canvas_w, self.unit)?;
        let canvas_h_len = format_length(canvas_h, self.unit)?;
        let inner = if padding == &crate::models::Padding::ZERO {
            child_source
        } else {
            let pad_left = format_length(padding.left, self.unit)?;
            let pad_top = format_length(padding.top, self.unit)?;
            format!("#place(top + left, dx: {pad_left}, dy: {pad_top})[{child_source}]")
        };
        let canvas = format!("#box(width: {canvas_w_len}, height: {canvas_h_len})[{inner}]");
        let rotated = self.wrap_rotation(canvas, placement.rotate);

        // Frame is physical and unrotated.
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
            writeln!(
                out,
                "#place(top + left, dx: {dx}, dy: {dy})[{frame_content}]"
            )
            .map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
        }

        // Single placement of the rotated author canvas, clipped to the physical box.
        writeln!(
            out,
            "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {box_width}, height: {box_height}, clip: true)[{rotated}]]"
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
        // Typst positive angles rotate clockwise (screen coords); our `rotate` contract is
        // counter-clockwise, so negate. `reflow: true` normalizes the box to the rotated footprint.
        match rotate
            .and_then(Rotation::from_degrees)
            .unwrap_or(Rotation::R0)
        {
            Rotation::R0 => content,
            Rotation::R90 => format!("#rotate(-90deg, reflow: true)[{content}]"),
            Rotation::R180 => format!("#rotate(180deg, reflow: true)[{content}]"),
            Rotation::R270 => format!("#rotate(90deg, reflow: true)[{content}]"),
        }
    }
}

/// 1×1 transparent PNG data URI: a valid stand-in for data-bound image fields.
pub const SAMPLE_PNG_DATA_URI: &str =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

/// Collect `{token}` field names from a well-formed template string.
///
/// Skips `{{` escapes, empty tokens (`{}`), and `vars.*` tokens (resolved from the
/// settings store, not from request data). This is not a full `interpolate` parser:
/// it does not error on malformed input such as unterminated `{` or `}}`; templates
/// that are actually malformed fail later at render time.
fn collect_data_tokens(s: &str, out: &mut Vec<String>) {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            continue;
        }
        let mut token = String::new();
        for tc in chars.by_ref() {
            if tc == '}' {
                if !token.is_empty()
                    && !token.starts_with("vars.")
                    && token != "datetime"
                    && !token.starts_with("datetime.")
                {
                    out.push(token);
                }
                break;
            }
            token.push(tc);
        }
    }
}

fn walk_placeholder(items: &[LayoutItem], text: &mut Vec<String>, image: &mut Vec<String>) {
    for item in items {
        match item {
            LayoutItem::Text { name, value, .. } | LayoutItem::Qr { name, value, .. } => {
                if let Some(n) = name {
                    text.push(n.clone());
                }
                if let Some(v) = value {
                    collect_data_tokens(v, text);
                }
            }
            LayoutItem::Image { name, src, .. } => {
                if let Some(n) = name {
                    image.push(n.clone());
                }
                if let Some(s) = src {
                    collect_data_tokens(s, image);
                }
            }
            LayoutItem::Container { items, .. } => walk_placeholder(items, text, image),
            LayoutItem::Line { .. } => {}
        }
    }
}

/// Build non-empty placeholder data for every referenced data field. Image fields get a 1×1 PNG;
/// other fields get their own name as a stand-in. `{vars.*}` is excluded (resolved from the store).
pub fn placeholder_data(template: &TemplateDefinition) -> HashMap<String, JsonValue> {
    let Layout::Items(items) = &template.layout;
    let mut text = Vec::new();
    let mut image = Vec::new();
    walk_placeholder(items, &mut text, &mut image);
    let mut data = HashMap::new();
    for f in text {
        data.entry(f.clone())
            .or_insert_with(|| JsonValue::String(f));
    }
    for f in image {
        // image wins over a same-named text guess
        data.insert(f, JsonValue::String(SAMPLE_PNG_DATA_URI.to_string()));
    }
    data
}

/// First allowed value per declared option, or None when the template declares no options.
pub fn default_option_selection(template: &TemplateDefinition) -> Option<BTreeMap<String, String>> {
    let options = template.options.as_ref()?;
    let selection: BTreeMap<String, String> = options
        .allowed()
        .iter()
        .filter_map(|(name, values)| values.first().map(|v| (name.clone(), v.clone())))
        .collect();
    if selection.is_empty() {
        None
    } else {
        Some(selection)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        count_pdf_pages, default_option_selection, placeholder_data, render_sheet_pages,
        render_single_label, render_single_label_pdf, render_thumbnail_png, SAMPLE_PNG_DATA_URI,
    };
    use crate::models::{
        Alignment, Dimension, Fit, FontSize, Frame, LabelInput, Layout, LayoutItem, Options,
        Padding, Placement, Position, SheetPosition, Size, SizeValue, TemplateFormat,
    };
    use crate::templates::TemplateDefinition;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn measure_skips_children_of_rotated_container() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new((80.0, 40.0), "mm", &data, None, &env, &images, None);

        let auto_text = LayoutItem::Text {
            name: None,
            value: Some("hello".to_string()),
            placement: Placement {
                at: Position([0.0, 0.0]),
                size: Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(10.0),
                ]),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(6.0),
            multiline: false,
            alignment: Alignment::default(),
        };
        let make_container = |rotate: Option<f32>| LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                size: Size([SizeValue::Value(80.0), SizeValue::Value(40.0)]),
                max_w: None,
                max_h: None,
                rotate,
            },
            option: None,
            frame: None,
            padding: Padding::ZERO,
            items: vec![auto_text.clone()],
        };

        let mut out_rot = Vec::new();
        ctx.measure(&[make_container(Some(90.0))], 80.0, &mut out_rot)
            .unwrap();
        assert!(
            out_rot.is_empty(),
            "rotated container must not measure its children"
        );

        let mut out_plain = Vec::new();
        ctx.measure(&[make_container(None)], 80.0, &mut out_plain)
            .unwrap();
        assert_eq!(
            out_plain.len(),
            1,
            "non-rotated container measures its auto child"
        );
    }

    #[test]
    fn r0_container_source_unchanged() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new((80.0, 40.0), "mm", &data, None, &env, &images, None);
        let container = LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                size: Size([SizeValue::Value(80.0), SizeValue::Value(40.0)]),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            option: None,
            frame: Some(Frame {
                thickness: 0.3,
                rounded: false,
            }),
            padding: Padding::ZERO,
            items: vec![],
        };
        let src = ctx.render_items(&[container]).expect("render r0 container");
        assert!(
            !src.contains("#rotate"),
            "R0 container must not emit #rotate"
        );
        assert!(
            src.contains("clip: true"),
            "R0 container keeps its single clipped box"
        );
    }

    fn rotated_container_template(rotate: f32, items: Vec<LayoutItem>) -> TemplateDefinition {
        TemplateDefinition {
            id: "rot".to_string(),
            name: "Rot".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(80.0),
                height: Dimension::Fixed(40.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(80.0), SizeValue::Value(40.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: Some(rotate),
                },
                option: None,
                frame: Some(Frame {
                    thickness: 0.3,
                    rounded: false,
                }),
                padding: Padding::ZERO,
                items,
            }]),
            version: None,
        }
    }

    #[test]
    fn rotated_container_renders_to_png() {
        let template = rotated_container_template(
            90.0,
            vec![LayoutItem::Text {
                name: None,
                value: Some("VERTICAL".to_string()),
                placement: Placement {
                    at: Position([2.0, 2.0]),
                    size: Size([SizeValue::Value(30.0), SizeValue::Value(8.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(8.0),
                multiline: false,
                alignment: Alignment::default(),
            }],
        );
        let data = HashMap::new();
        let png = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect("render rotated container");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    // Returns the dark-pixel fraction of each image quadrant: [TL, TR, BL, BR].
    fn quadrant_dark_fraction(png: &[u8]) -> [f32; 4] {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let (mw, mh) = (w / 2, h / 2);
        let mut dark = [0u32; 4];
        let mut total = [0u32; 4];
        for y in 0..h {
            for x in 0..w {
                let q = match (x < mw, y < mh) {
                    (true, true) => 0,
                    (false, true) => 1,
                    (true, false) => 2,
                    (false, false) => 3,
                };
                total[q] += 1;
                if img.get_pixel(x, y).0[0] < 128 {
                    dark[q] += 1;
                }
            }
        }
        [
            dark[0] as f32 / total[0] as f32,
            dark[1] as f32 / total[1] as f32,
            dark[2] as f32 / total[2] as f32,
            dark[3] as f32 / total[3] as f32,
        ]
    }

    #[test]
    fn rotation_ccw_corner_mapping_r90() {
        // A QR marker at the author canvas bottom-left (40x80 portrait); under CCW 90 it must land
        // in the physical bottom-right of the 80x40 label (spec table R90: BL -> BR).
        let template = rotated_container_template(
            90.0,
            vec![LayoutItem::Qr {
                name: None,
                value: Some("X".to_string()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(14.0), SizeValue::Value(14.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
            }],
        );
        let data = HashMap::new();
        let png = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect("render corner marker");
        let q = quadrant_dark_fraction(&png);
        assert!(
            q[3] > q[0] && q[3] > q[1] && q[3] > q[2],
            "QR at author BL must land physical BR under CCW 90; dark [TL,TR,BL,BR]={q:?}"
        );
    }

    #[test]
    fn rotation_ccw_corner_mapping_r180_and_r270() {
        let qr = || {
            vec![LayoutItem::Qr {
                name: None,
                value: Some("X".to_string()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(14.0), SizeValue::Value(14.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
            }]
        };
        let data = HashMap::new();

        // R180: author BL -> physical TR.
        let png = render_single_label(
            &rotated_container_template(180.0, qr()),
            &data,
            None,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render r180");
        let q = quadrant_dark_fraction(&png);
        assert!(
            q[1] > q[0] && q[1] > q[2] && q[1] > q[3],
            "R180 BL->TR; dark [TL,TR,BL,BR]={q:?}"
        );

        // R270: author BL -> physical TL.
        let png = render_single_label(
            &rotated_container_template(270.0, qr()),
            &data,
            None,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render r270");
        let q = quadrant_dark_fraction(&png);
        assert!(
            q[0] > q[1] && q[0] > q[2] && q[0] > q[3],
            "R270 BL->TL; dark [TL,TR,BL,BR]={q:?}"
        );
    }

    #[test]
    fn nested_rotated_containers_render() {
        // Outer R90 (frame + asymmetric author-space padding) containing an inner R90, frame-less
        // container with a text child. Proves nested rotation emits valid, compilable Typst.
        let inner = LayoutItem::Container {
            placement: Placement {
                at: Position([2.0, 2.0]),
                size: Size([SizeValue::Value(24.0), SizeValue::Value(24.0)]),
                max_w: None,
                max_h: None,
                rotate: Some(90.0),
            },
            option: None,
            frame: None,
            padding: Padding::ZERO,
            items: vec![LayoutItem::Text {
                name: None,
                value: Some("inner".to_string()),
                placement: Placement {
                    at: Position([1.0, 1.0]),
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(8.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                multiline: false,
                alignment: Alignment::default(),
            }],
        };
        let outer = LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                size: Size([SizeValue::Value(80.0), SizeValue::Value(40.0)]),
                max_w: None,
                max_h: None,
                rotate: Some(90.0),
            },
            option: None,
            frame: Some(Frame {
                thickness: 0.3,
                rounded: false,
            }),
            padding: Padding {
                top: 2.0,
                right: 4.0,
                bottom: 6.0,
                left: 8.0,
            },
            items: vec![inner],
        };
        let template = TemplateDefinition {
            id: "nest".to_string(),
            name: "Nest".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(80.0),
                height: Dimension::Fixed(40.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![outer]),
            version: None,
        };
        let png = render_single_label(
            &template,
            &HashMap::new(),
            None,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render nested rotated containers");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    fn no_settings() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn no_datetime() -> crate::datetime_fmt::DateTimeResolver<'static> {
        use std::sync::OnceLock;
        static EMPTY: OnceLock<std::collections::BTreeMap<String, String>> = OnceLock::new();
        let formats = EMPTY.get_or_init(std::collections::BTreeMap::new);
        crate::datetime_fmt::DateTimeResolver {
            formats,
            now: chrono::Local::now(),
        }
    }

    fn two_slot_sheet() -> TemplateDefinition {
        TemplateDefinition {
            id: "sheet2".to_string(),
            name: "Sheet2".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Sheet {
                paper_width: 20.0,
                paper_height: 10.0,
                label_width: 10.0,
                label_height: 10.0,
                positions: vec![SheetPosition([0.0, 0.0]), SheetPosition([10.0, 0.0])],
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: Some("message".to_string()),
                value: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(8.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        }
    }

    fn sheet_label(msg: &str) -> LabelInput {
        LabelInput {
            data: HashMap::from([("message".to_string(), json!(msg))]),
            option: None,
        }
    }

    #[test]
    fn sheet_pages_paginate_overflow() {
        let labels = vec![sheet_label("a"), sheet_label("b"), sheet_label("c")];
        let pdf = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            0,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render");
        assert!(pdf.starts_with(b"%PDF"));
        assert_eq!(count_pdf_pages(&pdf), 2);
    }

    #[test]
    fn sheet_pages_respect_start_slot() {
        let labels = vec![sheet_label("a"), sheet_label("b")];
        let pdf = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            1,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render");
        assert!(pdf.starts_with(b"%PDF"));
        assert_eq!(count_pdf_pages(&pdf), 2);
    }

    #[test]
    fn sheet_pages_collect_bad_label_index() {
        let labels = vec![
            sheet_label("a"),
            LabelInput {
                data: HashMap::new(),
                option: None,
            },
        ];
        let err = render_sheet_pages(
            &two_slot_sheet(),
            &labels,
            0,
            &no_settings(),
            &no_datetime(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
    }

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
                media_width: None,
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
        let png = render_single_label(
            &template,
            &data,
            Some(&selection),
            &no_settings(),
            &no_datetime(),
        )
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
                media_width: None,
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
        let png = render_single_label(
            &template,
            &data,
            Some(&selection),
            &no_settings(),
            &no_datetime(),
        )
        .expect("render label with qr");

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

        let pdf = render_sheet_pages(&template, &labels, 0, &no_settings(), &no_datetime())
            .expect("render sheet");

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
                media_width: None,
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
        let png = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect("render image");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_image_missing_data_errors() {
        let template = image_single_template();
        let data = HashMap::new();
        assert!(
            render_single_label(&template, &data, None, &no_settings(), &no_datetime()).is_err()
        );
    }

    #[test]
    fn render_image_invalid_base64_errors() {
        let template = image_single_template();
        let data = HashMap::from([(
            "logo".to_string(),
            json!("data:image/png;base64,@@@not-base64@@@"),
        )]);
        assert!(
            render_single_label(&template, &data, None, &no_settings(), &no_datetime()).is_err()
        );
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
        let pdf = render_sheet_pages(&template, &labels, 0, &no_settings(), &no_datetime())
            .expect("render sheet image");
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
        let png = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect("render svg");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn render_static_image_src() {
        use base64::Engine as _;
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cfg = std::env::temp_dir().join(format!("labeler_render_cfg_{n}"));
        let assets_dir = cfg.join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(PNG_1X1_B64)
            .unwrap();
        std::fs::write(assets_dir.join("logo.png"), &bytes).unwrap();
        std::env::set_var("LABELER_CONFIG_DIR", &cfg);

        let data = HashMap::new();
        let png = render_single_label(
            &image_single_template_with_src("logo.png"),
            &data,
            None,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render static src");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // A missing asset is rejected at render time.
        assert!(render_single_label(
            &image_single_template_with_src("missing.png"),
            &data,
            None,
            &no_settings(),
            &no_datetime(),
        )
        .is_err());

        std::env::remove_var("LABELER_CONFIG_DIR");
        std::fs::remove_dir_all(&cfg).ok();
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
                media_width: None,
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
        let pdf = render_single_label_pdf(&template, &data, None, &no_settings(), &no_datetime())
            .expect("render pdf");
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
        for id in [
            "brother_12mm",
            "brother_18mm",
            "brother_18mm_qr",
            "brother_24mm",
            "brother_24mm_qr",
        ] {
            let template = registry.get(id).unwrap_or_else(|| panic!("template {id}"));
            let png = render_single_label(template, &data, None, &no_settings(), &no_datetime())
                .expect("render tape");
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        }
    }

    #[test]
    fn render_value_text_and_qr_interpolate() {
        let template = TemplateDefinition {
            id: "interp".to_string(),
            name: "Interp".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: None,
                    value: Some("Item {id}".to_string()),
                    placement: Placement {
                        at: Position([0.0, 10.0]),
                        size: Size([SizeValue::Value(40.0), SizeValue::Value(8.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    font_size: FontSize::Fixed(8.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: None,
                    value: Some("{vars.qr_base_url}/{id}".to_string()),
                    placement: Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    params: None,
                },
            ]),
            version: None,
        };
        let data = HashMap::from([("id".to_string(), json!("A1"))]);
        let settings = BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())]);
        let png = render_single_label(&template, &data, None, &settings, &no_datetime())
            .expect("render interp");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // Missing setting is an error.
        assert!(
            render_single_label(&template, &data, None, &no_settings(), &no_datetime()).is_err()
        );
    }

    #[test]
    fn interpolated_data_cannot_inject_typst() {
        let template = TemplateDefinition {
            id: "inject".to_string(),
            name: "Inject".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(60.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("{x}".to_string()),
                placement: Placement {
                    at: Position([0.0, 6.0]),
                    size: Size([SizeValue::Value(60.0), SizeValue::Value(8.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(8.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        // Typst-hostile payload: markup that would call into the system if not escaped.
        let data = HashMap::from([("x".to_string(), json!(r#""]#sys.version[ \ end"#))]);
        let png = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect("render escaped");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn homebox_qr_template_renders() {
        let registry =
            crate::templates::TemplateRegistry::load_from_dir("templates").expect("load templates");
        let template = registry.get("homebox-qr").expect("template homebox-qr");
        let data = HashMap::from([
            ("id".to_string(), json!("A1")),
            ("message".to_string(), json!("Widget")),
        ]);
        let settings = BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())]);
        let dt_formats = crate::settings::default_datetime_formats();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now: chrono::Local::now(),
        };
        let png =
            render_single_label(template, &data, None, &settings, &dt).expect("render homebox-qr");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // Missing qr_base_url setting is an error.
        assert!(render_single_label(template, &data, None, &no_settings(), &dt).is_err());
    }

    #[test]
    fn render_thumbnail_of_sheet_is_label_sized() {
        let template = sheet_template_10x5_on_100x100();
        let data = HashMap::new();
        let settings = BTreeMap::new();
        let png =
            render_thumbnail_png(&template, &data, None, &settings, &no_datetime()).expect("png");
        let img = image::load_from_memory(&png).expect("decode png");
        // label 10x5 mm at 96 dpi ≈ 37.8 x 18.9 px; paper would be ~378 px. Assert it is the label box.
        assert!(
            img.width() > 20 && img.width() < 60,
            "width {} should be ~38px (label 10mm@96dpi), not paper-sized",
            img.width()
        );
        assert!(
            img.height() > 10 && img.height() < 30,
            "height {} should be ~19px (label 5mm@96dpi), not paper-sized",
            img.height()
        );
    }

    fn sheet_template_10x5_on_100x100() -> TemplateDefinition {
        use crate::models::{Alignment, FontSize, Position, SheetPosition, Size, SizeValue};
        TemplateDefinition {
            id: "s".into(),
            name: "s".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Sheet {
                paper_width: 100.0,
                paper_height: 100.0,
                label_width: 10.0,
                label_height: 5.0,
                positions: vec![SheetPosition([0.0, 0.0])],
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("hi".into()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        }
    }

    #[test]
    fn placeholder_data_fills_fields_excludes_vars_and_marks_images() {
        use crate::models::{Alignment, Fit, FontSize, Position, Size, SizeValue};
        let template = TemplateDefinition {
            id: "t".into(),
            name: "t".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(40.0),
                height: crate::models::Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: Some("title".into()),
                    value: None,
                    placement: Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    font_size: FontSize::Fixed(6.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: None,
                    value: Some("{url} {vars.base} {datetime} {datetime.short_date}".into()),
                    placement: Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(5.0), SizeValue::Value(5.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    params: None,
                },
                LayoutItem::Image {
                    name: Some("logo".into()),
                    src: None,
                    placement: Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(5.0), SizeValue::Value(5.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    fit: Fit::default(),
                },
            ]),
            version: None,
        };
        let data = placeholder_data(&template);
        assert_eq!(data.get("title").and_then(|v| v.as_str()), Some("title"));
        assert_eq!(data.get("url").and_then(|v| v.as_str()), Some("url"));
        assert!(!data.contains_key("base"), "vars.* must be excluded");
        assert!(!data.contains_key("vars.base"), "vars.* must be excluded");
        assert!(
            !data.contains_key("datetime"),
            "datetime namespace must be excluded"
        );
        assert!(
            !data.contains_key("datetime.short_date"),
            "datetime namespace must be excluded"
        );
        assert!(
            !data.contains_key("short_date"),
            "datetime namespace must be excluded"
        );
        assert_eq!(
            data.get("logo").and_then(|v| v.as_str()),
            Some(SAMPLE_PNG_DATA_URI)
        );
    }

    #[test]
    fn placeholder_data_skips_empty_token() {
        use crate::models::{Alignment, FontSize, Position, Size, SizeValue};
        let template = TemplateDefinition {
            id: "t".into(),
            name: "t".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(40.0),
                height: crate::models::Dimension::Fixed(20.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some("{} {real}".into()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(40.0), SizeValue::Value(20.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        };
        let data = placeholder_data(&template);
        assert!(
            !data.contains_key(""),
            "empty token must not produce an empty-string key"
        );
        assert_eq!(
            data.get("real").and_then(|v| v.as_str()),
            Some("real"),
            "real token must be collected"
        );
    }

    #[test]
    fn interpolate_datetime_tokens() {
        use crate::datetime_fmt::DateTimeResolver;
        use chrono::TimeZone;
        use std::collections::{BTreeMap, HashMap};

        let now = chrono::Local
            .with_ymd_and_hms(2026, 6, 25, 14, 30, 0)
            .single()
            .unwrap();
        let formats = BTreeMap::from([("short_date".to_string(), "%m/%d/%Y".to_string())]);
        let dt = DateTimeResolver {
            formats: &formats,
            now,
        };
        let vars = BTreeMap::new();
        // bare datetime => ISO date
        let mut data: HashMap<String, serde_json::Value> = HashMap::new();
        assert_eq!(
            super::helpers::interpolate("d={datetime}", &data, &vars, &dt).unwrap(),
            "d=2026-06-25"
        );
        // named format
        assert_eq!(
            super::helpers::interpolate("{datetime.short_date}", &data, &vars, &dt).unwrap(),
            "06/25/2026"
        );
        // unknown named format => error
        assert!(super::helpers::interpolate("{datetime.nope}", &data, &vars, &dt).is_err());
        // a data field named `datetime` is shadowed by the token
        data.insert("datetime".to_string(), serde_json::json!("SHADOWED"));
        assert_eq!(
            super::helpers::interpolate("{datetime}", &data, &vars, &dt).unwrap(),
            "2026-06-25"
        );
        // literal braces unaffected
        assert_eq!(
            super::helpers::interpolate("{{datetime}}", &data, &vars, &dt).unwrap(),
            "{datetime}"
        );
    }

    #[test]
    fn default_option_selection_picks_first_values() {
        use crate::models::{Dimension, Options};
        let template = TemplateDefinition {
            id: "t".into(),
            name: "t".into(),
            description: String::new(),
            unit: "mm".into(),
            dpi: 96,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0),
                height: Dimension::Fixed(20.0),
                media_width: None,
            },
            options: Some(Options(BTreeMap::from([
                (
                    "color".to_string(),
                    vec!["red".to_string(), "blue".to_string()],
                ),
                ("size".to_string(), vec!["small".to_string()]),
            ]))),
            layout: Layout::Items(vec![]),
            version: None,
        };
        let sel = default_option_selection(&template).expect("has options");
        assert_eq!(sel.get("color").map(String::as_str), Some("red"));
        assert_eq!(sel.get("size").map(String::as_str), Some("small"));

        let no_opts = TemplateDefinition {
            options: None,
            ..template
        };
        assert!(default_option_selection(&no_opts).is_none());
    }
}
