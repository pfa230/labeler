use crate::errors::TemplateError;
use crate::models::LayoutItem;
use crate::raw::{LayoutItemRaw, TemplateDefinitionRaw};
use crate::templates::TemplateDefinition;

pub fn parse_nodes(src: &str) -> Result<Vec<LayoutItem>, TemplateError> {
    let deserializer = serde_yaml_ng::Deserializer::from_str(src);
    let raw: Vec<LayoutItemRaw> =
        serde_path_to_error::deserialize(deserializer).map_err(|err| {
            TemplateError::Yaml {
                path: err.path().to_string(),
                msg: err.to_string(),
            }
            .with_prefix("items")
        })?;

    raw.into_iter()
        .enumerate()
        .map(|(idx, item)| {
            LayoutItem::try_from(item).map_err(|err| err.with_prefix(&format!("items[{idx}]")))
        })
        .collect()
}

pub fn parse_template(src: &str) -> Result<TemplateDefinition, TemplateError> {
    let deserializer = serde_yaml_ng::Deserializer::from_str(src);
    let raw: TemplateDefinitionRaw =
        serde_path_to_error::deserialize(deserializer).map_err(|err| TemplateError::Yaml {
            path: err.path().to_string(),
            msg: err.to_string(),
        })?;

    TemplateDefinition::try_from(raw)
}

#[cfg(test)]
mod tests {
    use super::parse_nodes;
    use crate::errors::TemplateError;
    use crate::models::{LayoutItem, Padding};

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
                    path.contains("items[0]") || path.contains("padding"),
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
    fn parse_nodes_image_accepts_name() {
        let src = r#"
- type: image
  name: photo
  at: [0.0, 0.0]
  size: [10.0, 10.0]
"#;
        let items = parse_nodes(src).expect("parse nodes");
        assert!(matches!(items[0], LayoutItem::Image { .. }));
    }

    #[test]
    fn parse_nodes_image_rejects_both_src_and_name() {
        let src = r#"
- type: image
  src: logo.png
  name: photo
  at: [0.0, 0.0]
  size: [10.0, 10.0]
"#;
        let err = parse_nodes(src).expect_err("expected error");
        assert!(matches!(err, TemplateError::Validation { .. }));
    }

    #[test]
    fn parse_nodes_image_rejects_neither_src_nor_name() {
        let src = r#"
- type: image
  at: [0.0, 0.0]
  size: [10.0, 10.0]
"#;
        let err = parse_nodes(src).expect_err("expected error");
        assert!(matches!(err, TemplateError::Validation { .. }));
    }
}
