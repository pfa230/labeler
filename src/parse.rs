use serde_yaml_ng::Value;

use crate::errors::TemplateError;
use crate::raw::TemplateDefinitionRaw;
use crate::templates::TemplateContent;

pub fn parse_template(src: &str) -> Result<TemplateContent, TemplateError> {
    let document: Value = serde_yaml_ng::from_str(src).map_err(|err| TemplateError::Yaml {
        path: String::new(),
        msg: err.to_string(),
    })?;
    refuse_nulls_and_nans(&document, "")?;

    let deserializer = serde_yaml_ng::Deserializer::from_str(src);
    let raw: TemplateDefinitionRaw =
        serde_path_to_error::deserialize(deserializer).map_err(|err| TemplateError::Yaml {
            path: err.path().to_string(),
            msg: err.into_inner().to_string(),
        })?;

    TemplateContent::try_from(raw)
}

/// Refuse the first mapping entry, at any depth, whose value is `null`, and the first number, at
/// any depth, that is NaN. One walk of the document instead of a check on every field, which each
/// new field would have to remember; NaN in particular slips past every `<=` bound. A `null`
/// sequence element is not a key and is left to the typed deserializer.
fn refuse_nulls_and_nans(value: &Value, path: &str) -> Result<(), TemplateError> {
    match value {
        Value::Mapping(mapping) => {
            for (key, entry) in mapping {
                let key = match key {
                    Value::String(key) => key.clone(),
                    other => serde_yaml_ng::to_string(other)
                        .map_err(|err| TemplateError::Yaml {
                            path: path.to_string(),
                            msg: err.to_string(),
                        })?
                        .trim_end()
                        .to_string(),
                };
                let entry_path = if path.is_empty() {
                    key
                } else {
                    format!("{path}.{key}")
                };
                if entry.is_null() {
                    return Err(TemplateError::Validation {
                        path: entry_path,
                        msg: "key must not be null".to_string(),
                    });
                }
                refuse_nulls_and_nans(entry, &entry_path)?;
            }
            Ok(())
        }
        Value::Sequence(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(idx, item)| refuse_nulls_and_nans(item, &format!("{path}[{idx}]"))),
        Value::Tagged(tagged) => refuse_nulls_and_nans(&tagged.value, path),
        Value::Number(number) if number.is_nan() => Err(TemplateError::Validation {
            path: path.to_string(),
            msg: "number must not be NaN".to_string(),
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_template;
    use crate::errors::TemplateError;
    use crate::models::{Layout, LayoutItem, Padding};

    /// Parse `src`, a YAML sequence of layout items, as the layout of an otherwise minimal template.
    fn parse_nodes(src: &str) -> Result<Vec<LayoutItem>, TemplateError> {
        let yaml = format!(
            "name: T\nunit: mm\ndpi: 200\nformat: {{ type: single, width: 10, height: 10 }}\nlayout:{src}"
        );
        let Layout::Items(items) = parse_template(&yaml)?.layout;
        Ok(items)
    }

    #[test]
    fn parse_nodes_accepts_uniform_padding() {
        let src = r#"
- type: container
  at: [0.2, 0.2]
  size: [1.0, 1.0]
  padding: 0.06
  items: []
"#;

        let items = parse_nodes(src).expect("parse nodes");
        let LayoutItem::Container { padding, .. } = &items[0] else {
            panic!("expected container");
        };

        assert_eq!(
            *padding,
            Padding {
                top: 0.06,
                right: 0.06,
                bottom: 0.06,
                left: 0.06,
            }
        );
    }

    #[test]
    fn parse_nodes_accepts_trbl_padding() {
        let src = r#"
- type: container
  at: [0.2, 0.2]
  size: [1.0, 1.0]
  padding: [0.05, 0.08, 0.05, 0.08]
  items: []
"#;

        let items = parse_nodes(src).expect("parse nodes");
        let LayoutItem::Container { padding, .. } = &items[0] else {
            panic!("expected container");
        };

        assert_eq!(
            *padding,
            Padding {
                top: 0.05,
                right: 0.08,
                bottom: 0.05,
                left: 0.08,
            }
        );
    }

    #[test]
    fn parse_nodes_defaults_padding_to_zero() {
        let src = r#"
- type: container
  at: [0.2, 0.2]
  size: [1.0, 1.0]
  items: []
"#;

        let items = parse_nodes(src).expect("parse nodes");
        let LayoutItem::Container { padding, .. } = &items[0] else {
            panic!("expected container");
        };

        assert_eq!(*padding, Padding::ZERO);
    }

    #[test]
    fn parse_nodes_rejects_negative_padding() {
        let src = r#"
- type: container
  at: [0.2, 0.2]
  size: [1.0, 1.0]
  padding: -0.1
  items: []
"#;

        let err = parse_nodes(src).expect_err("expected error");
        match err {
            TemplateError::Validation { path, .. } => {
                assert!(path.ends_with("padding"), "path was {path}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    /// The path has to name a key the author can find. There is no `placement:` key in the wire
    /// format, so an item missing both `size` and `to` reports the item kind, as `require_one_of`
    /// does for `name`/`value`.
    #[test]
    fn parse_nodes_missing_extent_is_reported_at_the_item_kind() {
        let src = r#"
- type: text
  value: hello
  at: [0.0, 0.0]
  font_size: 6
"#;

        let err = parse_nodes(src).expect_err("expected error");
        match err {
            TemplateError::Validation { path, msg } => {
                assert_eq!(path, "layout[0].text");
                assert_eq!(msg, "must set one of size or to");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn parse_nodes_rejects_wrong_padding_length() {
        let src = r#"
- type: container
  at: [0.2, 0.2]
  size: [1.0, 1.0]
  padding: [1, 2, 3]
  items: []
"#;

        let err = parse_nodes(src).expect_err("expected error");
        match err {
            TemplateError::Yaml { path, .. } => {
                assert!(
                    path.contains("layout[0]") || path.contains("padding"),
                    "path was {path}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn parse_nodes_image_accepts_src() {
        let src = r#"
- type: image
  src: logo.png
  at: [0.0, 0.0]
  size: [10.0, 10.0]
"#;
        let items = parse_nodes(src).expect("parse nodes");
        assert!(matches!(items[0], LayoutItem::Image { .. }));
    }

    #[test]
    fn parse_nodes_line_uses_at_and_to() {
        let src = r#"
- type: line
  at: [0.0, 0.0]
  to: [10.0, 0.0]
  stroke:
    thickness: 0.2
"#;
        let items = parse_nodes(src).expect("parse nodes");
        assert!(matches!(items[0], LayoutItem::Line { .. }));
    }

    #[test]
    fn parse_nodes_line_rejects_legacy_size() {
        let src = r#"
- type: line
  at: [0.0, 0.0]
  size: [10.0, 0.0]
  stroke:
    thickness: 0.2
"#;
        // `size` is no longer a line field (deny_unknown_fields); `to` is required.
        assert!(parse_nodes(src).is_err());
    }

    #[test]
    fn parse_nodes_text_rejects_name() {
        let src = r#"
- type: text
  name: message
  at: [0.0, 0.0]
  size: [10.0, 5.0]
  font_size: 6
"#;
        assert!(parse_nodes(src).is_err());
    }

    #[test]
    fn parse_nodes_qr_rejects_name() {
        let src = r#"
- type: qr
  name: code
  at: [0.0, 0.0]
  size: [10.0, 10.0]
"#;
        assert!(parse_nodes(src).is_err());
    }
}
