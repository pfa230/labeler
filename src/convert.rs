use crate::errors::TemplateError;
use crate::models::{AutoSize, Layout, LayoutItem, Padding, Placement, Size, SizeValue};
use crate::raw::{ContainerRaw, LayoutItemRaw, PaddingRaw, TemplateDefinitionRaw, TextRaw};
use crate::templates::TemplateDefinition;

impl TryFrom<PaddingRaw> for Padding {
    type Error = TemplateError;

    fn try_from(raw: PaddingRaw) -> Result<Self, Self::Error> {
        let padding = match raw {
            PaddingRaw::Uniform(value) => Padding {
                top: value,
                right: value,
                bottom: value,
                left: value,
            },
            PaddingRaw::Trbl([top, right, bottom, left]) => Padding {
                top,
                right,
                bottom,
                left,
            },
        };

        if padding.top < 0.0 || padding.right < 0.0 || padding.bottom < 0.0 || padding.left < 0.0 {
            return Err(TemplateError::Validation {
                path: "padding".to_string(),
                msg: "padding values must be >= 0".to_string(),
            });
        }

        Ok(padding)
    }
}

impl TryFrom<ContainerRaw> for LayoutItem {
    type Error = TemplateError;

    fn try_from(raw: ContainerRaw) -> Result<Self, Self::Error> {
        let at = raw.at.unwrap_or_default();
        let size = raw.size.unwrap_or(Size([
            SizeValue::Auto(AutoSize::Auto),
            SizeValue::Auto(AutoSize::Auto),
        ]));
        let placement = Placement {
            at,
            size,
            max_w: raw.max_w,
            max_h: raw.max_h,
            rotate: raw.rotate,
        };
        let padding = match raw.padding {
            None => Padding::ZERO,
            Some(padding) => Padding::try_from(padding)?,
        };

        let mut items = Vec::with_capacity(raw.items.len());
        for (idx, item) in raw.items.into_iter().enumerate() {
            let node = LayoutItem::try_from(item)
                .map_err(|err| err.with_prefix(&format!("items[{idx}]")))?;
            items.push(node);
        }

        Ok(LayoutItem::Container {
            placement,
            option: raw.option,
            frame: raw.frame,
            padding,
            items,
        })
    }
}

fn require_one_of(
    kind: &str,
    name: Option<String>,
    value: Option<String>,
) -> Result<(Option<String>, Option<String>), TemplateError> {
    match (&name, &value) {
        (Some(_), Some(_)) => Err(TemplateError::Validation {
            path: kind.to_string(),
            msg: format!("{kind} must set exactly one of name or value, not both"),
        }),
        (None, None) => Err(TemplateError::Validation {
            path: kind.to_string(),
            msg: format!("{kind} must set one of name or value"),
        }),
        _ => Ok((name, value)),
    }
}

impl TryFrom<LayoutItemRaw> for LayoutItem {
    type Error = TemplateError;

    fn try_from(raw: LayoutItemRaw) -> Result<Self, Self::Error> {
        match raw {
            LayoutItemRaw::Text(TextRaw {
                name,
                value,
                placement,
                font_size,
                multiline,
                alignment,
            }) => {
                let (name, value) = require_one_of("text", name, value)?;
                Ok(LayoutItem::Text {
                    name,
                    value,
                    placement,
                    font_size,
                    multiline,
                    alignment,
                })
            }
            LayoutItemRaw::Qr(raw) => {
                let (name, value) = require_one_of("qr", raw.name, raw.value)?;
                Ok(LayoutItem::Qr {
                    name,
                    value,
                    placement: raw.placement,
                    params: raw.params,
                })
            }
            LayoutItemRaw::Image(raw) => match (&raw.src, &raw.name) {
                (Some(_), Some(_)) => Err(TemplateError::Validation {
                    path: "image".to_string(),
                    msg: "image must set exactly one of src or name, not both".to_string(),
                }),
                (None, None) => Err(TemplateError::Validation {
                    path: "image".to_string(),
                    msg: "image must set one of src or name".to_string(),
                }),
                _ => Ok(LayoutItem::Image {
                    name: raw.name,
                    src: raw.src,
                    placement: raw.placement,
                    fit: raw.fit,
                }),
            },
            LayoutItemRaw::Line(raw) => Ok(LayoutItem::Line {
                at: raw.at,
                to: raw.to,
                thickness: raw.thickness,
            }),
            LayoutItemRaw::Container(raw) => LayoutItem::try_from(raw),
        }
    }
}

impl TryFrom<TemplateDefinitionRaw> for TemplateDefinition {
    type Error = TemplateError;

    fn try_from(raw: TemplateDefinitionRaw) -> Result<Self, Self::Error> {
        let mut items = Vec::with_capacity(raw.layout.len());
        for (idx, item) in raw.layout.into_iter().enumerate() {
            let node = LayoutItem::try_from(item)
                .map_err(|err| err.with_prefix(&format!("layout[{idx}]")))?;
            items.push(node);
        }

        Ok(TemplateDefinition {
            id: raw.id,
            name: raw.name,
            description: raw.description.unwrap_or_default(),
            unit: raw.unit,
            dpi: raw.dpi,
            format: raw.format,
            options: raw.options,
            layout: Layout::Items(items),
            version: raw.version,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::raw::TemplateDefinitionRaw;
    use crate::templates::TemplateDefinition;

    fn try_build(layout_yaml: &str) -> Result<TemplateDefinition, String> {
        let yaml = format!(
            "id: t\nname: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 10\n  height: 10\nlayout:\n{layout_yaml}"
        );
        let raw: TemplateDefinitionRaw =
            serde_yaml_ng::from_str(&yaml).map_err(|e| e.to_string())?;
        TemplateDefinition::try_from(raw).map_err(|e| e.to_string())
    }

    #[test]
    fn text_with_value_ok() {
        assert!(try_build("  - type: text\n    value: \"{id}\"\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_ok());
    }

    #[test]
    fn text_with_name_ok() {
        assert!(try_build(
            "  - type: text\n    name: id\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n"
        )
        .is_ok());
    }

    #[test]
    fn text_with_both_errors() {
        assert!(try_build("  - type: text\n    name: id\n    value: \"{id}\"\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_err());
    }

    #[test]
    fn text_with_neither_errors() {
        assert!(
            try_build("  - type: text\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n")
                .is_err()
        );
    }
}
