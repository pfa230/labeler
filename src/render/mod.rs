mod helpers;

pub const MAX_RENDER_DPI: u32 = 1200;

use crate::errors::AppError;
use crate::models::{
    resolve_coord, Dimension, Extent, Fit, FontSize, LabelInput, Layout, LayoutItem, Placement,
    Point, Position, Rotation, SizeValue, TemplateFormat,
};
use crate::templates::TemplateDefinition;
use helpers::{
    assets_root, binarize_rgba, build_qr_svg, escape_typst_string, fit_text_auto_length,
    fit_text_to_box, format_length, interpolate, parse_image_data_uri, resolve_dimension,
    resolve_image_asset, to_nonbreaking, to_page_coords, typst_alignment, typst_font_options,
    value_to_string, MeasuredText,
};
use serde_json::Value as JsonValue;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

/// Typst 0.15's `typst_render::render` takes `&RenderOptions` instead of a bare pixels-per-point
/// scalar; build one carrying the requested scale (bleed off, matching the previous behavior).
/// Wrap `body` in a `#pad` at the aligned edge. Typst's `#pad` grows the frame and translates the
/// child inward, so aligning the padded block insets the content by exactly `pad` — which is how ink
/// falling outside the cap-height/baseline line box (accents above, descenders below) stays inside
/// the clipped slot (#124). Center pads nothing: it already splits the slack, and reserving both
/// sides would cost a full em and shrink the bundled tape templates (ADR-0050).
fn pad_block(body: &str, pad: f32, vertical: crate::models::VerticalAlign) -> String {
    use crate::models::VerticalAlign;
    if pad <= 0.0 {
        return body.to_string();
    }
    match vertical {
        VerticalAlign::Top => format!("#pad(top: {pad:.2}pt)[{body}]"),
        VerticalAlign::Bottom => format!("#pad(bottom: {pad:.2}pt)[{body}]"),
        VerticalAlign::Center => body.to_string(),
    }
}

fn render_options(pixels_per_point: f32) -> typst_render::RenderOptions {
    typst_render::RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(pixels_per_point as f64),
        ..Default::default()
    }
}

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
                LengthMode::Fixed,
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

    let is_dynamic = matches!(
        &template.format,
        TemplateFormat::Single {
            width: Dimension::Dynamic { .. },
            ..
        }
    );
    let mode = if is_dynamic {
        LengthMode::Dynamic(AutoLength {
            texts: &measured,
            cursor: &cursor_cell,
        })
    } else {
        LengthMode::Fixed
    };

    let mut source = String::new();
    let page_width = format_length(width_units, unit)?;
    let page_height = format_length(height_units, unit)?;
    writeln!(
        source,
        "#set page(width: {page_width}, height: {page_height}, margin: 0{unit})"
    )
    .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
    writeln!(source, "#set text(font: \"Inter\")")
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;

    let context = RenderContext::new(
        (width_units, height_units),
        unit,
        data,
        selected_option,
        env,
        &images,
        mode,
    );
    source.push_str(&context.render_items(items)?);
    // Assert we consumed exactly the texts we measured.
    if cursor_cell.get() != measured.len() {
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
        .pages()
        .first()
        .ok_or_else(|| AppError::render_failed("typst did not produce any pages"))?;
    let pixmap = typst_render::render(page, &render_options(template.dpi as f32 / 72.0));
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
        .pages()
        .first()
        .ok_or_else(|| AppError::render_failed("typst did not produce any pages"))?;

    let dpi = opts.resolution_dpi.unwrap_or(template.dpi);
    let mut pixmap = typst_render::render(page, &render_options(dpi as f32 / 72.0));
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
            LengthMode::Fixed,
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
            writeln!(source, "#set text(font: \"Inter\")").map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
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
    // typst-pdf 0.15 serializes dictionary keys without whitespace (`/Type/Page`, `/Type/Pages`).
    let needle = b"/Type/Page";
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
struct AutoLength<'a> {
    texts: &'a [MeasuredText],
    cursor: &'a Cell<usize>,
}

/// Whether this frame is on a dynamic-width (auto-length) label. `Dynamic` carries the measured
/// texts, which may legitimately be empty: a label can be sized by lines or containers alone.
/// This is a property of the template format and must never be inferred from `texts.is_empty()`.
enum LengthMode<'a> {
    Fixed,
    Dynamic(AutoLength<'a>),
}

/// Render-time environment: the variables map and the datetime resolver, passed together through
/// every render call so related configuration travels as a unit.
struct RenderEnv<'a> {
    settings: &'a BTreeMap<String, String>,
    datetime: &'a crate::datetime_fmt::DateTimeResolver<'a>,
}

/// The anchor of a leaf box item, for `measure`'s clause 1 (a right-anchored item cannot define
/// the width it is anchored to, so it is skipped entirely). `Container` is excluded on purpose: a
/// container's own position can be right-anchored while its *inner frame* is not (its width is
/// still known, since `validate_placement_position` forbids pairing an edge-relative `at.x` with a
/// frame-dependent width on a dynamic-width template), so its children must still be measured.
/// `measure` gives `Container` its own clause-1 branch instead of using this. `Line` has two
/// endpoints and no box, so it is handled separately too.
fn item_anchor(item: &LayoutItem) -> Option<&Position> {
    match item {
        LayoutItem::Text { placement, .. }
        | LayoutItem::Qr { placement, .. }
        | LayoutItem::Image { placement, .. } => Some(&placement.at),
        LayoutItem::Line { .. } | LayoutItem::Container { .. } => None,
    }
}

/// `size = to - at`, both corners resolved against the frame first: `to` is the top-right corner of
/// a box whose bottom-left is `at`. Mirrors `templates::resolve_to_extent` (kept in sync per this
/// file's compile-time/render-time note) with one deliberate difference: a *zero* extent is legal
/// here and rejected there. On a dynamic-width label an empty data value can measure to exactly
/// `at.x`, so the label clamps to it and a `to`-spanning box collapses to zero width. That is an
/// ordinary outcome of blank input, not an authoring mistake; a *negative* extent still is one, and
/// the compile-time check (which resolves against the max-width frame) still rejects corners that
/// are statically inverted or degenerate.
fn resolve_to_extent(
    at: &Position,
    to: &Position,
    frame_w: f32,
    frame_h: f32,
) -> Result<(f32, f32), AppError> {
    const EPS: f32 = 1.0e-4;
    let width = resolve_coord(to.x(), frame_w) - resolve_coord(at.x(), frame_w);
    let height = resolve_coord(to.y(), frame_h) - resolve_coord(at.y(), frame_h);
    if width < -EPS || height < -EPS {
        return Err(AppError::unsupported_layout_item(
            "to must be above and to the right of at",
        ));
    }
    Ok((width.max(0.0), height.max(0.0)))
}

struct RenderContext<'a> {
    frame_width_units: f32,
    frame_height_units: f32,
    unit: &'a str,
    data: &'a HashMap<String, JsonValue>,
    selected_option: Option<&'a BTreeMap<String, String>>,
    env: &'a RenderEnv<'a>,
    images: &'a RefCell<ImageCollector>,
    mode: LengthMode<'a>,
}

impl<'a> RenderContext<'a> {
    fn new(
        frame: (f32, f32),
        unit: &'a str,
        data: &'a HashMap<String, JsonValue>,
        selected_option: Option<&'a BTreeMap<String, String>>,
        env: &'a RenderEnv<'a>,
        images: &'a RefCell<ImageCollector>,
        mode: LengthMode<'a>,
    ) -> Self {
        Self {
            frame_width_units: frame.0,
            frame_height_units: frame.1,
            unit,
            data,
            selected_option,
            env,
            images,
            mode,
        }
    }

    fn is_dynamic_width(&self) -> bool {
        matches!(self.mode, LengthMode::Dynamic(_))
    }

