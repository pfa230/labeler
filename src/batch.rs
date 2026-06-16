//! Unified batch rendering (ADR-0011). Renders a list of resolved labels into either a download blob
//! (ZIP for single templates, PDF for sheet) or a set of print artifacts. Pure/sync; the async print
//! dispatch lives in the `/batch` handler.

use std::collections::BTreeMap;
use std::io::Write as _;

use crate::errors::{AppError, BatchFailure};
use crate::models::{LabelInput, TemplateFormat};
use crate::render::{render_sheet_pages, render_single_label, render_single_label_pdf};
use crate::templates::TemplateDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchMode {
    Download,
    Print,
}

/// One print job's bytes plus the label indices it covers (single: one label; sheet: all labels).
#[derive(Debug)]
pub struct PrintUnit {
    pub bytes: Vec<u8>,
    pub indices: Vec<usize>,
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
    settings: &BTreeMap<String, String>,
    max_labels: usize,
) -> Result<RenderedBatch, AppError> {
    if labels.len() > max_labels {
        return Err(AppError::batch_too_large(labels.len(), max_labels));
    }
    if labels.is_empty() {
        return Err(AppError::invalid_request("batch has no labels"));
    }

    match &template.format {
        TemplateFormat::Single { .. } => {
            render_single_batch(template, labels, mode, format, settings)
        }
        TemplateFormat::Sheet { .. } => {
            render_sheet_batch(template, labels, mode, start_slot, settings)
        }
    }
}

fn render_single_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    format: Option<&str>,
    settings: &BTreeMap<String, String>,
) -> Result<RenderedBatch, AppError> {
    let fmt = format.unwrap_or("png");
    let ext: &'static str = match fmt {
        "" | "png" => "png",
        "pdf" => "pdf",
        other => {
            return Err(AppError::invalid_request(format!(
                "unknown format '{other}'; use png or pdf"
            )))
        }
    };

    let mut artifacts: Vec<Vec<u8>> = Vec::with_capacity(labels.len());
    let mut failures: Vec<BatchFailure> = Vec::new();
    for (idx, lbl) in labels.iter().enumerate() {
        let res = match ext {
            "pdf" => render_single_label_pdf(template, &lbl.data, lbl.option.as_ref(), settings),
            _ => render_single_label(template, &lbl.data, lbl.option.as_ref(), settings),
        };
        match res {
            Ok(bytes) => artifacts.push(bytes),
            Err(err) => {
                failures.push(BatchFailure {
                    index: idx,
                    code: err.code(),
                    message: err.message_text(),
                });
                artifacts.push(Vec::new());
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
                .enumerate()
                .map(|(i, bytes)| PrintUnit {
                    bytes,
                    indices: vec![i],
                })
                .collect(),
        }),
        BatchMode::Download => {
            let width = labels.len().to_string().len();
            let mut cursor = std::io::Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (i, bytes) in artifacts.iter().enumerate() {
                let name = format!("{:0width$}.{ext}", i + 1, width = width);
                zip.start_file(name, opts)
                    .map_err(|e| AppError::render_failed(format!("zip error: {e}")))?;
                zip.write_all(bytes)
                    .map_err(|e| AppError::render_failed(format!("zip error: {e}")))?;
            }
            zip.finish()
                .map_err(|e| AppError::render_failed(format!("zip error: {e}")))?;
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
    settings: &BTreeMap<String, String>,
) -> Result<RenderedBatch, AppError> {
    let pdf = render_sheet_pages(template, labels, start_slot, settings)?;
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
            }],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Alignment, Dimension, FontSize, Layout, LayoutItem, Placement, Position, Size, SizeValue,
    };
    use serde_json::json;
    use std::collections::HashMap;

    fn single_tpl() -> TemplateDefinition {
        TemplateDefinition {
            id: "s".to_string(),
            name: "S".to_string(),
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
                    size: Size([SizeValue::Value(20.0), SizeValue::Value(8.0)]),
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

    fn lbl(msg: &str) -> LabelInput {
        LabelInput {
            data: HashMap::from([("message".to_string(), json!(msg))]),
            option: None,
        }
    }

    fn no_settings() -> BTreeMap<String, String> {
        BTreeMap::new()
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
            &no_settings(),
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
            &no_settings(),
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

    #[test]
    fn bad_label_is_batch_invalid_with_index() {
        let labels = vec![
            lbl("a"),
            LabelInput {
                data: HashMap::new(),
                option: None,
            },
        ];
        let err = render_batch(
            &single_tpl(),
            &labels,
            BatchMode::Download,
            Some("png"),
            0,
            &no_settings(),
            500,
        )
        .unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
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
            &no_settings(),
            1,
        )
        .unwrap_err();
        assert_eq!(err.code(), "BatchTooLarge");
    }
}
