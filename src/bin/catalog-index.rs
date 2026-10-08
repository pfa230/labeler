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

/// Every file under `dir`, except the index the generator writes at `root/index.json`.
fn catalog_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            catalog_files(root, &path, out);
        } else if path != root.join("index.json") {
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
    TemplateDefinition { id, content }
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

/// The index entries for every template under `root`, in ascending path order.
fn build_index(root: &Path) -> Vec<serde_json::Value> {
    let mut files = Vec::new();
    catalog_files(root, root, &mut files);
    files.sort();

    let mut entries = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(root).expect("under catalog");
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        // catalog/<category>/<vendor>/<file>.yaml, or catalog/<category>/<file>.yaml for examples.
        // Fail on any other shape: a file dropped directly in catalog/ would otherwise be indexed
        // with its own filename as the category, a deeper path would silently lose components, and
        // a file that is not `.yaml` would silently never be installable.
        let (category, vendor) = match parts.len() {
            2 if parts[1].ends_with(".yaml") => (parts[0].clone(), None),
            3 if parts[2].ends_with(".yaml") => (parts[0].clone(), Some(parts[1].clone())),
            _ => panic!(
                "{}: expected catalog/<category>/<file>.yaml or catalog/<category>/<vendor>/<file>.yaml",
                rel.display()
            ),
        };
        let template = load(path);
        entries.push(serde_json::json!({
            "id": template.id,
            "name": template.name,
            "description": template.description,
            "path": rel.to_string_lossy(),
            "category": category,
            "vendor": vendor,
            "format": format_kind(&template),
            "media_width_mm": media_width(&template),
            "fields": template.params.keys().collect::<Vec<_>>(),
        }));
    }
    entries
}

fn main() {
    let root = Path::new("catalog");
    let entries = build_index(root);
    let json = serde_json::to_string_pretty(&entries).expect("serialize") + "\n";
    std::fs::write(root.join("index.json"), json).expect("write catalog/index.json");
    eprintln!("wrote catalog/index.json ({} entries)", entries.len());
}

#[cfg(test)]
mod tests {
    use super::build_index;
    use std::path::{Path, PathBuf};

    const MINIMAL_TEMPLATE: &str = "name: T\nunit: mm\ndpi: 300\nformat:\n  type: single\n  width: 20.0\n  height: 10.0\nlayout: []\n";

    /// A fresh catalog root holding `files` (relative path, contents).
    fn catalog_with(label: &str, files: &[(&str, &str)]) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "labeler_catalog_index_{label}_{}_{unique}",
            std::process::id()
        ));
        for (rel, contents) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        root
    }

    /// The panic message `build_index` fails with on `root`.
    fn build_index_panic(root: &Path) -> String {
        let payload = std::panic::catch_unwind(|| build_index(root))
            .expect_err("build_index must refuse this catalog");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .expect("string panic payload")
    }

    #[test]
    fn fields_lists_every_declared_parameter_in_declaration_order() {
        let template = r#"name: Fields
unit: mm
dpi: 300
params:
  - name: zeta
    type: string
  - name: alpha
    type: string
    default: A
  - name: mid
    type: string
format:
  type: single
  width: 20.0
  height: 10.0
layout:
  - type: text
    value: "{alpha} {mid}"
    at: [0.0, 0.0]
    size: [20.0, 5.0]
    font_size: 10.0
"#;
        let root = catalog_with("fields", &[("tape/fields.yaml", template)]);

        let entries = build_index(&root);

        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["fields"],
            serde_json::json!(["zeta", "alpha", "mid"])
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_that_is_not_yaml_fails_naming_its_path() {
        for (label, rel) in [("yml", "tape/x.yml"), ("txt", "tape/notes.txt")] {
            let root = catalog_with(label, &[(rel, MINIMAL_TEMPLATE)]);
            let message = build_index_panic(&root);
            assert!(message.contains(rel), "{rel}: {message}");
            std::fs::remove_dir_all(&root).ok();
        }
    }

    /// Regression guard: the depth check already panicked before #411; this pins it once it moves
    /// into the path-shape check that also refuses non-`.yaml` files.
    #[test]
    fn a_yaml_file_at_the_wrong_depth_fails_naming_its_path() {
        for (label, rel) in [("shallow", "x.yaml"), ("deep", "tape/brother/extra/x.yaml")] {
            let root = catalog_with(label, &[(rel, MINIMAL_TEMPLATE)]);
            let message = build_index_panic(&root);
            assert!(message.contains(rel), "{rel}: {message}");
            std::fs::remove_dir_all(&root).ok();
        }
    }
}