    fn auto_length(&self) -> Option<&AutoLength<'a>> {
        match &self.mode {
            LengthMode::Dynamic(al) => Some(al),
            LengthMode::Fixed => None,
        }
    }

    /// Resolve a template position against this frame, edge-relative components included. Errors if
    /// either axis resolves below zero: compile time cannot always prove this on a dynamic-width
    /// label, since an edge-relative inset is only checked against `width.max` at load time.
    fn resolve_point(&self, p: &Position) -> Result<Point, AppError> {
        const EPS: f32 = 1.0e-4;
        let x = resolve_coord(p.x(), self.frame_width_units);
        let y = resolve_coord(p.y(), self.frame_height_units);
        if x < -EPS || y < -EPS {
            return Err(AppError::unsupported_layout_item(format!(
                "a coordinate resolves outside the frame: [{}, {}] against {}x{}",
                p.x(),
                p.y(),
                self.frame_width_units,
                self.frame_height_units
            )));
        }
        Ok(Point { x, y })
    }

    /// Mirrors `templates::validate_bounds` for the cases compile time had to defer.
    fn check_box_bounds(&self, point: &Point, width: f32, height: f32) -> Result<(), AppError> {
        const EPS: f32 = 1.0e-4;
        if point.x + width > self.frame_width_units + EPS
            || point.y + height > self.frame_height_units + EPS
        {
            return Err(AppError::unsupported_layout_item(format!(
                "an item resolves outside the frame: {width}x{height} at [{}, {}] in {}x{}",
                point.x, point.y, self.frame_width_units, self.frame_height_units
            )));
        }
        Ok(())
    }

    /// The line checks compile time had to defer: on a dynamic-width label an edge-relative
    /// endpoint is resolved against `max`, so neither its upper bound nor its degeneracy against a
    /// plain endpoint could be decided until the final width was known.
    fn check_line(&self, start: &Point, end: &Point) -> Result<(), AppError> {
        const EPS: f32 = 1.0e-4;
        for p in [start, end] {
            if p.x > self.frame_width_units + EPS || p.y > self.frame_height_units + EPS {
                return Err(AppError::unsupported_layout_item(format!(
                    "a line endpoint resolves outside the frame: [{}, {}] in {}x{}",
                    p.x, p.y, self.frame_width_units, self.frame_height_units
                )));
            }
        }
        if (start.x - end.x).abs() < EPS && (start.y - end.y).abs() < EPS {
            return Err(AppError::unsupported_layout_item(
                "line start and end must differ after resolution",
            ));
        }
        Ok(())
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
            // Clause 1: a right-anchored item cannot define the width it is anchored to. Its inset
            // is the narrowest label it fits on, and that is all it can say. Skipping the item here
            // also means it pushes no MeasuredText, which `render_text_item` must mirror exactly.
            if let Some(at) = item_anchor(item) {
                if at.x().is_sign_negative() {
                    extent = extent.max(-at.x());
                    continue;
                }
            }
            // Clause 1, container case: unlike a leaf item, a right-anchored container's *inner
            // frame* is not itself right-anchored. Its own width is always known here (see
            // `item_anchor`'s doc comment), so its children's fits depend only on that known
            // width, not on where the container ends up sitting. They must still be measured, or
            // `render_container_item` (which recurses into every container unconditionally, with
            // no clause-1 skip of its own) will consume `MeasuredText` entries this pass never
            // pushed and fail with an auto-length cursor mismatch.
            if let LayoutItem::Container {
                placement,
                option,
                padding,
                items: children,
                ..
            } = item
            {
                if placement.at.x().is_sign_negative() {
                    if let Some(opt) = option {
                        if let Some(sel) = self.selected_option {
                            if !opt.iter().all(|(n, v)| sel.get(n) == Some(v)) {
                                continue;
                            }
                        }
                    }
                    let at_y = resolve_coord(placement.at.y(), self.frame_height_units);
                    self.measure_container_footprint(placement, at_y, padding, children, out)?;
                    extent = extent.max(-placement.at.x());
                    continue;
                }
            }
            let right = match item {
                LayoutItem::Text {
                    name,
                    value,
                    placement,
                    font_size,
                    font_weight,
                    multiline,
                    alignment,
                    ..
                } => {
                    let text = self.resolve_item_text("text", name.as_deref(), value.as_deref())?;
                    // Same weight the render pass will use: this pre-pass decides the auto width, so
                    // measuring it unweighted would size the box for text that renders wider.
                    let weight = font_weight.unwrap_or(400);
                    // `at.y` may itself be edge-relative (measured from the top), so it must be
                    // resolved before any height arithmetic. `at.x` is already known to be
                    // non-negative here: clause 1 skipped the item otherwise.
                    let at = Point {
                        x: placement.at.x(),
                        y: resolve_coord(placement.at.y(), self.frame_height_units),
                    };
                    if placement.width_is_frame_dependent() {
                        // The right margin an edge-relative `to` asks for. Subtracting it from the
                        // budget keeps this item's contribution (at.x + width + inset) inside
                        // `budget_w`, so the clamp can never hand back a box narrower than the
                        // width the text was wrapped at.
                        let inset = match &placement.extent {
                            Extent::To(to) if to.x().is_sign_negative() => -to.x(),
                            _ => 0.0,
                        };
                        // The cap binds here, not at render: the rendered box for this item is
                        // exactly `m.width`, so capping the budget is what caps the width.
                        let budget = (budget_w - at.x - inset)
                            .min(placement.max_w.unwrap_or(f32::INFINITY))
                            .max(0.0);
                        let box_h = self.measure_box_height(placement, at.y)?;
                        let m = fit_text_auto_length(
                            &text,
                            font_size,
                            *multiline,
                            weight,
                            alignment.vertical,
                            helpers::FitBox {
                                width_units: budget,
                                height_units: box_h,
                                unit: self.unit,
                            },
                        )?;
                        let w = m.width;
                        out.push(m);
                        at.x + w + inset
                    } else {
                        // A numeric size or a numeric `to`: the width is known, so this is the
                        // ordinary fixed-width case and no MeasuredText is recorded.
                        let (w, _) = self.resolve_size(
                            &placement.at,
                            &placement.extent,
                            placement.max_w,
                            placement.max_h,
                            false,
                        )?;
                        at.x + w
                    }
                }
                LayoutItem::Qr { placement, .. } | LayoutItem::Image { placement, .. } => {
                    let at_x = placement.at.x();
                    match &placement.extent {
                        // `auto` fills the remaining budget, capped by `max_w`. The render side
                        // already resolves this through `resolve_size(.., allow_auto_fill: false)`,
                        // which honors `max_w` exactly, so without the cap here the label was
                        // sized for a code that renders far narrower.
                        Extent::Size(size) => {
                            at_x + size.0[0].value().unwrap_or(
                                (budget_w - at_x)
                                    .min(placement.max_w.unwrap_or(f32::INFINITY))
                                    .max(0.0),
                            )
                        }
                        // A numeric `to` is a known width; a frame-dependent one contributes
                        // nothing, since a qr or image has no measured intrinsic width to offer.
                        Extent::To(_) => {
                            if placement.width_is_frame_dependent() {
                                0.0
                            } else {
                                at_x + self
                                    .resolve_size(
                                        &placement.at,
                                        &placement.extent,
                                        placement.max_w,
                                        placement.max_h,
                                        false,
                                    )?
                                    .0
                            }
                        }
                    }
                }
                // An edge-relative endpoint cannot contribute a frame-dependent term (the frame
                // width is the unknown being solved for), but it does contribute its inset: the
                // narrowest label the endpoint fits on, exactly as clause 1 does for a
                // right-anchored box.
                LayoutItem::Line { at, to, .. } => [at.x(), to.x()]
                    .into_iter()
                    .map(|x| if x.is_sign_negative() { -x } else { x })
                    .fold(0.0f32, f32::max),
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
                    let at_x = placement.at.x();
                    let at_y = resolve_coord(placement.at.y(), self.frame_height_units);
                    if placement.width_is_frame_dependent() {
                        // Width comes from the children. Any right-edge inset is theirs to pay
                        // for, exactly as for text, so it comes out of their budget and goes back
                        // into the contribution.
                        let inset = match &placement.extent {
                            Extent::To(to) if to.x().is_sign_negative() => -to.x(),
                            _ => 0.0,
                        };
                        let outer_cap = placement.max_w.unwrap_or(f32::INFINITY);
                        let outer_budget = (budget_w - at_x - inset).min(outer_cap).max(0.0);
                        let inner_budget = (outer_budget - padding.left - padding.right).max(0.0);
                        let inner_h = (self.measure_box_height(placement, at_y)?
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
                            LengthMode::Fixed,
                        );
                        let child_extent = ctx.measure(items, inner_budget, out)?;
                        // Capping only the budget would still let padding push the contribution
                        // past the cap.
                        let outer = (padding.left + child_extent + padding.right).min(outer_budget);
                        at_x + outer + inset
                    } else {
                        // A numeric size or a numeric `to`: the footprint is known.
                        let (explicit_w, _) =
                            self.measure_container_footprint(placement, at_y, padding, items, out)?;
                        at_x + explicit_w
                    }
                }
            };
            extent = extent.max(right);
        }
        Ok(extent)
    }

    /// A box item's vertical slot during measurement: its explicit height, the box its corners
    /// describe, or the rest of the frame above `at_y`. `at_y` is the **resolved** bottom edge,
    /// never the raw value: mixing a resolved `to.y` with a raw `at.y` inflates the slot.
    fn measure_box_height(&self, placement: &Placement, at_y: f32) -> Result<f32, AppError> {
        Ok(match &placement.extent {
            // The same call `render_text_item` makes for this slot, so the two cannot disagree
            // (#150). Only the height axis goes through the helper: `resolve_size` would also
            // resolve the width and error on a `size: [40, auto]` container, which must keep
            // measuring.
            Extent::Size(size) => self.resolve_size_value(
                &size.0[1],
                placement.max_h,
                Some(self.frame_height_units - at_y),
                "height",
            )?,
            Extent::To(to) => resolve_coord(to.y(), self.frame_height_units) - at_y,
        })
    }

    /// A container's own footprint when its extent is *not* frame-dependent (a numeric `size` or
    /// a numeric `to`), plus its children measured against the resulting inner frame. Width comes
    /// from `resolve_size` (a numeric `to` needs its full corner-resolution logic; a numeric
    /// `size` is read directly, since `resolve_size` would also resolve height and this is the
    /// one call site that must not: see the height note below). Height comes from
    /// `measure_box_height`, not `resolve_size`, so an auto height with no `max_h` still falls
    /// back to the remaining frame height above `at_y` rather than erroring: `size: [40, auto]` is
    /// a documented container idiom (SPEC §4, "auto size ... falls back to the parent frame") and
    /// must keep working under measurement.
    ///
    /// Shared by the `Container` arm's non-frame-dependent branch and clause 1's container case (a
    /// right-anchored container whose own width is still known; see `item_anchor`'s doc comment),
    /// so the two can never compute a container's footprint two different ways.
    fn measure_container_footprint(
        &self,
        placement: &Placement,
        at_y: f32,
        padding: &crate::models::Padding,
        children: &[LayoutItem],
        out: &mut Vec<MeasuredText>,
    ) -> Result<(f32, f32), AppError> {
        let explicit_w = match &placement.extent {
            Extent::Size(size) => {
                self.resolve_size_value(&size.0[0], placement.max_w, None, "width")?
            }
            Extent::To(_) => {
                self.resolve_size(
                    &placement.at,
                    &placement.extent,
                    placement.max_w,
                    placement.max_h,
                    false,
                )?
                .0
            }
        };
        let explicit_h = self.measure_box_height(placement, at_y)?;
        let rotation = placement
            .rotate
            .and_then(Rotation::from_degrees)
            .unwrap_or(Rotation::R0);
        // Rotated containers are self-contained (§4.2.1): their author-space children must not be
        // measured in physical-horizontal terms, so do not recurse into them (#98).
        if !rotation.is_rotated() {
            let inner_w = (explicit_w - padding.left - padding.right).max(0.0);
            let inner_h = (explicit_h - padding.top - padding.bottom).max(0.0);
            let ctx = RenderContext::new(
                (inner_w, inner_h),
                self.unit,
                self.data,
                self.selected_option,
                self.env,
                self.images,
                LengthMode::Fixed,
            );
            ctx.measure(children, inner_w, out)?;
        }
        Ok((explicit_w, explicit_h))
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
                    font_weight,
                    multiline,
                    alignment,
                } => {
                    let text = self.resolve_item_text("text", name.as_deref(), value.as_deref())?;
                    self.render_text_item(
                        &mut out,
                        text,
                        placement,
                        font_size,
                        *font_weight,
                        *multiline,
                        alignment,
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

    #[allow(clippy::too_many_arguments)]
    fn render_text_item(
        &self,
        out: &mut String,
        raw_text: String,
        placement: &Placement,
        font_size: &FontSize,
        font_weight: Option<u16>,
        multiline: bool,
        alignment: &crate::models::Alignment,
    ) -> Result<(), AppError> {
        // Absent stays absent rather than emitting 400: an explicit weight would rewrite every
        // existing template's generated source for no behavioral reason.
        let weight_arg = font_weight
            .map(|w| format!(", weight: {w}"))
            .unwrap_or_default();
        let weight = font_weight.unwrap_or(400);
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

        let point = self.resolve_point(&placement.at)?;
        let left = point.x;

        // A blank first/last line carries no ink but still gets a line box, which shoves the visible
        // text off centre. Drop them at emission (#127); interior blanks are real spacing and stay.
        fn trim_blank_edges(lines: &[String]) -> Vec<String> {
            let start = lines.iter().position(|l| !l.trim().is_empty());
            let end = lines.iter().rposition(|l| !l.trim().is_empty());
            match (start, end) {
                (Some(s), Some(e)) => lines[s..=e].to_vec(),
                _ => Vec::new(),
            }
        }

        // When auto-length is active and this text item's width is frame-dependent (an auto size or
        // an edge-relative `to`), consume the next measured fit. This must fire for exactly the
        // items `measure` pushed a `MeasuredText` for; both call `width_is_frame_dependent` on the
        // same placement so they never disagree.
        if let Some(al) = self.auto_length() {
            if placement.width_is_frame_dependent() && !placement.at.x().is_sign_negative() {
                let idx = al.cursor.get();
                let m = al.texts.get(idx).ok_or_else(|| {
                    AppError::render_failed(format!("auto-length cursor overrun at index {idx}"))
                })?;
                al.cursor.set(idx + 1);

                // The text's allotted vertical slot: `size` height (honoring `max_h`) for an auto
                // width, or the box `to`'s corners describe for an edge-relative `to`.
                let slot_h = match &placement.extent {
                    Extent::Size(size) => self.resolve_size_value(
                        &size.0[1],
                        placement.max_h,
                        Some(self.frame_height_units - point.y),
                        "height",
                    )?,
                    Extent::To(_) => self.measure_box_height(placement, point.y)?,
                };
                let slot_top = self.frame_height_units - point.y - slot_h;
                // The box width is the resolved extent for a `to` (so a right-edge inset stays a
                // visible margin), and the content's fitted width for an auto size, matching what
                // the measure pass pushed.
                let box_w = match &placement.extent {
                    Extent::To(_) => {
                        self.resolve_size(
                            &placement.at,
                            &placement.extent,
                            placement.max_w,
                            placement.max_h,
                            false,
                        )?
                        .0
                    }
                    Extent::Size(_) => m.width,
                };
                self.check_box_bounds(&point, box_w, slot_h)?;
                let body = trim_blank_edges(&m.lines)
                    .iter()
                    .map(|l| {
                        format!(
                            "#text(\"{}\", size: {}pt{weight_arg})",
                            escape_typst_string(l),
                            m.font
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("#linebreak()");
                // Vertical placement is Typst's job, not ours: its line box runs cap-height to
                // baseline, not the full fontdue line height, so any dy we compute from font
                // metrics lands the glyphs high (#123). Box the whole slot and let `#align` place
                // the block inside it, exactly as the fixed-size path below does.
                // #124: pad the aligned edge so the ink Typst's cap-height/baseline line box leaves
                // outside — accents above, descenders below — lands inside the clipped slot.
                let body = pad_block(
                    &body,
                    helpers::pad_pt(weight, m.font, alignment.vertical)?,
                    alignment.vertical,
                );
                let inner = format!("#align({})[{body}]", typst_alignment(alignment));
                let dx = format_length(left, self.unit)?;
                let dy = format_length(slot_top, self.unit)?;
                let box_width = format_length(box_w, self.unit)?;
                let box_height = format_length(slot_h, self.unit)?;
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

        let (width, box_height_units) = self.resolve_size(
            &placement.at,
            &placement.extent,
            placement.max_w,
            placement.max_h,
            false,
        )?;
        self.check_box_bounds(&point, width, box_height_units)?;
        let bottom = point.y;
        let top = bottom + box_height_units;
        let (size, text) = match font_size {
            FontSize::Fixed(size) => (*size, text),
            FontSize::Range { min, max } => fit_text_to_box(
                &text,
                multiline,
                weight,
                alignment.vertical,
                *min,
                *max,
                helpers::FitBox {
                    width_units: width,
                    height_units: box_height_units,
                    unit: self.unit,
                },
            )?,
        };
        let text =
            trim_blank_edges(&text.lines().map(str::to_string).collect::<Vec<_>>()).join("\n");
        let text = escape_typst_string(&text);
        let dx = format_length(left, self.unit)?;
        let dy = format_length(self.frame_height_units - top, self.unit)?;
        let box_width = format_length(width, self.unit)?;
        let box_height = format_length(box_height_units, self.unit)?;

        let align = typst_alignment(alignment);
        let body = format!("#text(\"{text}\", size: {size}pt{weight_arg})");
        let body = pad_block(
            &body,
            helpers::pad_pt(weight, size, alignment.vertical)?,
            alignment.vertical,
        );
        let content = format!("#align({align})[{body}]");
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
        let (width, height) = self.resolve_size(
            &placement.at,
            &placement.extent,
            placement.max_w,
            placement.max_h,
            false,
        )?;
        let point = self.resolve_point(&placement.at)?;
        self.check_box_bounds(&point, width, height)?;
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
        let (width, height) = self.resolve_size(
            &placement.at,
            &placement.extent,
            placement.max_w,
            placement.max_h,
            false,
        )?;
        let point = self.resolve_point(&placement.at)?;
        self.check_box_bounds(&point, width, height)?;
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
        let start_point = self.resolve_point(at)?;
        let end_point = self.resolve_point(to)?;
        self.check_line(&start_point, &end_point)?;
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
        let point = self.resolve_point(&placement.at)?;
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
            let width = if self.is_dynamic_width()
                && placement.size_or_auto().is_some_and(|s| s.0[0].is_auto())
            {
                // Deliberately an explicit `min` rather than `resolve_size_value` with the
                // narrowed remainder as its fallback: that helper rejects `<= 0`, and a zero
                // remainder here is a legitimate outcome of measurement rather than an authoring
                // error. A zero-width container renders an empty box, as it does today.
                (self.frame_width_units - left)
                    .min(placement.max_w.unwrap_or(f32::INFINITY))
                    .max(0.0)
            } else {
                self.resolve_size(
                    &placement.at,
                    &placement.extent,
                    placement.max_w,
                    placement.max_h,
                    true,
                )?
                .0
            };
            let height = self
                .resolve_size(
                    &placement.at,
                    &placement.extent,
                    placement.max_w,
                    placement.max_h,
                    true,
                )?
                .1;
            self.check_box_bounds(&point, width, height)?;
            let bottom = point.y;
            let top = bottom + height;

            let inner_width = (width - padding.left - padding.right).max(0.0);
            let inner_height = (height - padding.top - padding.bottom).max(0.0);
            let child_mode = match &self.mode {
                LengthMode::Dynamic(al) => LengthMode::Dynamic(AutoLength {
                    texts: al.texts,
                    cursor: al.cursor,
                }),
                LengthMode::Fixed => LengthMode::Fixed,
            };
            let context = RenderContext::new(
                (inner_width, inner_height),
                self.unit,
                self.data,
                self.selected_option,
                self.env,
                self.images,
                child_mode,
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
            .resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                true,
            )?
            .0;
        let height = self
            .resolve_size(
                &placement.at,
                &placement.extent,
                placement.max_w,
                placement.max_h,
                true,
            )?
            .1;
        self.check_box_bounds(&point, width, height)?;
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

        // No dynamic width under rotation (validation forbids auto descendants).
        let context = RenderContext::new(
            (content_w, content_h),
            self.unit,
            self.data,
            self.selected_option,
            self.env,
            self.images,
            LengthMode::Fixed,
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
        at: &Position,
        extent: &Extent,
        max_w: Option<f32>,
        max_h: Option<f32>,
        allow_auto_fill: bool,
    ) -> Result<(f32, f32), AppError> {
        let size = match extent {
            Extent::Size(size) => size,
            Extent::To(to) => {
                return resolve_to_extent(at, to, self.frame_width_units, self.frame_height_units);
            }
        };
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
                // `max_*` caps the resolution of `auto`; it does not replace the fallback. A cap
                // larger than the room available is simply not binding, so the smaller wins.
                let resolved = match (max, fallback) {
                    (Some(max), Some(fallback)) => max.min(fallback),
                    (Some(max), None) => max,
                    (None, Some(fallback)) => fallback,
                    (None, None) => {
                        return Err(AppError::unsupported_layout_item(format!(
                            "size {label} is auto but no max_{label} provided"
                        )))
                    }
                };
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

/// The request data keys a template needs, deduped and sorted.
///
/// Shares the walker behind `placeholder_data`, so it inherits the rule that `{vars.*}` and
/// `{datetime[.*]}` are NOT request fields — they resolve from the variables store and the datetime
/// resolver. The catalog index lists this so an entry advertises only what the caller must supply
/// (#137); `homebox-qr` would otherwise appear to demand `vars.qr_base_url`.
pub fn template_fields(template: &TemplateDefinition) -> Vec<String> {
    let Layout::Items(items) = &template.layout;
    let mut text = Vec::new();
    let mut image = Vec::new();
    walk_placeholder(items, &mut text, &mut image);
    let mut all: Vec<String> = text.into_iter().chain(image).collect();
    all.sort();
    all.dedup();
    all
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
        render_single_label, render_single_label_pdf, render_thumbnail_png, template_fields,
        SAMPLE_PNG_DATA_URI,
    };
    use crate::models::{
        Alignment, AutoSize, Dimension, Extent, Fit, FontSize, Frame, HorizontalAlign, LabelInput,
        Layout, LayoutItem, Options, Padding, Placement, Position, SheetPosition, Size, SizeValue,
        TemplateFormat, VerticalAlign,
    };
    use crate::templates::TemplateDefinition;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    /// Build a one-text-item source. `size_w` of `None` means an auto width, which routes through
    /// the auto-length path; `Some(w)` takes the fixed-size path. Both must carry the weight.
    fn text_source(
        weight: Option<u16>,
        size_w: Option<f32>,
        font_size: FontSize,
        text: &str,
    ) -> String {
        text_source_aligned(weight, size_w, font_size, text, VerticalAlign::Top)
    }

    fn text_source_aligned(
        weight: Option<u16>,
        size_w: Option<f32>,
        font_size: FontSize,
        text: &str,
        vertical: VerticalAlign,
    ) -> String {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let item = LayoutItem::Text {
            name: None,
            value: Some(text.to_string()),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([
                    match size_w {
                        Some(w) => SizeValue::Value(w),
                        None => SizeValue::Auto(crate::models::AutoSize::Auto),
                    },
                    SizeValue::Value(8.0),
                ]),
            ),
            font_size,
            font_weight: weight,
            multiline: false,
            alignment: crate::models::Alignment {
                horizontal: crate::models::HorizontalAlign::Left,
                vertical,
            },
        };
        // The auto-width path replays what the measure pre-pass recorded, so that pass has to run
        // first and its results have to be handed to the render context.
        let measuring = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let mut measured = Vec::new();
        measuring
            .measure(std::slice::from_ref(&item), 80.0, &mut measured)
            .expect("measure");
        let cursor = std::cell::Cell::new(0);
        let ctx = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Dynamic(super::AutoLength {
                texts: &measured,
                cursor: &cursor,
            }),
        );
        ctx.render_items(&[item]).expect("render text item")
    }

    fn fitted_pt(source: &str) -> f32 {
        let at = source.find("size: ").expect("a size in the source") + 6;
        let rest = &source[at..];
        let end = rest.find("pt").expect("pt suffix");
        rest[..end].parse().expect("a number")
    }

    /// #97 on the fixed-size path.
    #[test]
    fn font_weight_is_emitted_on_the_fixed_size_path() {
        let src = text_source(Some(700), Some(60.0), FontSize::Fixed(10.0), "Widget");
        assert!(src.contains("weight: 700"), "no weight in source: {src}");
    }

    /// The emitted pad is `pad_em × size` for the aligned edge — the *placement* constant. Not
    /// `overflow_em`: the fitter's reservation is twice this and never reaches the source (#124).
    #[test]
    fn the_emitted_pad_is_the_aligned_edge_metric() {
        // 0.2412em at 20pt = 4.82pt, and in Inter the top and bottom pads are the same 494 units —
        // a coincidence of this font, not a shared constant.
        let bottom = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "gjpqy",
            VerticalAlign::Bottom,
        );
        assert!(
            bottom.contains("#pad(bottom: 4.82"),
            "unexpected source: {bottom}"
        );
        let top = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "Édgy",
            VerticalAlign::Top,
        );
        assert!(top.contains("#pad(top: 4.82"), "unexpected source: {top}");
        let centered = text_source_aligned(
            None,
            Some(60.0),
            FontSize::Fixed(20.0),
            "Hxy",
            VerticalAlign::Center,
        );
        assert!(
            !centered.contains("#pad"),
            "center must not pad: {centered}"
        );
    }

    /// #97 on the auto-length path. Wired separately from the fixed-size path, and a field carried
    /// by only one of the two is a failure this codebase has had before.
    #[test]
    fn font_weight_is_emitted_on_the_auto_length_path() {
        let src = text_source(Some(700), None, FontSize::Fixed(10.0), "Widget");
        assert!(src.contains("weight: 700"), "no weight in source: {src}");
    }

    /// Absent means absent: existing templates keep byte-identical source (spec, Decision 2).
    #[test]
    fn no_font_weight_emits_no_weight_argument() {
        let src = text_source(None, Some(60.0), FontSize::Fixed(10.0), "Widget");
        assert!(
            !src.contains("weight:"),
            "unexpected weight in source: {src}"
        );
    }

    /// The measure pre-pass is a third, separate consumer of the weight, and the source assertions
    /// cannot see it: it decides the auto width of a tape label before any text is emitted. If it
    /// measured unweighted, a bold label would be sized for narrower text than it renders (#96).
    #[test]
    fn the_measure_pre_pass_sizes_an_auto_width_item_for_its_weight() {
        fn measured_width(weight: Option<u16>) -> f32 {
            use std::cell::RefCell;
            let data: HashMap<String, super::JsonValue> = HashMap::new();
            let settings = no_settings();
            let datetime = no_datetime();
            let env = super::RenderEnv {
                settings: &settings,
                datetime: &datetime,
            };
            let images = RefCell::new(super::ImageCollector::default());
            let ctx = super::RenderContext::new(
                (80.0, 40.0),
                "mm",
                &data,
                None,
                &env,
                &images,
                super::LengthMode::Fixed,
            );
            let item = LayoutItem::Text {
                name: None,
                value: Some("Widget A-42 Storage".to_string()),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(8.0),
                    ]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: weight,
                multiline: false,
                alignment: crate::models::Alignment::default(),
            };
            let mut measured = Vec::new();
            ctx.measure(&[item], 200.0, &mut measured).expect("measure");
            measured[0].width
        }

        let regular = measured_width(None);
        let bold = measured_width(Some(900));
        assert!(
            bold > regular,
            "the pre-pass measured {bold} for weight 900 and {regular} unweighted: it ignored the weight"
        );
    }

    /// The renderer emitting `weight:` proves nothing about the *fitter* getting it: leaving 400
    /// wired into the fit calls would keep every other test green while #96 stayed unfixed. Bold is
    /// wider, so the same string in the same box must fit at a smaller size.
    #[test]
    fn a_bold_item_fits_at_a_smaller_size_than_an_unweighted_one() {
        let range = FontSize::Range {
            min: 6.0,
            max: 40.0,
        };
        let text = "Widget A-42 Storage Bin";
        let regular = fitted_pt(&text_source(None, Some(40.0), range.clone(), text));
        let bold = fitted_pt(&text_source(Some(900), Some(40.0), range, text));
        assert!(
            regular < 40.0,
            "the box must actually constrain the fit (got {regular}pt)"
        );
        assert!(
            bold < regular,
            "bold fitted at {bold}pt, regular at {regular}pt: the fitter ignored the weight"
        );
    }

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
        let ctx = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );

        let auto_text = LayoutItem::Text {
            name: None,
            value: Some("hello".to_string()),
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(10.0),
                ]),
            ),
            font_size: FontSize::Fixed(6.0),
            font_weight: None,
            multiline: false,
            alignment: Alignment::default(),
        };
        let make_container = |rotate: Option<f32>| LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: Extent::Size(Size([SizeValue::Value(80.0), SizeValue::Value(40.0)])),
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

    fn measured_extent_of(item: LayoutItem, budget: f32) -> (f32, usize) {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (budget, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let mut measured = Vec::new();
        let extent = ctx
            .measure(&[item], budget, &mut measured)
            .expect("measure");
        (extent, measured.len())
    }

    /// Builds a `RenderContext` over a dynamic-width frame, so `render_container_item` takes its
    /// auto-width branch. Empty `texts` is legitimate: the mode comes from the format, not from
    /// whether any text needed measuring.
    fn dynamic_ctx_source(frame_w: f32, item: LayoutItem) -> String {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let texts: Vec<super::MeasuredText> = Vec::new();
        let cursor = std::cell::Cell::new(0usize);
        let ctx = super::RenderContext::new(
            (frame_w, 12.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Dynamic(super::AutoLength {
                texts: &texts,
                cursor: &cursor,
            }),
        );
        ctx.render_items(&[item]).expect("render")
    }

    fn capped_container(at_x: f32, max_w: Option<f32>, items: Vec<LayoutItem>) -> LayoutItem {
        LayoutItem::Container {
            placement: Placement {
                at: Position([at_x, 0.0]),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(12.0),
                ])),
                max_w,
                max_h: None,
                rotate: None,
            },
            option: None,
            frame: None,
            padding: crate::models::Padding::ZERO,
            items,
        }
    }

    /// The render half of #152. The frame is 100mm wide and the container sits at x=90, so the
    /// remainder is 10mm and the 5mm cap is the binding constraint. Before the fix this branch
    /// ignores `max_w` entirely and emits the 10mm remainder.
    #[test]
    fn max_w_caps_a_dynamic_container_at_render() {
        let source = dynamic_ctx_source(100.0, capped_container(90.0, Some(5.0), vec![]));
        assert!(
            source.contains("width: 5mm"),
            "the container must render at its 5mm cap, not the 10mm frame remainder: {source}"
        );
    }

    /// The measure half of #152. The child is load-bearing: the cap only binds when the content
    /// would otherwise exceed it, so an *empty* container measures the same before and after and
    /// proves nothing. Uncapped this contributes at_x plus the child's full natural width; capped
    /// it contributes at_x plus the cap.
    #[test]
    fn max_w_caps_a_dynamic_container_during_measurement() {
        let child = LayoutItem::Text {
            name: None,
            value: Some("a string far wider than any five millimetre cap".to_string()),
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(8.0),
                ])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            multiline: false,
            alignment: crate::models::Alignment::default(),
        };
        let (uncapped, _) =
            measured_extent_of(capped_container(10.0, None, vec![child.clone()]), 100.0);
        let (capped, _) = measured_extent_of(capped_container(10.0, Some(5.0), vec![child]), 100.0);
        assert!(
            uncapped > 30.0,
            "the child must be wide enough for the cap to bind, got {uncapped}"
        );
        assert!(
            (capped - 15.0).abs() < 0.5,
            "a container at x=10 capped to 5mm contributes 15, not {capped}"
        );
    }

    /// #152's own repro template, asserted as *correctly* rejected. The load-time check was right
    /// all along; the renderer was the liar. Testing only the rejection would pass even against
    /// unfixed code, so this also pins that the container really does render at its cap, which is
    /// what makes a child line reaching x=50 genuinely not fit.
    #[test]
    fn the_152_repro_is_rejected_and_the_rejection_is_correct() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [0.0, 0.0]\n    size: [auto, 12.0]\n    max_w: 30.0\n    items:\n      - type: line\n        at: [0.0, 3.0]\n        to: [50.0, 3.0]\n        thickness: 0.2\n";
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml).expect("parses");
        let template = crate::templates::TemplateDefinition::try_from(raw).expect("converts");
        assert!(
            template.validate().is_err(),
            "a 50mm line inside a 30mm-capped container must be rejected"
        );
        // And the rejection is correct because the container really is 30mm at render.
        let source = dynamic_ctx_source(100.0, capped_container(0.0, Some(30.0), vec![]));
        assert!(
            source.contains("width: 30mm"),
            "the container renders at its cap, so the rejected line truly does not fit: {source}"
        );
    }

    /// A refactor guard only. This PASSES against unfixed code, because the branch is already
    /// `(frame_width - left).max(0.0)`. It exists to catch a later rewrite that routes this branch
    /// through `resolve_size_value`, which rejects `<= 0` and would break a legitimate zero
    /// remainder. It is NOT a guard for #152.
    #[test]
    fn a_zero_remainder_container_renders_an_empty_box() {
        let source = dynamic_ctx_source(90.0, capped_container(90.0, Some(30.0), vec![]));
        assert!(
            source.contains("width: 0mm"),
            "a container with no room left renders an empty box rather than erroring: {source}"
        );
    }

    /// Spec §4.1. The Task 1 loosening admits this at load and it then fails at render, because the
    /// container has no width left for a divider. Like the test above, this PASSES after Task 1 and
    /// before Task 5 — it is not a per-step regression guard. It is here to pin *how* such a
    /// template fails: the standard explained error, not a panic and not a corrupt page.
    #[test]
    fn a_container_with_no_room_left_fails_cleanly_at_render() {
        let yaml = "id: t\nname: T\nunit: mm\ndpi: 180\nformat:\n  type: single\n  width: { min: 10, max: 100 }\n  height: 12\nlayout:\n  - type: container\n    at: [90.0, 0.0]\n    size: [auto, 12.0]\n    max_w: 30.0\n    items:\n      - type: line\n        at: [0.0, 6.0]\n        to: [-0.0, 6.0]\n        thickness: 0.2\n";
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml).expect("parses");
        let template = crate::templates::TemplateDefinition::try_from(raw).expect("converts");
        assert_eq!(
            template.validate(),
            Ok(()),
            "the cap loosening admits this at load"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let err = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect_err("a divider in a zero-width container cannot render");
        assert!(
            err.message_text().contains("must differ"),
            "expected the standard degenerate-line error, got: {}",
            err.message_text()
        );
    }

    /// A cap below the container's own padding leaves no inner box at all. Before the cap this was
    /// unreachable (the container always got the whole frame remainder); with it, the inner
    /// dimensions must clamp at zero rather than going negative.
    ///
    /// What this test can and cannot assert, because it took four attempts to get right: a
    /// zero-width inner frame cannot host *any* auto-width child, since `render_container_item`
    /// computes height via `resolve_size(..).1`, which resolves the width axis first and rejects
    /// `<= 0`. So the child errors either way and there is no "renders successfully" green to
    /// reach. What the clamp changes is *which* error: without it the child's edge-relative `at.x`
    /// resolves against a negative frame and fails in `resolve_point` with a coordinate error;
    /// with it the frame is zero and the failure is the accurate size error. Asserting the error
    /// kind is the discriminating assertion available here.
    #[test]
    fn a_cap_smaller_than_the_padding_clamps_the_inner_box() {
        let item = LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(12.0),
                ])),
                max_w: Some(2.0),
                max_h: None,
                rotate: None,
            },
            option: None,
            frame: None,
            padding: crate::models::Padding {
                top: 3.0,
                right: 3.0,
                bottom: 3.0,
                left: 3.0,
            },
            // The child is load-bearing: unclamped inner dimensions are only ever *passed into*
            // the child context, so an empty container emits nothing and the bug stays invisible.
            // Its position is edge-relative so it resolves against the inner frame width.
            items: vec![LayoutItem::Container {
                placement: Placement {
                    at: Position([-0.0, 0.0]),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(1.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                option: None,
                frame: None,
                padding: crate::models::Padding::ZERO,
                items: vec![],
            }],
        };
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let texts: Vec<super::MeasuredText> = Vec::new();
        let cursor = std::cell::Cell::new(0usize);
        let ctx = super::RenderContext::new(
            (100.0, 12.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Dynamic(super::AutoLength {
                texts: &texts,
                cursor: &cursor,
            }),
        );
        let err = ctx
            .render_items(&[item])
            .expect_err("a zero-width inner frame cannot host an auto-width child");
        assert!(
            err.message_text().contains("must be greater than 0"),
            "expected the size error a clamped zero-width frame produces, not the negative \
             coordinate error an unclamped one produces; got: {}",
            err.message_text()
        );
    }

    /// The cap must be inert when no bound is set. One assertion per site the branch capped, so a
    /// leak names the site. These pass before and after this branch; they exist to stay green.
    #[test]
    fn no_max_w_means_no_cap_anywhere() {
        // Text: an uncapped auto-width text measures its natural width against the full budget.
        let long = "a string long enough to have a natural width worth measuring";
        let (text_extent, _) = measured_extent_of(
            LayoutItem::Text {
                name: None,
                value: Some(long.to_string()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(8.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                multiline: false,
                alignment: crate::models::Alignment::default(),
            },
            200.0,
        );
        assert!(text_extent > 0.0 && text_extent < 200.0);

        // Qr: an uncapped auto-width qr still fills the remaining budget.
        let (qr_extent, _) = measured_extent_of(
            LayoutItem::Qr {
                name: None,
                value: Some("abc".to_string()),
                placement: Placement {
                    at: Position([10.0, 0.0]),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(20.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
            },
            100.0,
        );
        assert_eq!(qr_extent, 100.0, "no bound means fill the remaining budget");

        // Container, measurement: the child must be something whose measured width depends on
        // the inner budget, or the assertion proves nothing. An empty container contributes `at_x`
        // whatever the budget was, including a budget wrongly capped to zero.
        let child = LayoutItem::Text {
            name: None,
            value: Some(long.to_string()),
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(8.0),
                ])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            multiline: false,
            alignment: crate::models::Alignment::default(),
        };
        let (c_extent, _) = measured_extent_of(capped_container(10.0, None, vec![child]), 200.0);
        assert!(
            (c_extent - (10.0 + text_extent)).abs() < 0.5,
            "an uncapped container is sized by its child ({text_extent}mm at x=10), got {c_extent}"
        );

        // Container, render: uncapped, fills the frame remainder.
        let source = dynamic_ctx_source(100.0, capped_container(10.0, None, vec![]));
        assert!(
            source.contains("width: 90mm"),
            "an uncapped container fills the frame remainder: {source}"
        );
    }

    /// #152. `brother_24mm_weights.yaml` sets `max_w: 117` at `at.x: 1.5` on a `width.max: 120`
    /// tape, so the budget goes from 118.5 to 117 — a cap that binds by only 1.5mm. With the short
    /// placeholder data the suite uses, the render must be unchanged: this pins that a cap this
    /// close to the natural remainder does not perturb a real catalog/fixture template.
    #[test]
    fn brother_24mm_weights_render_is_unchanged_by_the_cap() {
        let registry = crate::templates::load_all_for_tests().0;
        let capped = registry.get("brother_24mm_weights").expect("template");
        let TemplateFormat::Single {
            width: Dimension::Dynamic {
                max: Some(max_w), ..
            },
            ..
        } = &capped.format
        else {
            panic!("expected a dynamic-width single format");
        };
        assert_eq!(*max_w, 120.0, "budget math below assumes width.max: 120");
        let data = placeholder_data(capped);
        let capped_png = render_thumbnail_png(capped, &data, None, &no_settings(), &no_datetime())
            .expect("render capped");

        // Same template, `max_w` stripped from both text items: the fallback remainder is
        // `width.max - at.x` = 118.5mm, so 117mm binds by only 1.5mm. With the short placeholder
        // text this suite uses, neither budget is the constraint that decides the fitted font
        // size or width, so the two renders must be pixel-identical.
        let mut uncapped = capped.clone();
        let Layout::Items(items) = &mut uncapped.layout;
        for item in items {
            if let LayoutItem::Text { placement, .. } = item {
                placement.max_w = None;
            }
        }
        let uncapped_png =
            render_thumbnail_png(&uncapped, &data, None, &no_settings(), &no_datetime())
                .expect("render uncapped");

        assert_eq!(
            capped_png, uncapped_png,
            "a 117mm cap on a 118.5mm remainder must not change the render with short placeholder text"
        );
    }

    fn to_text(at: [f32; 2], to: [f32; 2], value: &str) -> LayoutItem {
        LayoutItem::Text {
            name: None,
            value: Some(value.to_string()),
            placement: Placement {
                at: Position(at),
                extent: crate::models::Extent::To(Position(to)),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            font_size: FontSize::Fixed(10.0),
            font_weight: None,
            multiline: false,
            alignment: crate::models::Alignment::default(),
        }
    }

    /// The whole point of #147: a text box that spans the label still sizes the label to its own
    /// content, so several full-width centered lines produce a label as wide as the longest one.
    #[test]
    fn an_edge_relative_to_text_contributes_its_natural_width() {
        let (extent, pushed) =
            measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42"), 80.0);
        assert!(
            extent > 0.0 && extent < 80.0,
            "expected a content-sized extent, got {extent}"
        );
        assert_eq!(
            pushed, 1,
            "an edge-relative to text is measured like an auto one"
        );
    }

    /// A right margin has to be paid for out of the label width, or the text is clipped by its own box.
    #[test]
    fn an_inset_to_text_contributes_its_natural_width_plus_the_inset() {
        let (plain, _) = measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42"), 80.0);
        let (inset, _) = measured_extent_of(to_text([0.0, 0.0], [-2.0, 8.0], "Widget A-42"), 80.0);
        assert!(
            (inset - (plain + 2.0)).abs() < 0.2,
            "expected {} + 2, got {inset}",
            plain
        );
    }

    /// A numeric `to` is a fixed width: known before the frame is, so it measures like `size:` and is
    /// rendered by fit_text_to_box, not replayed from a MeasuredText.
    #[test]
    fn a_numeric_to_text_measures_as_a_fixed_width() {
        let (extent, pushed) = measured_extent_of(
            to_text([0.0, 0.0], [30.0, 8.0], "text far too long for 30mm"),
            100.0,
        );
        assert_eq!(extent, 30.0);
        assert_eq!(pushed, 0, "a fixed-width item must not push a MeasuredText");
    }

    /// A cap on an auto-width text must bind during measurement, since the rendered box is exactly
    /// what the measure pass recorded.
    #[test]
    fn max_w_caps_an_auto_width_text_during_measurement() {
        let long = "a string far too long to fit inside twenty millimetres of tape";
        fn text(max_w: Option<f32>, value: &str) -> LayoutItem {
            LayoutItem::Text {
                name: None,
                value: Some(value.to_string()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(8.0),
                    ])),
                    max_w,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                multiline: false,
                alignment: crate::models::Alignment::default(),
            }
        }
        let (uncapped, _) = measured_extent_of(text(None, long), 100.0);
        let (capped, pushed) = measured_extent_of(text(Some(20.0), long), 100.0);
        assert_eq!(pushed, 1);
        assert!(
            uncapped > 20.0,
            "the fixture must be long enough to exceed the cap, got {uncapped}"
        );
        assert!(
            capped <= 20.0 + 1.0e-3,
            "max_w must bind during measurement: measured {capped} against a 20mm cap"
        );
    }

    /// A capped qr sizes the label to its cap, not to `width.max`. Rendering already honored `max_w`
    /// here, so before this fix an auto-length label came out `width.max` long with a small code on
    /// it. An image item takes the same measure arm, so this covers both.
    #[test]
    fn max_w_caps_an_auto_width_qr_during_measurement() {
        let qr = |max_w: Option<f32>| LayoutItem::Qr {
            name: None,
            value: Some("abc".to_string()),
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: crate::models::Extent::Size(Size([
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                    SizeValue::Value(20.0),
                ])),
                max_w,
                max_h: None,
                rotate: None,
            },
            params: None,
        };
        let (capped, pushed) = measured_extent_of(qr(Some(30.0)), 100.0);
        assert_eq!(pushed, 0, "a qr never records a MeasuredText");
        assert_eq!(
            capped, 30.0,
            "a capped qr must contribute its cap, not the whole {}mm budget",
            100.0
        );
    }

    /// A slot expressed with edge-relative corners must measure the same as the identical slot
    /// expressed with plain ones. `at.y: -32` in a 40mm frame is y=8, so the slot is 32mm tall; mixing
    /// a resolved `to.y` with a raw `at.y` would compute 72mm, and the fitter would choose a font size
    /// the render-time box cannot fit.
    #[test]
    fn an_edge_relative_at_y_is_resolved_before_the_measure_height() {
        fn wrapped(at: [f32; 2], to: [f32; 2]) -> LayoutItem {
            LayoutItem::Text {
                name: None,
                value: Some("Some words that will wrap across several lines".to_string()),
                placement: Placement {
                    at: Position(at),
                    extent: crate::models::Extent::To(Position(to)),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Range {
                    min: 6.0,
                    max: 28.0,
                },
                font_weight: None,
                multiline: true,
                alignment: crate::models::Alignment::default(),
            }
        }
        // The frame is 40mm tall (see `measured_extent_of`), so these two describe the same 32mm slot.
        let (edge, _) = measured_extent_of(wrapped([0.0, -32.0], [-0.0, -0.0]), 60.0);
        let (plain, _) = measured_extent_of(wrapped([0.0, 8.0], [-0.0, 40.0]), 60.0);
        assert!(
            (edge - plain).abs() < 0.01,
            "the same slot measured {edge} with edge-relative corners and {plain} with plain ones"
        );
    }

    /// A container spanning to the right edge is measured by its children, like an auto-width one.
    /// Measuring it at its resolved width instead would peg every such label to its maximum.
    #[test]
    fn an_edge_relative_to_container_is_measured_by_its_children() {
        let (bare, _) = measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42"), 80.0);

        let container = LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: crate::models::Extent::To(Position([-0.0, 10.0])),
                max_w: None,
                max_h: None,
                rotate: None,
            },
            option: None,
            frame: None,
            padding: crate::models::Padding {
                top: 0.0,
                right: 1.0,
                bottom: 0.0,
                left: 1.0,
            },
            items: vec![to_text([0.0, 0.0], [-0.0, 8.0], "Widget A-42")],
        };
        let (wrapped, pushed) = measured_extent_of(container, 80.0);
        assert_eq!(pushed, 1, "the child text is still measured exactly once");
        assert!(
            wrapped < 80.0,
            "the container was measured at its resolved width ({wrapped}), not by its children"
        );
        // 1mm of padding a side, and the child's budget shrinks by the same 2mm, so allow some slack.
        assert!(
            (wrapped - (bare + 2.0)).abs() < 0.5,
            "expected roughly the child width {bare} plus 2mm of padding, got {wrapped}"
        );
    }

    /// A qr spanning to the right edge has no intrinsic width this codebase measures, so it must not
    /// drag the label out to its maximum. Text alone sizes the label.
    #[test]
    fn an_edge_relative_to_qr_contributes_nothing() {
        let (extent, pushed) = measured_extent_of(
            LayoutItem::Qr {
                name: None,
                value: Some("payload".to_string()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    extent: crate::models::Extent::To(Position([-0.0, 8.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
            },
            80.0,
        );
        assert_eq!(extent, 0.0);
        assert_eq!(pushed, 0);
    }

    /// Carried over from Task 4's review: no unit test covered an edge-relative `at.x` on a `Qr`
    /// specifically (only `Text` had one). Clause 1 must skip it the same way regardless of item kind.
    #[test]
    fn an_edge_relative_at_x_on_a_qr_contributes_only_its_inset() {
        let (extent, pushed) = measured_extent_of(
            LayoutItem::Qr {
                name: None,
                value: Some("payload".to_string()),
                placement: Placement {
                    at: Position([-5.0, 0.0]),
                    extent: crate::models::Extent::Size(Size([
                        SizeValue::Value(10.0),
                        SizeValue::Value(10.0),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
            },
            80.0,
        );
        assert_eq!(
            extent, 5.0,
            "an edge-relative at.x contributes only its inset"
        );
        assert_eq!(pushed, 0, "a qr never pushes a MeasuredText");
    }

    /// Measuring against the un-inset budget lets a text whose natural width reaches the budget
    /// contribute budget + inset. The page clamps back to `max`, and the text is then fitted into a
    /// box `inset` narrower than the width it was measured at, clipping it. The contribution must
    /// never exceed the budget it was measured against.
    #[test]
    fn an_inset_to_text_never_measures_wider_than_its_own_box() {
        let long = "a very long string that will not fit in forty millimetres at all";
        let (extent, pushed) = measured_extent_of(to_text([0.0, 0.0], [-2.0, 8.0], long), 40.0);
        assert_eq!(pushed, 1);
        assert!(
            extent <= 40.0 + 1.0e-3,
            "an inset item contributed {extent} against a 40mm budget: the inset was not subtracted \
             from the measure budget, so the label clamps to 40 and the text is clipped by 2mm"
        );
        // The measured text itself has to fit the box it will get: budget minus the inset.
        let (plain, _) = measured_extent_of(to_text([0.0, 0.0], [-0.0, 8.0], long), 38.0);
        assert!(
            (extent - (plain + 2.0)).abs() < 0.2,
            "expected the inset contribution to be the 38mm-budget width plus 2, got {extent} vs {plain}"
        );
    }

    /// Review finding (code-reviewer, post-Task-8): clause 1 used to skip a right-anchored
    /// container's subtree entirely, so a frame-dependent child inside it (here, a `to`-spanned
    /// text) never got a `MeasuredText` pushed. `render_container_item` has no such skip and
    /// recurses unconditionally, so `render_text_item` then consumed a cursor entry that was never
    /// pushed and failed with "auto-length cursor overrun". The container's own width (`size:
    /// [8, 8]`) is fixed, so `validate_placement_position` allows pairing it with an edge-relative
    /// `at.x`; clause 1 must still measure the children against that known inner width.
    #[test]
    fn a_frame_dependent_child_inside_a_right_anchored_container_does_not_mismatch_the_cursor() {
        let template = TemplateDefinition {
            id: "t".to_string(),
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(5.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(8.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: Placement {
                    at: Position([-10.0, 0.0]),
                    extent: Extent::Size(Size([SizeValue::Value(8.0), SizeValue::Value(8.0)])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                option: None,
                frame: None,
                padding: crate::models::Padding::ZERO,
                items: vec![to_text([0.0, 0.0], [-0.0, 6.0], "x")],
            }]),
            version: None,
        };
        assert_eq!(
            template.validate(),
            Ok(()),
            "a fixed-width container paired with an edge-relative at.x is a legal shape"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        render_single_label(&template, &data, None, &no_settings(), &no_datetime()).expect(
            "a right-anchored container's frame-dependent child must still be measured, not \
             skipped along with the container",
        );
    }

    /// Review finding (code-reviewer, post-Task-8): Step 4 of the task brief routed the container's
    /// fixed-branch height through `resolve_size(..., allow_auto_fill: false)`, which has no
    /// fallback for an auto height with no `max_h`. `size: [40, auto]` is a documented container
    /// idiom (SPEC §4: "auto size resolves to `max_w`/`max_h` if present; for `container` it falls
    /// back to the parent frame"), accepted by `validate()` and rendered fine by
    /// `render_container_item` (which passes `allow_auto_fill: true`); only the measure pre-pass had
    /// been tightened, so every such container on a dynamic-width label started failing measurement
    /// with "size height is auto but no max_height provided".
    #[test]
    fn an_auto_height_fixed_width_container_measures_without_erroring() {
        let template = TemplateDefinition {
            id: "t".to_string(),
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(30.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Container {
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    extent: Extent::Size(Size([
                        SizeValue::Value(40.0),
                        SizeValue::Auto(AutoSize::Auto),
                    ])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                option: None,
                frame: None,
                padding: crate::models::Padding::ZERO,
                items: vec![],
            }]),
            version: None,
        };
        assert_eq!(
            template.validate(),
            Ok(()),
            "`size: [40, auto]` is a documented container idiom"
        );
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        render_single_label(&template, &data, None, &no_settings(), &no_datetime()).expect(
            "an auto height with no max_h must fall back to the remaining frame height during \
             measurement, not error",
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
        let ctx = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let container = LayoutItem::Container {
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::Value(80.0), SizeValue::Value(40.0)]),
            ),
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

    /// The render-time copy of the helper must cap identically, or it drifts from validation —
    /// which is exactly the class of bug #152 is.
    #[test]
    fn render_resolve_size_value_caps_rather_than_substituting() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (100.0, 12.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let auto = SizeValue::Auto(crate::models::AutoSize::Auto);
        assert_eq!(
            ctx.resolve_size_value(&auto, Some(30.0), Some(10.0), "width")
                .unwrap(),
            10.0
        );
        assert_eq!(
            ctx.resolve_size_value(&auto, Some(10.0), Some(30.0), "width")
                .unwrap(),
            10.0
        );
        assert_eq!(
            ctx.resolve_size_value(&auto, Some(30.0), None, "width")
                .unwrap(),
            30.0
        );
        assert_eq!(
            ctx.resolve_size_value(&auto, None, Some(30.0), "width")
                .unwrap(),
            30.0
        );
        assert!(ctx.resolve_size_value(&auto, None, None, "width").is_err());
        assert_eq!(
            ctx.resolve_size_value(&SizeValue::Value(50.0), Some(30.0), Some(30.0), "width")
                .unwrap(),
            50.0,
            "a numeric size is never clamped by the bound"
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
                    extent: Extent::Size(Size([SizeValue::Value(80.0), SizeValue::Value(40.0)])),
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
                placement: Placement::sized(
                    Position([2.0, 2.0]),
                    Size([SizeValue::Value(30.0), SizeValue::Value(8.0)]),
                ),
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(14.0), SizeValue::Value(14.0)]),
                ),
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(14.0), SizeValue::Value(14.0)]),
                ),
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
                extent: Extent::Size(Size([SizeValue::Value(24.0), SizeValue::Value(24.0)])),
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
                placement: Placement::sized(
                    Position([1.0, 1.0]),
                    Size([SizeValue::Value(20.0), SizeValue::Value(8.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: Alignment::default(),
            }],
        };
        let outer = LayoutItem::Container {
            placement: Placement {
                at: Position([0.0, 0.0]),
                extent: Extent::Size(Size([SizeValue::Value(80.0), SizeValue::Value(40.0)])),
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

    /// An auto-length (dynamic-width) tape template whose single text item owns the whole
    /// `height_mm`-tall label, so the item's slot is exactly the rendered image. 180 dpi keeps the
    /// pixel geometry the same as the bundled brother tapes.
    fn autolength_tape(
        text: &str,
        multiline: bool,
        vertical: VerticalAlign,
        font_pt: f32,
    ) -> TemplateDefinition {
        const HEIGHT_MM: f32 = 20.0;
        TemplateDefinition {
            id: "tape".to_string(),
            name: "Tape".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(HEIGHT_MM),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: None,
                value: Some(text.to_string()),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Auto(AutoSize::Auto), SizeValue::Value(HEIGHT_MM)]),
                ),
                font_size: FontSize::Fixed(font_pt),
                font_weight: None,
                multiline,
                alignment: Alignment {
                    horizontal: HorizontalAlign::Center,
                    vertical,
                },
            }]),
            version: None,
        }
    }

    /// A tape whose slot height is the caller's choice, so the same string can be rendered with room
    /// to spare and then in a slot tight enough that the old cap-height/baseline box would clip.
    fn tape_of_height(
        text: &str,
        vertical: VerticalAlign,
        font_pt: f32,
        height_mm: f32,
    ) -> TemplateDefinition {
        let mut t = autolength_tape(text, false, vertical, font_pt);
        t.format = TemplateFormat::Single {
            width: Dimension::Dynamic {
                min: Some(10.0),
                max: Some(200.0),
            },
            height: Dimension::Fixed(height_mm),
            media_width: None,
        };
        let Layout::Items(items) = &mut t.layout;
        if let Some(LayoutItem::Text { placement, .. }) = items.first_mut() {
            placement.extent = Extent::Size(Size([
                SizeValue::Auto(AutoSize::Auto),
                SizeValue::Value(height_mm),
            ]));
        }
        t
    }

    fn ink_pixels(png: &[u8]) -> u64 {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        img.pixels().map(|p| (255 - p.0[0]) as u64).sum()
    }

    /// Clipping removes ink, so the same string at the same size must put the same ink on the page in
    /// a generous slot and in a tight one. Fixed `font_size` throughout: that bypasses the fitter, so
    /// this tests the *placement* pad alone — which is the half that fixes #124's reported defect.
    /// The reservation half is covered by the fitting tests in helpers.rs.
    #[test]
    fn ink_survives_a_tight_slot_at_top_and_bottom_alignment() {
        for (text, vertical) in [
            ("Édgy", VerticalAlign::Top),
            ("gjpqy", VerticalAlign::Bottom),
            ("Édgy", VerticalAlign::Bottom),
            ("gjpqy", VerticalAlign::Top),
        ] {
            // The control is a *centered* render in a roomy slot, not a taller slot at the same
            // alignment: bottom alignment puts the baseline on the slot floor however tall the slot
            // is, so an aligned control would clip exactly as much as the subject and the comparison
            // would be blind. Centered in 30mm, nothing can be cut.
            let generous = ink_pixels(&render_tape(&tape_of_height(
                text,
                VerticalAlign::Center,
                12.0,
                30.0,
            )));
            // 5.3mm just holds the 12pt ink band (1.21em = 5.12mm): enough room for the glyphs, but
            // only if the block is inset. Unpadded, the baseline sits on the slot edge and the
            // descenders or accents fall outside. A slot smaller than the band cannot be saved by
            // placement at a fixed font size, which is what the fitter reservation is for.
            let tight = ink_pixels(&render_tape(&tape_of_height(text, vertical, 12.0, 5.3)));
            let loss = (generous as f64 - tight as f64) / generous as f64;
            assert!(
                loss < 0.005,
                "{text} {vertical:?}: the tight slot lost {:.1}% of the ink",
                loss * 100.0
            );
        }
    }

    /// Count bands of inked rows separated by at least one blank row — i.e. how many lines of text
    /// actually landed on the page.
    fn ink_bands(png: &[u8]) -> usize {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let mut bands = 0;
        let mut inside = false;
        for y in 0..h {
            let inked = (0..w).any(|x| img.get_pixel(x, y).0[0] < 128);
            if inked && !inside {
                bands += 1;
            }
            inside = inked;
        }
        bands
    }

    /// #148: the print form now offers a textarea for multiline fields, which is only worth anything
    /// if a newline in the data becomes a line on the label. The UI tests can prove the value reaches
    /// the request; only a render proves the rest.
    #[test]
    fn a_newline_in_a_multiline_field_renders_as_two_lines() {
        let two = render_tape(&autolength_tape(
            "one\ntwo",
            true,
            VerticalAlign::Center,
            12.0,
        ));
        assert_eq!(
            ink_bands(&two),
            2,
            "a two-line value must put two lines of ink on the label"
        );

        // The control case: the same value in a single-line item keeps one line, which is the
        // truncation the form now warns about.
        let one = render_tape(&autolength_tape(
            "one\ntwo",
            false,
            VerticalAlign::Center,
            12.0,
        ));
        assert_eq!(
            ink_bands(&one),
            1,
            "a single-line item must still render only its first line"
        );
    }

    /// First and last image rows carrying ink, plus the image height.
    fn ink_rows(png: &[u8]) -> (u32, u32, u32) {
        let img = image::load_from_memory(png).expect("decode").to_luma8();
        let (w, h) = (img.width(), img.height());
        let inked: Vec<u32> = (0..h)
            .filter(|&y| (0..w).any(|x| img.get_pixel(x, y).0[0] < 128))
            .collect();
        assert!(!inked.is_empty(), "rendered label has no ink");
        (inked[0], inked[inked.len() - 1], h)
    }

    fn render_tape(template: &TemplateDefinition) -> Vec<u8> {
        render_single_label(
            template,
            &HashMap::new(),
            None,
            &no_settings(),
            &no_datetime(),
        )
        .expect("render tape label")
    }

    /// #123: auto-length text placed its own box using fontdue's full line height (~1.21 em) while
    /// Typst lays the line out cap-height-to-baseline (~0.73 em) at the box top, so centered text
    /// floated ~0.24 em high. "test" has no descender, so its ink box is cap-height-to-baseline and
    /// centering it must put the ink centre on the slot centre. Two font sizes: the old error scaled
    /// with the em (~6.5 px at 12 pt, ~13 px at 24 pt), so any re-introduced metric-derived offset
    /// blows the tolerance at 24 pt even if it hid at 12 pt.
    #[test]
    fn autolength_text_centers_vertically() {
        for (label, multiline, text) in [
            ("single line", false, "test"),
            ("multiline", true, "test\ntest"),
        ] {
            for font_pt in [12.0, 24.0] {
                let png = render_tape(&autolength_tape(
                    text,
                    multiline,
                    VerticalAlign::Center,
                    font_pt,
                ));
                let (top, bottom, height) = ink_rows(&png);
                let offset = (top + bottom) as f32 / 2.0 - (height - 1) as f32 / 2.0;
                assert!(
                    offset.abs() <= 2.0,
                    "{label} at {font_pt}pt: ink rows {top}..{bottom} in {height}px label are off-centre by {offset:+.1}px"
                );
            }
        }
    }

    /// #133: alignment is baseline-relative, the industry norm. A fixed metric box (Typst's default
    /// cap-height→baseline, the same box CSS `text-box-trim` and Figma's vertical trim use) means the
    /// baseline lands in the same place no matter which glyphs a string contains — so `test`,
    /// `testj` and `es` sit on one line. #127 briefly centred the per-string ink box instead, which
    /// centred each label perfectly but let `j` and `t` move the baseline between labels.
    #[test]
    fn baseline_is_stable_across_glyph_classes() {
        // Strings with no descender end their ink ON the baseline, so the last inked row is a direct
        // read of where the baseline sits.
        let baseline_of = |text: &str| {
            let (_, bottom, _) = ink_rows(&render_tape(&autolength_tape(
                text,
                false,
                VerticalAlign::Center,
                18.0,
            )));
            bottom
        };
        let reference = baseline_of("test");
        for text in ["es", "MESSAGE", "Ml", "123"] {
            let got = baseline_of(text);
            assert!(
                got.abs_diff(reference) <= 1,
                "{text:?} put its baseline at row {got}, but \"test\" is at {reference}: \
                 alignment must not depend on which glyphs the string contains"
            );
        }

        // Descenders hang below that same baseline rather than moving it: the ink runs lower, by
        // about the descender depth, and by the SAME amount for every descender string.
        let with_desc: Vec<u32> = ["testj", "message", "typogy"]
            .iter()
            .map(|t| baseline_of(t))
            .collect();
        for (text, got) in ["testj", "message", "typogy"].iter().zip(&with_desc) {
            assert!(
                *got > reference,
                "{text:?} has a descender, so its ink must extend below the baseline ({got} vs {reference})"
            );
        }
        let spread = with_desc.iter().max().unwrap() - with_desc.iter().min().unwrap();
        assert!(
            spread <= 1,
            "descender strings must all hang the same distance below the baseline, spread was {spread}px"
        );
    }

    /// A blank edge line carries no ink, so it is trimmed at emission (#127): it must not drag the
    /// visible text off centre. `fit_text_auto_length` does preserve them — `"\nmessage"` measures as
    /// `["", "message"]` — and a leading one adds a real line box, so without the trim the text sits
    /// a full line-advance low. (A *trailing* blank is trimmed for the same reason but is not
    /// separately observable: Typst gives a trailing empty line no box.)
    #[test]
    fn blank_edge_line_does_not_shift_centering() {
        let plain = render_tape(&autolength_tape(
            "message",
            true,
            VerticalAlign::Center,
            18.0,
        ));
        let leading = render_tape(&autolength_tape(
            "\nmessage",
            true,
            VerticalAlign::Center,
            18.0,
        ));
        let (t1, b1, _) = ink_rows(&plain);
        let (t2, b2, _) = ink_rows(&leading);
        assert_eq!(
            t2 + b2,
            t1 + b1,
            "a leading blank line changed the ink centre ({t2}..{b2} vs {t1}..{b1})"
        );
    }

    /// Guards the other two `alignment.vertical` values (ADR-0030 honours them literally), so a
    /// centering fix cannot hardcode centre.
    ///
    /// #124 turned "pinned to the edge" into "inset by the font's ink overflow", so this asserts the
    /// *metric* inset rather than contact. It deliberately does not require the ink to touch the
    /// slot edge: the pad is `ascender − cap_height` / `|descender|`, which overshoots `test`'s
    /// actual glyphs, so demanding contact would fail a correct implementation and push it toward
    /// glyph-dependent placement — the thing ADR-0050 rejects.
    #[test]
    fn autolength_text_top_and_bottom_pin_to_slot_edges() {
        let (top_first, top_last, height) = ink_rows(&render_tape(&autolength_tape(
            "test",
            false,
            VerticalAlign::Top,
            12.0,
        )));
        let (bottom_first, bottom_last, _) = ink_rows(&render_tape(&autolength_tape(
            "test",
            false,
            VerticalAlign::Bottom,
            12.0,
        )));
        // The pad is 0.2412em at 12pt = 2.89pt of an 18mm-tall tape rendered at `height` rows, plus
        // the cap-height gap `test` leaves under the ascender line. Bound it generously above and
        // require it to be non-zero below: zero would mean the pad never reached this path.
        let px_per_pt = height as f32 / super::helpers::units_to_pt_for_test(20.0, "mm");
        let pad_px = 0.2412 * 12.0 * px_per_pt;
        assert!(
            top_first as f32 >= pad_px * 0.5,
            "top-aligned ink must be inset by the pad, got row {top_first} (pad ≈ {pad_px:.1}px)"
        );
        assert!(
            (top_first as f32) < pad_px * 3.0,
            "top-aligned ink is far below the pad, got row {top_first} (pad ≈ {pad_px:.1}px)"
        );
        let bottom_gap = (height - 1 - bottom_last) as f32;
        assert!(
            bottom_gap >= pad_px * 0.5 && bottom_gap < pad_px * 3.0,
            "bottom-aligned ink must be inset by the pad, got gap {bottom_gap} (pad ≈ {pad_px:.1}px)"
        );
        assert!(
            bottom_first > top_last,
            "bottom alignment must sit below top alignment ({bottom_first} vs {top_last})"
        );
    }

    /// #137: the catalog index lists the request fields a template needs. `{vars.*}` and
    /// `{datetime.*}` resolve from the variables store and the datetime resolver, not the caller, so
    /// they must not appear — `homebox-qr` would otherwise advertise `vars.qr_base_url` as something
    /// the user has to supply.
    #[test]
    fn template_fields_lists_request_keys_only() {
        let registry = crate::templates::load_all_for_tests().0;
        let t = registry.get("homebox-qr").expect("homebox-qr");
        assert_eq!(
            template_fields(t),
            vec!["id".to_string(), "message".to_string()]
        );
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                ),
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(20.0), SizeValue::Value(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
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
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                    ),
                    font_size: FontSize::Fixed(10.0),
                    font_weight: None,
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: Some("code".to_string()),
                    value: None,
                    placement: Placement::sized(
                        Position([20.0, 0.0]),
                        Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                    ),
                    params: None,
                },
                LayoutItem::Line {
                    at: Position([0.0, 1.0]),
                    to: Position([30.0, 1.0]),
                    thickness: 0.2,
                },
                LayoutItem::Container {
                    placement: Placement::sized(
                        Position([0.5, 1.5]),
                        Size([SizeValue::Value(29.0), SizeValue::Value(18.0)]),
                    ),
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                ),
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
                ),
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
            placement: Placement::sized(
                Position([0.0, 0.0]),
                Size([SizeValue::Value(20.0), SizeValue::Value(20.0)]),
            ),
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(20.0), SizeValue::Value(5.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
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

    /// #135: every template in either root must parse, validate and render — the five catalog
    /// entries and the five fixtures alike. Deliberately wider than the catalog: `brother_18mm_qr`,
    /// `brother_9mm` and `brother_18mm` have zero references anywhere in `src/`, so this gate is the
    /// only thing proving they work. An exact set, not a floor: "render whatever the loader found"
    /// passes vacuously the moment a root is misconfigured and the loader quietly returns fewer.
    #[test]
    fn every_template_renders() {
        let registry = crate::templates::load_all_for_tests().0;
        // Bind the Vec: `summaries()` returns by value, so borrowing `&str` straight out of the
        // call expression drops the temporary while the set still holds references (E0716).
        let summaries = registry.summaries();
        let found: BTreeSet<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
        let expected: BTreeSet<&str> = BTreeSet::from([
            "avery5163",
            "avery5163_asset_tag",
            "brother_12mm",
            "brother_18mm",
            "brother_18mm_qr",
            "brother_24mm",
            "brother_24mm_lines_divider",
            "brother_24mm_max_w_cap",
            "brother_24mm_multiline",
            "brother_24mm_qr",
            "brother_24mm_weights",
            "brother_9mm",
            "homebox-qr",
        ]);
        assert_eq!(
            found, expected,
            "template roots do not hold the expected set"
        );
        // homebox-qr interpolates {vars.qr_base_url} and {datetime.iso_date}; supply both so the
        // demo entry is covered rather than skipped.
        let settings =
            BTreeMap::from([("qr_base_url".to_string(), "https://example.com".to_string())]);
        let formats = BTreeMap::from([("iso_date".to_string(), "%Y-%m-%d".to_string())]);
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now: chrono::Local::now(),
        };
        for summary in registry.summaries() {
            let template = registry.get(&summary.id).expect("template");
            let data = placeholder_data(template);
            let selection = default_option_selection(template);
            let png = render_thumbnail_png(template, &data, selection.as_ref(), &settings, &dt)
                .unwrap_or_else(|e| panic!("render {}: {e:?}", summary.id));
            assert_eq!(
                &png[..8],
                b"\x89PNG\r\n\x1a\n",
                "{} did not render a PNG",
                summary.id
            );
        }
    }

    fn walk_templates(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
            let path = entry.expect("entry").path();
            let meta = std::fs::symlink_metadata(&path).expect("stat template entry");
            if meta.is_dir() {
                walk_templates(&path, out);
            } else if path.extension().is_some_and(|x| x == "yaml" || x == "yml") {
                out.push(path);
            }
        }
    }

    /// #135: the catalog is the product surface and is designed, not accreted. An exact set rather
    /// than a count: it names what ships, and it fails on a silent rename as well as an addition.
    /// Fixtures live in `tests/fixtures/templates/` and must never appear here.
    #[test]
    fn catalog_is_exactly_the_starter_set() {
        let mut files = Vec::new();
        walk_templates(std::path::Path::new("catalog"), &mut files);
        let found: BTreeSet<String> = files
            .iter()
            .map(|p| p.file_stem().expect("stem").to_string_lossy().to_string())
            .collect();
        let expected: BTreeSet<String> = [
            "avery5163",
            "brother_12mm",
            "brother_18mm",
            "brother_24mm",
            "brother_9mm",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            found, expected,
            "catalog contents changed; update this gate and docs/adr/0047 deliberately"
        );
    }

    /// Ids are the API key, the `/print/{id}` route and what print webhooks hardcode, and installs
    /// land flat in `{config}/templates` — so a duplicate id anywhere in the nested catalog would
    /// collide on install, and an id that differs from its filename would install under a name the
    /// catalog does not know (#137).
    #[test]
    fn template_ids_are_unique_and_match_filenames() {
        let mut files = Vec::new();
        walk_templates(std::path::Path::new("catalog"), &mut files);
        // Both roots flatten into one dir at test time, so a cross-root duplicate would overwrite a
        // file before `load_from_dir` could ever see two ids (#135).
        walk_templates(std::path::Path::new("tests/fixtures/templates"), &mut files);
        assert!(!files.is_empty(), "no templates found");

        let mut seen: HashMap<String, std::path::PathBuf> = HashMap::new();
        for path in files {
            let yaml = std::fs::read_to_string(&path).expect("read template");
            let id = yaml
                .lines()
                .find_map(|l| l.strip_prefix("id:"))
                .map(|v| v.trim().to_string())
                .unwrap_or_else(|| panic!("{path:?} has no id"));
            let stem = path
                .file_stem()
                .expect("stem")
                .to_string_lossy()
                .to_string();
            assert_eq!(id, stem, "{path:?}: id must equal the filename stem");
            if let Some(prev) = seen.insert(id.clone(), path.clone()) {
                panic!("duplicate catalog id {id}: {prev:?} and {path:?}");
            }
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
                    placement: Placement::sized(
                        Position([0.0, 10.0]),
                        Size([SizeValue::Value(40.0), SizeValue::Value(8.0)]),
                    ),
                    font_size: FontSize::Fixed(8.0),
                    font_weight: None,
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: None,
                    value: Some("{vars.qr_base_url}/{id}".to_string()),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                    ),
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
                placement: Placement::sized(
                    Position([0.0, 6.0]),
                    Size([SizeValue::Value(60.0), SizeValue::Value(8.0)]),
                ),
                font_size: FontSize::Fixed(8.0),
                font_weight: None,
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
        let registry = crate::templates::load_all_for_tests().0;
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
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
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(10.0), SizeValue::Value(5.0)]),
                    ),
                    font_size: FontSize::Fixed(6.0),
                    font_weight: None,
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: None,
                    value: Some("{url} {vars.base} {datetime} {datetime.short_date}".into()),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(5.0), SizeValue::Value(5.0)]),
                    ),
                    params: None,
                },
                LayoutItem::Image {
                    name: Some("logo".into()),
                    src: None,
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::Value(5.0), SizeValue::Value(5.0)]),
                    ),
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
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::Value(40.0), SizeValue::Value(20.0)]),
                ),
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
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

    /// Engine-upgrade visual harness (#101): dumps a label-sized PNG for every bundled template
    /// (both avery orientations) into $LABELER_RENDER_DUMP_DIR. Run explicitly:
    /// LABELER_RENDER_DUMP_DIR=.render-scratch/pre-015 cargo test --lib dump_all_template_renders -- --ignored
    #[test]
    #[ignore = "env-gated render dump for engine-upgrade visual comparison"]
    fn dump_all_template_renders() {
        let Some(dir) = std::env::var_os("LABELER_RENDER_DUMP_DIR") else {
            panic!("set LABELER_RENDER_DUMP_DIR");
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("create dump dir");
        let registry = crate::templates::load_all_for_tests().0;
        // homebox-qr interpolates {vars.qr_base_url}; placeholder_data excludes variables by design.
        let settings =
            BTreeMap::from([("qr_base_url".to_string(), "https://example.com".to_string())]);
        // homebox-qr also references {datetime.iso_date}, a named format; supply it so the harness
        // resolves it (no_datetime carries no named formats).
        let datetime_formats = BTreeMap::from([("iso_date".to_string(), "%Y-%m-%d".to_string())]);
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &datetime_formats,
            now: chrono::Local::now(),
        };
        for summary in registry.summaries() {
            let template = registry.get(&summary.id).expect("template");
            let data = placeholder_data(template);
            // Render every orientation variant explicitly (default_option_selection picks only the
            // first). Start each variant from the FULL default selection and override orientation,
            // so other option defaults (avery's `outline: yes`) stay in effect.
            let base = default_option_selection(template);
            let selections: Vec<(String, Option<BTreeMap<String, String>>)> = match template
                .options
                .as_ref()
                .and_then(|o| o.allowed().get("orientation").cloned())
            {
                Some(orientations) => orientations
                    .into_iter()
                    .map(|o| {
                        let mut sel = base.clone().unwrap_or_default();
                        sel.insert("orientation".to_string(), o.clone());
                        (format!("{}-{o}", summary.id), Some(sel))
                    })
                    .collect(),
                None => vec![(summary.id.clone(), base.clone())],
            };
            for (name, selection) in selections {
                let png =
                    render_thumbnail_png(template, &data, selection.as_ref(), &settings, &datetime)
                        .unwrap_or_else(|e| panic!("render {name}: {e:?}"));
                std::fs::write(dir.join(format!("{name}.png")), png).expect("write png");
            }
        }
    }

    /// Compile a probe source and hand back the document. Shares `weight_probe_ink`'s
    /// zero-warnings assertion: a missing bundled Inter would otherwise resolve through the
    /// embedded fallback and quietly calibrate the fitter against the wrong font.
    fn compile_probe(source: &str) -> super::PagedDocument {
        let engine = super::TypstEngine::builder()
            .main_file(source.to_string())
            .search_fonts_with(super::typst_font_options())
            .build();
        let warned = engine.compile::<super::PagedDocument>();
        assert!(
            warned.warnings.is_empty(),
            "probe must compile without warnings: {:?}",
            warned.warnings
        );
        warned.output.expect("compile probe")
    }

    /// Lay out `lines` lines at `size` on an auto-height page with no margin: the page height Typst
    /// produces *is* the block height the fitter has to predict. Ink extents are the wrong quantity
    /// here — they include descenders and exclude leading.
    fn typst_block_height_pt(lines: usize, size: f32) -> f32 {
        let body = (0..lines)
            .map(|_| "Hxy")
            .collect::<Vec<_>>()
            .join("#linebreak()");
        let source = format!(
            "#set page(width: 200mm, height: auto, margin: 0mm)\n#set text(font: \"Inter\", size: {size}pt)\n{body}"
        );
        compile_probe(&source).pages()[0].frame.height().to_pt() as f32
    }

    fn typst_line_width_pt(text: &str, size: f32) -> f32 {
        let source = format!(
            "#set page(width: auto, height: auto, margin: 0mm)\n#set text(font: \"Inter\", size: {size}pt)\n{text}"
        );
        compile_probe(&source).pages()[0].frame.width().to_pt() as f32
    }

    /// The fitter's block model must match what Typst lays out, or auto-shrink is guessing. One, two
    /// and three lines: a per-line constant that folds leading in is right at n=1 and wrong by one
    /// leading per line after that, so a single count would not catch it (#96).
    #[test]
    fn block_height_matches_typst_layout() {
        for lines in 1..=3usize {
            let rendered = typst_block_height_pt(lines, 20.0);
            let predicted = super::helpers::block_height_for_test(400, 20.0, lines);
            let drift = (rendered - predicted).abs() / rendered;
            // Measured 0.00% off at every count; 1% leaves room for a future font revision without
            // letting a model error through.
            assert!(
                drift < 0.01,
                "{lines} line(s): predicted {predicted:.2}pt, Typst laid out {rendered:.2}pt ({:.1}% off)",
                drift * 100.0
            );
        }
    }

    /// The per-character advance sum must match a real shaped line. Not a claim of shaping parity —
    /// the string has no kerning pairs or ligatures — but a units-per-em or scaling mistake would
    /// otherwise pass every unit test that compares text_width against itself (#96). Two sizes,
    /// because opsz differs between them.
    #[test]
    fn text_width_matches_typst_layout() {
        let text = "HIHIHI 123";
        for size in [10.0f32, 24.0] {
            let rendered = typst_line_width_pt(text, size);
            let predicted = super::helpers::text_width_for_test(400, size, text);
            let drift = (rendered - predicted).abs() / rendered;
            assert!(
                drift < 0.01,
                "{size}pt: predicted {predicted:.2}pt, Typst laid out {rendered:.2}pt ({:.1}% off)",
                drift * 100.0
            );
        }
    }

    fn weight_probe_ink(weight: u32) -> u64 {
        // Typst 0.15 strips the "Variable" suffix from stored family names (typst-library
        // `typographic_family`), so the bundled InterVariable.ttf registers as "Inter"; requesting
        // "Inter Variable" is now an unknown family and warns. Probe the name that actually resolves
        // so the zero-warnings guard still fails loudly if the bundled Inter is missing.
        let source = format!(
            "#set page(width: 60mm, height: 20mm, margin: 0mm)\n#set text(font: \"Inter\", size: 14pt, weight: {weight})\nWeight Probe 123"
        );
        let engine = super::TypstEngine::builder()
            .main_file(source)
            .search_fonts_with(super::typst_font_options())
            .build();
        let warned = engine.compile::<super::PagedDocument>();
        assert!(
            warned.warnings.is_empty(),
            "font must resolve without warnings (else the embedded fallback could fake a real bold): {:?}",
            warned.warnings
        );
        let doc = warned.output.expect("compile weight probe");
        let pixmap = typst_render::render(&doc.pages()[0], &super::render_options(200.0 / 72.0));
        let png = pixmap.encode_png().expect("png");
        let img = image::load_from_memory(&png).expect("decode").to_luma8();
        img.pixels().map(|p| (255 - p.0[0]) as u64).sum()
    }

    /// #101 acceptance: Typst 0.15 drives the wght axis of the bundled variable Inter.
    /// On 0.14 the axis is ignored (ratio ~1.0) and this fails; on 0.15 bold has ≥10% more ink.
    #[test]
    fn variable_font_weight_is_honored() {
        let regular = weight_probe_ink(400);
        let bold = weight_probe_ink(700);
        assert!(
            bold as f64 >= regular as f64 * 1.10,
            "weight 700 must add ≥10% ink over 400 (got {regular} vs {bold}, ratio {:.3})",
            bold as f64 / regular as f64
        );
    }

    /// Dynamic-width mode is a property of the template format, not of whether any text needed
    /// measuring: a label can be sized by a line or a non-text container alone. An auto-width
    /// container at x=5 on a 25mm dynamic label must be 20mm wide, not the full 25mm, or it overruns
    /// the page by exactly its own offset.
    #[test]
    fn dynamic_width_mode_is_independent_of_measured_text() {
        use std::cell::RefCell;
        // `at_x` differs per mode: the render-time bounds check (Task 5) now rejects a container
        // that resolves past the frame edge, and the fixed-mode auto-width fallback fills the whole
        // frame regardless of offset, so it needs `at_x = 0.0` to stay in bounds. Compile-time
        // `validate_bounds` already forbids the x=5 fixed-mode combination on any real template
        // (5 + 25 > 25), so this keeps the fixture reachable through the real pipeline.
        fn container_width(mode: super::LengthMode<'_>, at_x: f32) -> String {
            let data: HashMap<String, super::JsonValue> = HashMap::new();
            let settings = no_settings();
            let datetime = no_datetime();
            let env = super::RenderEnv {
                settings: &settings,
                datetime: &datetime,
            };
            let images = RefCell::new(super::ImageCollector::default());
            let ctx =
                super::RenderContext::new((25.0, 12.0), "mm", &data, None, &env, &images, mode);
            ctx.render_items(&[LayoutItem::Container {
                placement: Placement::sized(
                    Position([at_x, 0.0]),
                    Size([
                        SizeValue::Auto(crate::models::AutoSize::Auto),
                        SizeValue::Value(12.0),
                    ]),
                ),
                option: None,
                frame: None,
                padding: crate::models::Padding::ZERO,
                items: vec![LayoutItem::Line {
                    at: Position([0.0, 6.0]),
                    to: Position([20.0, 6.0]),
                    thickness: 0.2,
                }],
            }])
            .expect("render")
        }

        let cursor = std::cell::Cell::new(0usize);
        let empty: Vec<super::MeasuredText> = Vec::new();
        let dynamic = container_width(
            super::LengthMode::Dynamic(super::AutoLength {
                texts: &empty,
                cursor: &cursor,
            }),
            5.0,
        );
        assert!(
            dynamic.contains("width: 20mm"),
            "a dynamic label with no measured text must still size the container to the remaining \
             width, got: {dynamic}"
        );

        let fixed = container_width(super::LengthMode::Fixed, 0.0);
        assert!(
            fixed.contains("width: 25mm"),
            "on a fixed label an auto container fills the frame, got: {fixed}"
        );
    }

    /// An edge-relative line endpoint contributes its inset, exactly as a right-anchored box does:
    /// it cannot define the width it is measured against, but the label still has to be at least as
    /// wide as the inset or the endpoint has nowhere to sit. Here the wider endpoint is 5mm in.
    #[test]
    fn an_edge_relative_line_endpoint_contributes_its_inset() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let item = LayoutItem::Line {
            at: Position([-5.0, 6.0]),
            to: Position([-3.0, 6.0]),
            thickness: 0.2,
        };
        let mut measured = Vec::new();
        let extent = ctx.measure(&[item], 80.0, &mut measured).expect("measure");
        assert_eq!(extent, 5.0);
        assert!(measured.is_empty());
    }

    /// A right-anchored item cannot define the width it is anchored to, but the label still has to
    /// be at least as wide as the inset or the item has nowhere to sit. That inset is its
    /// contribution.
    #[test]
    fn an_edge_relative_at_x_contributes_its_inset() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let item = LayoutItem::Text {
            name: None,
            value: Some("x".to_string()),
            placement: Placement::sized(
                Position([-20.0, 0.0]),
                Size([SizeValue::Value(20.0), SizeValue::Value(8.0)]),
            ),
            font_size: FontSize::Fixed(6.0),
            font_weight: None,
            multiline: false,
            alignment: crate::models::Alignment::default(),
        };
        let mut measured = Vec::new();
        let extent = ctx.measure(&[item], 80.0, &mut measured).expect("measure");
        assert_eq!(extent, 20.0);
        assert!(measured.is_empty());
    }

    /// The divider spans to the frame's right edge, not back to x=0.
    #[test]
    fn an_edge_relative_line_renders_to_the_right_edge() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (40.0, 12.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let source = ctx
            .render_items(&[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([-0.0, 6.0]),
                thickness: 0.2,
            }])
            .expect("render");
        assert!(
            source.contains("end: (40mm, 0mm)"),
            "expected a 40mm-long line, got: {source}"
        );
    }

    /// Builds a dynamic-width label whose text measures to roughly 10mm, plus one line.
    fn dynamic_label_with_line(at: Position, to: Position) -> TemplateDefinition {
        TemplateDefinition {
            id: "t".to_string(),
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(5.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: None,
                    value: Some("hi".to_string()),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([
                            SizeValue::Auto(crate::models::AutoSize::Auto),
                            SizeValue::Value(6.0),
                        ]),
                    ),
                    font_size: FontSize::Fixed(6.0),
                    font_weight: None,
                    multiline: false,
                    alignment: crate::models::Alignment::default(),
                },
                LayoutItem::Line {
                    at,
                    to,
                    thickness: 0.2,
                },
            ]),
            version: None,
        }
    }

    /// A right-anchored line beside content-sized text: the label must grow to hold the line's own
    /// inset, the same way it grows to hold a right-anchored box. Before the line rule matched the
    /// box rule this rendered a `a coordinate resolves outside the frame` error, because the label
    /// resolved to the ~10mm of text and the 20mm inset then landed left of x=0.
    #[test]
    fn a_right_anchored_line_widens_the_label_to_its_inset() {
        let template = dynamic_label_with_line(Position([-20.0, 8.0]), Position([-0.0, 8.0]));
        assert_eq!(template.validate(), Ok(()));
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let png = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect("a right-anchored line must render beside auto-width text");
        let width_px = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        // 20mm at 180dpi is ~142px; the ~10mm of text alone would be about half that.
        assert!(
            width_px >= 138,
            "the label must be at least as wide as the line's 20mm inset, got {width_px}px"
        );
    }

    /// The render-time endpoint bound (`check_line`) is the mirror of the load-time one, per SPEC §7's
    /// compile-time/render-time duplication. Load-time validation now rejects every template that
    /// could reach it (a plain endpoint past `width.max` is rejected outright, and an edge-relative
    /// one sizes the label to its own inset), so it is exercised here at the context level.
    #[test]
    fn a_line_endpoint_outside_the_frame_errors_at_render() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (10.0, 12.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let err = ctx
            .render_items(&[LayoutItem::Line {
                at: Position([0.0, 6.0]),
                to: Position([30.0, 6.0]),
                thickness: 0.2,
            }])
            .expect_err("a 30mm endpoint on a 10mm frame must not render");
        assert!(
            err.message_text().contains("outside the frame"),
            "unexpected error: {}",
            err.message_text()
        );
    }

    /// Compile time could not compare these endpoints: one is edge-relative and one is not, and the
    /// final width was unknown. The content measures well under `min`, so the clamp pins the label to
    /// exactly 20mm, where the two endpoints coincide and the line is degenerate after all.
    #[test]
    fn a_line_that_becomes_degenerate_at_the_final_width_errors_at_render() {
        let mut template = dynamic_label_with_line(Position([20.0, 8.0]), Position([-0.0, 8.0]));
        template.format = TemplateFormat::Single {
            width: Dimension::Dynamic {
                min: Some(20.0),
                max: Some(100.0),
            },
            height: Dimension::Fixed(12.0),
            media_width: None,
        };
        assert_eq!(template.validate(), Ok(()), "not comparable at load time");
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let err = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect_err("a zero-length line must not render");
        assert!(
            err.message_text().contains("must differ"),
            "unexpected error: {}",
            err.message_text()
        );
    }

    /// The #146/#147 acceptance template renders, and its width tracks its content. Visual
    /// correctness was verified by looking at the PNG; this guards the mechanics.
    #[test]
    fn the_lines_divider_template_is_content_sized() {
        let (registry, _dir) = crate::templates::load_all_for_tests();
        let template = registry
            .get("brother_24mm_lines_divider")
            .expect("fixture template is loaded");
        let render = |l1: &str, l2: &str| {
            let mut data: HashMap<String, super::JsonValue> = HashMap::new();
            data.insert("line1".to_string(), json!(l1));
            data.insert("line2".to_string(), json!(l2));
            let png = render_single_label(template, &data, None, &no_settings(), &no_datetime())
                .expect("render");
            u32::from_be_bytes([png[16], png[17], png[18], png[19]])
        };
        let short = render("Bin 7", "Shed");
        let long = render("Storage Bin A-42", "Workshop / North Wall");
        assert!(
            long > short,
            "an auto-length label must track its content: {long}px vs {short}px"
        );
    }

    /// A blank optional field is ordinary in CSV-driven printing. The empty value measures to
    /// nothing, the label clamps to the item's own `at.x`, and the `to`-spanning box collapses to
    /// zero width — a legitimate render-time outcome of empty data, not an authoring error, so it
    /// must render rather than 422. The same shape with a value still renders.
    #[test]
    fn an_empty_value_collapses_a_to_spanned_box_instead_of_erroring() {
        let template = TemplateDefinition {
            id: "t".to_string(),
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(60.0),
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: Some("v".to_string()),
                value: None,
                placement: Placement {
                    at: Position([12.0, 0.0]),
                    extent: crate::models::Extent::To(Position([-0.0, 12.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: crate::models::Alignment::default(),
            }]),
            version: None,
        };
        assert_eq!(template.validate(), Ok(()));
        for value in ["hello", ""] {
            let mut data: HashMap<String, super::JsonValue> = HashMap::new();
            data.insert("v".to_string(), json!(value));
            render_single_label(&template, &data, None, &no_settings(), &no_datetime())
                .unwrap_or_else(|err| {
                    panic!("value {value:?} must render, got: {}", err.message_text())
                });
        }
    }

    /// A `to`-sized qr contributes nothing to the measured extent (it has no intrinsic content
    /// width, ADR-0050 decision 11), so the label falls back to `width.min`. That only leaves room
    /// for the item when its own `at.x` fits inside the fallback: anchored at x=30 on a 10mm label
    /// there is no box left to draw, and it errors rather than silently disappearing. Pins the §6
    /// wording.
    #[test]
    fn a_to_sized_qr_anchored_past_the_fallback_width_errors() {
        let qr_at = |x: f32| TemplateDefinition {
            id: "t".to_string(),
            name: "T".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 180,
            format: TemplateFormat::Single {
                width: Dimension::Dynamic {
                    min: Some(10.0),
                    max: Some(100.0),
                },
                height: Dimension::Fixed(12.0),
                media_width: None,
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Qr {
                name: None,
                value: Some("payload".to_string()),
                placement: Placement {
                    at: Position([x, 0.0]),
                    extent: crate::models::Extent::To(Position([-0.0, 12.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                params: None,
            }]),
            version: None,
        };
        let data: HashMap<String, super::JsonValue> = HashMap::new();

        let flush_left = qr_at(0.0);
        assert_eq!(flush_left.validate(), Ok(()));
        render_single_label(&flush_left, &data, None, &no_settings(), &no_datetime())
            .expect("from x=0 the fallback width is the whole box");

        let template = qr_at(30.0);
        assert_eq!(template.validate(), Ok(()), "valid against the 100mm max");
        let err = render_single_label(&template, &data, None, &no_settings(), &no_datetime())
            .expect_err("a 30mm anchor on a 10mm label leaves no box");
        assert!(
            err.message_text().contains("above and to the right"),
            "unexpected error: {}",
            err.message_text()
        );
    }

    /// The box spans from x=0 to the frame's right edge, so a centered line centers on the label.
    #[test]
    fn a_to_box_renders_at_the_full_frame_width() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (40.0, 12.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        let source = ctx
            .render_items(&[LayoutItem::Text {
                name: None,
                value: Some("x".to_string()),
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    extent: crate::models::Extent::To(Position([-0.0, 12.0])),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(6.0),
                font_weight: None,
                multiline: false,
                alignment: crate::models::Alignment::default(),
            }])
            .expect("render");
        assert!(
            source.contains("width: 40mm"),
            "expected a full-width box, got: {source}"
        );
    }

    /// #150: the measure pass and the render pass must resolve the *same* vertical slot for the same
    /// placement. `measure_box_height` ignored `max_h` while `render_text_item` honored it, so the
    /// fitter chose a font for a taller box than the text landed in.
    #[test]
    fn measure_and_render_resolve_the_same_slot_height() {
        use std::cell::RefCell;
        let data: HashMap<String, super::JsonValue> = HashMap::new();
        let settings = no_settings();
        let datetime = no_datetime();
        let env = super::RenderEnv {
            settings: &settings,
            datetime: &datetime,
        };
        let images = RefCell::new(super::ImageCollector::default());
        let ctx = super::RenderContext::new(
            (80.0, 40.0),
            "mm",
            &data,
            None,
            &env,
            &images,
            super::LengthMode::Fixed,
        );
        // An auto height with a max_h well below the frame remainder above at.y.
        let placement = Placement {
            at: Position([0.0, 0.0]),
            extent: crate::models::Extent::Size(Size([
                SizeValue::Value(20.0),
                SizeValue::Auto(crate::models::AutoSize::Auto),
            ])),
            max_w: None,
            max_h: Some(6.0),
            rotate: None,
        };
        let measured = ctx
            .measure_box_height(&placement, 0.0)
            .expect("measure height");
        // What `render_text_item` resolves for the same slot.
        let rendered = ctx
            .resolve_size_value(
                &Size([
                    SizeValue::Value(20.0),
                    SizeValue::Auto(crate::models::AutoSize::Auto),
                ])
                .0[1],
                placement.max_h,
                Some(40.0 - 0.0),
                "height",
            )
            .expect("render height");
        assert_eq!(
            measured, rendered,
            "measure resolved {measured} and render resolved {rendered} for the same slot"
        );
        assert_eq!(measured, 6.0, "max_h below the frame remainder must bind");
    }
}
