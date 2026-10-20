//! Unified batch rendering. Renders a list of resolved labels into either a download blob
//! (ZIP for single templates, PDF for sheet) or a set of print artifacts. Pure/sync; the async print
//! dispatch lives in `api::rendering::run_batch`, behind `/print`.

use std::io::Write as _;
use std::num::NonZeroU32;
use std::ops::Range;

use crate::errors::{AppError, BatchFailure};
use crate::models::{LabelInput, TemplateFormat};
use crate::reason::Reason;
use crate::render::{render_sheet_pages, render_single_label_as, RenderedSingle, SingleKind};
use crate::templates::TemplateDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchMode {
    Download,
    Print,
}

/// The request's resolved render environment and image render options, threaded through batch
/// rendering.
pub struct BatchEnv<'a> {
    pub render: &'a crate::render::RenderEnv<'a>,
    pub render_opts: crate::render::ImageRenderOptions,
}

/// One print job's bytes plus the label indices it covers (single: one entry's labels; sheet: all
/// labels), the IPP `copies` it asks for (single: the entry's count; sheet: 1), and for a single
/// label the length its job declares along the feed, in hundredths of a millimetre.
#[derive(Debug)]
pub struct PrintUnit {
    pub bytes: Vec<u8>,
    pub indices: Vec<usize>,
    pub copies: u32,
    pub y_dimension: Option<i32>,
}

#[derive(Debug)]
pub enum RenderedBatch {
    Download {
        bytes: Vec<u8>,
        content_type: &'static str,
        filename: String,
    },
    Print {
        units: Vec<PrintUnit>,
    },
}

pub fn render_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    format: Option<&str>,
    start_slot: u32,
    env: &BatchEnv,
    max_labels: usize,
) -> Result<RenderedBatch, AppError> {
    // Counted before anything is expanded or rendered, so a huge `copies` costs nothing.
    let within_cap = labels.iter().try_fold(0usize, |n, l| {
        n.checked_add(l.copies.get() as usize)
            .filter(|&n| n <= max_labels)
    });
    if within_cap.is_none() {
        return Err(AppError::batch_too_large(max_labels));
    }
    if labels.is_empty() {
        return Err(AppError::invalid_request(
            Reason::BatchEmpty,
            "batch has no labels",
        ));
    }

    match &template.format {
        TemplateFormat::Single { .. } => render_single_batch(template, labels, mode, format, env),
        TemplateFormat::Sheet { .. } => render_sheet_batch(template, labels, mode, start_slot, env),
    }
}

fn render_single_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    format: Option<&str>,
    env: &BatchEnv,
) -> Result<RenderedBatch, AppError> {
    let ext: &'static str = match format.unwrap_or("png") {
        "png" => "png",
        "pdf" => "pdf",
        other => {
            return Err(AppError::invalid_request(
                Reason::FormatUnknown,
                format!("unknown format '{other}'; use png or pdf"),
            ))
        }
    };

    let kind = if ext == "pdf" {
        SingleKind::Pdf
    } else {
        SingleKind::Png
    };
    // Each entry renders once and covers the next `copies` label indices.
    let mut artifacts: Vec<(RenderedSingle, Range<usize>, u32)> = Vec::with_capacity(labels.len());
    let mut failures: Vec<BatchFailure> = Vec::new();
    let mut next = 0;
    for lbl in labels {
        let copies = lbl.copies.get();
        let indices = next..next + copies as usize;
        next = indices.end;
        let res = crate::render::validate_label_data_keys(template, &lbl.data).and_then(|()| {
            render_single_label_as(template, &lbl.data, env.render, kind, env.render_opts)
        });
        match res {
            Ok(label) => artifacts.push((label, indices, copies)),
            Err(err) => {
                let failure = BatchFailure::new(indices.start, err);
                failures.extend(indices.map(|index| BatchFailure {
                    index,
                    ..failure.clone()
                }));
            }
        }
    }
    if !failures.is_empty() {
        return Err(AppError::batch_invalid(failures));
    }

    match mode {
        BatchMode::Print => Ok(RenderedBatch::Print {
            units: artifacts
                .into_iter()
                .map(|(label, indices, copies)| PrintUnit {
                    bytes: label.bytes,
                    indices: indices.collect(),
                    copies,
                    y_dimension: Some(label.y_dimension),
                })
                .collect(),
        }),
        BatchMode::Download => {
            let width = next.to_string().len();
            let mut cursor = std::io::Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (label, indices, _) in &artifacts {
                for i in indices.clone() {
                    let name = format!("{:0width$}.{ext}", i + 1, width = width);
                    zip.start_file(name, opts)
                        .map_err(|e| AppError::internal(format!("zip error: {e}")))?;
                    zip.write_all(&label.bytes)
                        .map_err(|e| AppError::internal(format!("zip error: {e}")))?;
                }
            }
            zip.finish()
                .map_err(|e| AppError::internal(format!("zip error: {e}")))?;
            Ok(RenderedBatch::Download {
                bytes: cursor.into_inner(),
                content_type: "application/zip",
                filename: format!("{}.zip", template.id),
            })
        }
    }
}

