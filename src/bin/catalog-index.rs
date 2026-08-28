//! Regenerates `catalog/index.json`, the file the UI fetches to browse installable templates (#137).
//!
//! Run from the repo root: `cargo run --bin catalog-index`. CI runs the same command and fails if the
//! committed index differs, so a hand-edited or stale index cannot survive review.
//!
//! Reads `catalog/` at run time — deliberately no `include_str!`/`include_dir!` — so the Docker build
//! needs no catalog and the release binary embeds nothing. Parsing and validation go through the same
//! `parse_template` + `validate` the server uses, so the index cannot describe a template the server
//! would reject.

use labeler::models::TemplateFormat;
use labeler::parse::parse_template;
use labeler::templates::TemplateDefinition;
use std::path::{Path, PathBuf};

fn yaml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            yaml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
            // Filters by extension, so index.json itself and any future non-template file are
            // skipped rather than parsed.
            out.push(path);
        }
    }
}

fn load(path: &Path) -> TemplateDefinition {
    let yaml = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let content = parse_template(&yaml).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    content
        .validate()
        .unwrap_or_else(|m| panic!("{}: {m}", path.display()));
    let id = path
        .file_stem()
        .expect("file stem")
        .to_str()
        .expect("valid utf-8 id")
        .to_string();
    TemplateDefinition {
        id,
        group: None,
        content,
    }
}

fn format_kind(t: &TemplateDefinition) -> &'static str {
    match t.format {
        TemplateFormat::Single { .. } => "single",
        TemplateFormat::Sheet { .. } => "sheet",
    }
}

fn media_width(t: &TemplateDefinition) -> Option<f32> {
    match t.format {
        TemplateFormat::Single { media_width, .. } => media_width,
        TemplateFormat::Sheet { .. } => None,
    }
}

fn main() {
    let root = Path::new("catalog");
    let mut files = Vec::new();
    yaml_files(root, &mut files);
    files.sort();

    let mut entries = Vec::new();
    for path in &files {
        let template = load(path);
        let rel = path.strip_prefix(root).expect("under catalog");
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        // catalog/<category>/<vendor>/<file>.yaml, or catalog/<category>/<file>.yaml for examples.
        // Fail on any other shape: a file dropped directly in catalog/ would otherwise be indexed
        // with its own filename as the category, and a deeper path would silently lose components.
        let (category, vendor) = match parts.len() {
            2 => (parts[0].clone(), None),
            3 => (parts[0].clone(), Some(parts[1].clone())),
            _ => panic!(
                "{}: expected catalog/<category>/<file>.yaml or catalog/<category>/<vendor>/<file>.yaml",
                rel.display()
            ),
        };
        entries.push(serde_json::json!({
            "id": template.id,
            "name": template.name,
            "description": template.description,
            "path": rel.to_string_lossy(),
            "category": category,
            "vendor": vendor,
            "format": format_kind(&template),
            "media_width_mm": media_width(&template),
            "fields": template
                .inputs_all()
                .into_iter()
                .filter(|i| i.required)
                .map(|i| i.name)
                .collect::<Vec<_>>(),
        }));
    }

    let json = serde_json::to_string_pretty(&entries).expect("serialize") + "\n";
    std::fs::write(root.join("index.json"), json).expect("write catalog/index.json");
    eprintln!("wrote catalog/index.json ({} entries)", entries.len());
}
