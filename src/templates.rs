use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
};
use thiserror::Error;

use crate::models::{FieldSpec, OptionDetail, TemplateDetail, TemplateFormat, TemplateSummary};

#[derive(Debug, Deserialize, Clone)]
pub struct TemplateDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub format: TemplateFormat,
    #[serde(default)]
    pub options: HashMap<String, OptionDetail>,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug)]
pub struct TemplateRegistry {
    templates: HashMap<String, TemplateDefinition>,
}

impl TemplateRegistry {
    pub fn load_from_dir<P: AsRef<FsPath>>(dir: P) -> Result<Self, TemplateRegistryError> {
        let dir = dir.as_ref();
        let mut templates = HashMap::new();
        let mut seen_paths: HashMap<String, PathBuf> = HashMap::new();
        let entries = std::fs::read_dir(dir)
            .map_err(|source| TemplateRegistryError::Io {
                path: dir.to_path_buf(),
                source,
            })?;

        for entry in entries {
            let entry = entry.map_err(|source| TemplateRegistryError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("yaml") | Some("yml")) {
                continue;
            }

            let contents = std::fs::read_to_string(&path).map_err(|source| {
                TemplateRegistryError::Io {
                    path: path.clone(),
                    source,
                }
            })?;
            let template: TemplateDefinition =
                serde_yaml::from_str(&contents).map_err(|source| TemplateRegistryError::Yaml {
                    path: path.clone(),
                    source,
                })?;
            template
                .validate()
                .map_err(|message| TemplateRegistryError::Validation {
                    path: path.clone(),
                    message,
                })?;

            if let Some(existing_path) = seen_paths.get(&template.id) {
                return Err(TemplateRegistryError::DuplicateId {
                    id: template.id.clone(),
                    first: existing_path.clone(),
                    second: path,
                });
            }

            seen_paths.insert(template.id.clone(), path);
            templates.insert(template.id.clone(), template);
        }

        Ok(Self { templates })
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn get(&self, id: &str) -> Option<&TemplateDefinition> {
        self.templates.get(id)
    }

    pub fn summaries(&self) -> Vec<TemplateSummary> {
        let mut items: Vec<_> = self.templates.values().map(TemplateSummary::from).collect();
        items.sort_by(|a, b| a.id.cmp(&b.id));
        items
    }

    pub fn detail(&self, id: &str) -> Option<TemplateDetail> {
        self.templates.get(id).map(TemplateDetail::from)
    }
}

#[derive(Debug, Error)]
pub enum TemplateRegistryError {
    #[error("failed to read templates from {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("failed to parse template {path}: {source}")]
    Yaml { path: PathBuf, source: serde_yaml::Error },
    #[error("template {path} failed validation: {message}")]
    Validation { path: PathBuf, message: String },
    #[error("duplicate template id '{id}' found in {first} and {second}")]
    DuplicateId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
}

impl TemplateDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("id must not be empty".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".to_string());
        }

        let mut seen_fields = HashSet::new();
        for field in &self.fields {
            if field.name.trim().is_empty() {
                return Err("field name must not be empty".to_string());
            }
            if !seen_fields.insert(field.name.as_str()) {
                return Err(format!("duplicate field name '{}'", field.name));
            }
        }

        for (name, option) in &self.options {
            if name.trim().is_empty() {
                return Err("option name must not be empty".to_string());
            }
            if option.values.is_empty() {
                return Err(format!("option '{}' must define values", name));
            }
            if !option.values.contains(&option.default) {
                return Err(format!(
                    "option '{}' default '{}' not in values",
                    name, option.default
                ));
            }
        }

        match self.format {
            TemplateFormat::Sheet {
                labels_per_sheet, ..
            } if labels_per_sheet == 0 => {
                return Err("labels_per_sheet must be greater than 0".to_string());
            }
            TemplateFormat::Continuous { width_mm } if width_mm <= 0.0 => {
                return Err("width_mm must be greater than 0".to_string());
            }
            _ => {}
        }

        Ok(())
    }
}

impl From<&TemplateDefinition> for TemplateSummary {
    fn from(template: &TemplateDefinition) -> Self {
        let options = template
            .options
            .iter()
            .map(|(name, option)| (name.clone(), option.values.clone()))
            .collect();
        Self {
            id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            options,
            format: template.format.clone(),
        }
    }
}

impl From<&TemplateDefinition> for TemplateDetail {
    fn from(template: &TemplateDefinition) -> Self {
        Self {
            id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            format: template.format.clone(),
            options: template.options.clone(),
            fields: template.fields.clone(),
            version: template.version.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TemplateDefinition, TemplateRegistry};
    use crate::models::{FieldSpec, OptionDetail, TemplateFormat};
    use std::{collections::HashMap, fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    fn temp_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("labeler_test_{label}_{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_template(dir: &PathBuf, name: &str, contents: &str) {
        let path = dir.join(name);
        fs::write(&path, contents).expect("write template");
    }

    #[test]
    fn validate_rejects_empty_id() {
        let template = TemplateDefinition {
            id: " ".to_string(),
            name: "Label".to_string(),
            description: "desc".to_string(),
            format: TemplateFormat::Continuous { width_mm: 12.0 },
            options: HashMap::new(),
            fields: Vec::new(),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("id must not be empty"));
    }

    #[test]
    fn validate_rejects_invalid_option_default() {
        let template = TemplateDefinition {
            id: "test".to_string(),
            name: "Label".to_string(),
            description: "desc".to_string(),
            format: TemplateFormat::Continuous { width_mm: 12.0 },
            options: HashMap::from([(
                "color".to_string(),
                OptionDetail {
                    values: vec!["red".to_string()],
                    default: "blue".to_string(),
                },
            )]),
            fields: Vec::new(),
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("default"));
    }

    #[test]
    fn load_from_dir_reads_templates() {
        let dir = temp_dir("load");
        write_template(
            &dir,
            "sample.yaml",
            r#"
id: sample
name: Sample
description: Sample template
format:
  type: continuous
  width_mm: 12.0
fields:
  - name: message
    type: string
    max_length: 50
"#,
        );

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        assert_eq!(registry.len(), 1);
        assert!(registry.get("sample").is_some());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn summaries_are_sorted_by_id() {
        let dir = temp_dir("sorted");
        write_template(
            &dir,
            "b.yaml",
            r#"
id: b
name: B
description: B
format:
  type: continuous
  width_mm: 12.0
fields: []
"#,
        );
        write_template(
            &dir,
            "a.yaml",
            r#"
id: a
name: A
description: A
format:
  type: continuous
  width_mm: 12.0
fields: []
"#,
        );

        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        let summaries = registry.summaries();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, "a");
        assert_eq!(summaries[1].id, "b");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_duplicate_field_names() {
        let template = TemplateDefinition {
            id: "dup".to_string(),
            name: "dup".to_string(),
            description: "dup".to_string(),
            format: TemplateFormat::Continuous { width_mm: 12.0 },
            options: HashMap::new(),
            fields: vec![
                FieldSpec {
                    name: "value".to_string(),
                    field_type: "string".to_string(),
                    max_length: None,
                    multiline: None,
                },
                FieldSpec {
                    name: "value".to_string(),
                    field_type: "string".to_string(),
                    max_length: None,
                    multiline: None,
                },
            ],
            version: None,
        };
        let err = template.validate().expect_err("expected error");
        assert!(err.contains("duplicate field name"));
    }
}