fn render_sheet_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    start_slot: u32,
    env: &BatchEnv,
) -> Result<RenderedBatch, AppError> {
    // Every slot is drawn, so the sheet takes the entries expanded into labels.
    let labels: Vec<LabelInput> = labels
        .iter()
        .flat_map(|l| {
            std::iter::repeat_n(
                LabelInput {
                    data: l.data.clone(),
                    copies: NonZeroU32::MIN,
                },
                l.copies.get() as usize,
            )
        })
        .collect();
    let pdf = render_sheet_pages(template, &labels, start_slot, env.render)?;
    match mode {
        BatchMode::Download => Ok(RenderedBatch::Download {
            bytes: pdf,
            content_type: "application/pdf",
            filename: format!("{}.pdf", template.id),
        }),
        BatchMode::Print => Ok(RenderedBatch::Print {
            units: vec![PrintUnit {
                bytes: pdf,
                indices: (0..labels.len()).collect(),
                copies: 1,
                y_dimension: None,
            }],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Alignment, Dimension, FontSize, Layout, LayoutItem, Overflow, Placement, Position, Size,
        SizeValue,
    };
    use crate::templates::TemplateContent;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::collections::HashMap;

    fn single_tpl() -> TemplateDefinition {
        TemplateDefinition {
            id: "s".to_string(),
            content: TemplateContent {
                name: "S".to_string(),
                description: String::new(),
                categories: Vec::new(),
                unit: "mm".to_string(),
                dpi: 200,
                format: TemplateFormat::Single {
                    width: Dimension::Fixed(20.0).into(),
                    height: 10.0.into(),
                    media_width: None,
                },
                params: indexmap::IndexMap::from([(
                    "message".to_string(),
                    crate::models::ParamSpec {
                        param_type: crate::models::ParamType::String { multiline: false },
                        default: None,
                        min: None,
                        max: None,
                        description: None,
                        default_instant: None,
                    },
                )]),
                layout: Layout::Items(vec![LayoutItem::Text {
                    value: "{message}".to_string(),
                    placement: Placement::sized(
                        Position([0.0, 0.0]),
                        Size([SizeValue::fixed(20.0), SizeValue::fixed(8.0)]),
                    ),
                    font_size: FontSize::Fixed(8.0),
                    font_weight: None,
                    color: None,
                    wrap: false,
                    line_spacing: None,
                    alignment: Alignment::default(),
                    overflow: Overflow::Ellipsis,
                    when: None,
                }]),
            },
        }
    }

    fn lbl(msg: &str) -> LabelInput {
        LabelInput {
            data: HashMap::from([("message".to_string(), json!(msg))]),
            copies: NonZeroU32::MIN,
        }
    }

    fn no_env() -> BatchEnv<'static> {
        use std::sync::OnceLock;
        static EMPTY_SETTINGS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
        static EMPTY_FORMATS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
        static DT: OnceLock<crate::datetime_fmt::DateTimeResolver<'static>> = OnceLock::new();
        static ENV: OnceLock<crate::render::RenderEnv<'static>> = OnceLock::new();
        let settings = EMPTY_SETTINGS.get_or_init(BTreeMap::new);
        let formats = EMPTY_FORMATS.get_or_init(BTreeMap::new);
        let datetime = DT.get_or_init(|| crate::datetime_fmt::DateTimeResolver {
            formats,
            now: chrono::Local::now(),
        });
        let render = ENV.get_or_init(|| {
            crate::render::resolve_environment(&single_tpl(), settings, datetime)
                .expect("the single template reads no variables")
        });
        BatchEnv {
            render,
            render_opts: crate::render::ImageRenderOptions::default(),
        }
    }

    #[test]
    fn single_download_zips_each_label() {
        let labels = vec![lbl("a"), lbl("b")];
        let out = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Download,
            Some("png"),
            0,
            &no_env(),
            500,
        )
        .unwrap();
        match out {
            RenderedBatch::Download {
                bytes,
                content_type,
                ..
            } => {
                assert_eq!(content_type, "application/zip");
                assert_eq!(&bytes[..4], b"PK\x03\x04");
            }
            _ => panic!("expected download"),
        }
    }

    #[test]
    fn single_print_one_unit_per_label() {
        let labels = vec![lbl("a"), lbl("b"), lbl("c")];
        let out = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Print,
            None,
            0,
            &no_env(),
            500,
        )
        .unwrap();
        match out {
            RenderedBatch::Print { units } => {
                assert_eq!(units.len(), 3);
                assert_eq!(units[1].indices, vec![1]);
            }
            _ => panic!("expected print"),
        }
    }

    /// Print units for `labels` of the registry's `id` template, which must read no variables.
    fn print_units(id: &str, labels: &[LabelInput]) -> Vec<PrintUnit> {
        let registry = crate::templates::load_all_for_tests().0;
        let template = registry.get(id).expect("fixture template");
        let (settings, formats) = (BTreeMap::new(), BTreeMap::new());
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now: chrono::Local::now(),
        };
        let render = crate::render::resolve_environment(template, &settings, &datetime).unwrap();
        let env = BatchEnv {
            render: &render,
            render_opts: crate::render::ImageRenderOptions::default(),
        };
        match render_batch(
            template,
            labels,
            BatchMode::Print,
            Some("png"),
            0,
            &env,
            500,
        )
        .unwrap()
        {
            RenderedBatch::Print { units } => units,
            _ => panic!("expected print"),
        }
    }

    #[test]
    fn single_print_units_carry_each_labels_width() {
        let label = |message: &str| LabelInput {
            data: HashMap::from([
                ("message".to_string(), json!(message)),
                ("code".to_string(), json!("Q")),
            ]),
            copies: NonZeroU32::MIN,
        };
        let units = print_units(
            "brother_24mm_qr",
            &[label("a"), label("a much longer message on the tape")],
        );
        let lengths: Vec<i32> = units
            .iter()
            .map(|u| u.y_dimension.expect("single length"))
            .collect();
        assert!(lengths[1] > lengths[0], "{lengths:?}");
    }

    #[test]
    fn a_sheet_print_unit_carries_no_width() {
        let label = LabelInput {
            data: HashMap::from([
                ("id".to_string(), json!("A1")),
                ("url".to_string(), json!("https://example.com/A1")),
                ("name".to_string(), json!("Grinder")),
                ("tags".to_string(), json!("Power tools")),
                ("description".to_string(), json!("Angle grinder")),
            ]),
            copies: NonZeroU32::MIN,
        };
        let units = print_units("avery5163_asset_tag", &[label.clone(), label]);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].indices, vec![0, 1]);
        assert_eq!(units[0].y_dimension, None);
    }

    #[test]
    fn bad_label_is_batch_invalid_with_index() {
        let labels = vec![
            lbl("a"),
            LabelInput {
                data: HashMap::from([("message".to_string(), json!(["a", "b"]))]),
                copies: NonZeroU32::MIN,
            },
        ];
        let err = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Download,
            Some("png"),
            0,
            &no_env(),
            500,
        )
        .unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
    }

    #[test]
    fn single_print_renders_requested_format() {
        let labels = vec![lbl("a")];
        let out = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Print,
            Some("pdf"),
            0,
            &no_env(),
            500,
        )
        .unwrap();
        let RenderedBatch::Print { units } = out else {
            panic!("expected print")
        };
        assert!(
            units[0].bytes.starts_with(b"%PDF"),
            "pdf driver format must yield PDF bytes"
        );
        let out = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Print,
            Some("png"),
            0,
            &no_env(),
            500,
        )
        .unwrap();
        let RenderedBatch::Print { units } = out else {
            panic!("expected print")
        };
        assert_eq!(
            &units[0].bytes[..8],
            b"\x89PNG\r\n\x1a\n",
            "png driver format must yield PNG bytes"
        );
    }

    #[test]
    fn empty_batch_is_invalid_request() {
        let labels: Vec<LabelInput> = vec![];
        let err = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Download,
            Some("png"),
            0,
            &no_env(),
            500,
        )
        .unwrap_err();
        assert_eq!(err.code(), "InvalidRequest");
    }

    fn copies(label: LabelInput, n: u32) -> LabelInput {
        LabelInput {
            copies: NonZeroU32::new(n).unwrap(),
            ..label
        }
    }

    #[test]
    fn a_copies_entry_is_one_print_unit_over_its_labels() {
        let labels = vec![lbl("a"), copies(lbl("b"), 3)];
        let RenderedBatch::Print { units } = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Print,
            None,
            0,
            &no_env(),
            500,
        )
        .unwrap() else {
            panic!("expected print")
        };
        let shape: Vec<(Vec<usize>, u32)> = units
            .iter()
            .map(|u| (u.indices.clone(), u.copies))
            .collect();
        assert_eq!(shape, vec![(vec![0], 1), (vec![1, 2, 3], 3)]);
    }

    #[test]
    fn copies_fill_sheet_slots_like_repeated_entries() {
        let mut sheet = single_tpl();
        sheet.content.format = TemplateFormat::Sheet {
            paper_width: 120.0,
            paper_height: 40.0,
            label_width: 20.0,
            label_height: 10.0,
            shape: Default::default(),
            rounded: None,
            positions: (0..24)
                .map(|i| {
                    crate::models::SheetPosition([(i % 6) as f32 * 20.0, (i / 6) as f32 * 10.0])
                })
                .collect(),
        };
        let pdf = |labels: &[LabelInput]| match render_batch(
            &sheet,
            labels,
            BatchMode::Download,
            None,
            22,
            &no_env(),
            500,
        )
        .unwrap()
        {
            RenderedBatch::Download { bytes, .. } => bytes,
            _ => panic!("expected download"),
        };
        let with_copies = pdf(&[copies(lbl("A"), 3), lbl("B")]);
        assert!(
            with_copies == pdf(&[lbl("A"), lbl("A"), lbl("A"), lbl("B")]),
            "copies must fill the slots the repeated entries fill"
        );
        assert_eq!(crate::render::count_pdf_pages(&with_copies), 2);
    }

    #[test]
    fn over_cap_is_too_large() {
        let labels = vec![lbl("a"), lbl("b")];
        let err = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Download,
            Some("png"),
            0,
            &no_env(),
            1,
        )
        .unwrap_err();
        assert_eq!(err.code(), "PayloadTooLarge");
    }
}
