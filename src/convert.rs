use crate::errors::TemplateError;
use crate::models::{
    AutoSize, DynamicDimension, Extent, Layout, LayoutItem, Padding, ParamSpec, ParamType,
    ParamValue, Placement, Size, SizeValue, TemplateFormat,
};
use crate::raw::{
    ContainerRaw, LayoutItemRaw, PaddingRaw, PlacementRaw, RawDimension, RawParamSpec,
    RawTemplateFormat, TemplateDefinitionRaw,
};
use crate::templates::TemplateDefinition;

impl PlacementRaw {
    /// `size` xor `to`. `kind` is the item type (`text`, `qr`, `image`, `container`) and becomes the
    /// error path, the way `require_one_of` does it: there is no `placement:` key in the YAML, so
    /// naming one would point the author at something they cannot find. `default_extent` is what
    /// "neither" means for this item kind: `None` makes it an error (text, qr, image), `Some`
    /// supplies the container's fill-the-parent default.
    pub(crate) fn into_placement(
        self,
        kind: &str,
        default_extent: Option<Extent>,
    ) -> Result<Placement, TemplateError> {
        let extent = match (self.size, self.to) {
            (Some(_), Some(_)) => {
                return Err(TemplateError::Validation {
                    path: kind.to_string(),
                    msg: "set exactly one of size or to, not both".to_string(),
                })
            }
            (Some(size), None) => Extent::Size(size),
            (None, Some(to)) => Extent::To(to),
            (None, None) => default_extent.ok_or_else(|| TemplateError::Validation {
                path: kind.to_string(),
                msg: "must set one of size or to".to_string(),
            })?,
        };
        if matches!(extent, Extent::To(_)) && (self.max_w.is_some() || self.max_h.is_some()) {
            return Err(TemplateError::Validation {
                path: kind.to_string(),
                msg: "max_w and max_h resolve `auto` and cannot be combined with to".to_string(),
            });
        }
        Ok(Placement {
            at: self.at.unwrap_or_default(),
            extent,
            max_w: self.max_w,
            max_h: self.max_h,
            rotate: self.rotate,
        })
    }
}

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
        // A container with neither `size` nor `to` keeps today's fill-the-parent default.
        let default_extent = Some(Extent::Size(Size([
            SizeValue::Auto(AutoSize::Auto),
            SizeValue::Auto(AutoSize::Auto),
        ])));
        let placement = raw.placement.into_placement("container", default_extent)?;
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
            when: raw.when.or(raw.option),
            frame: raw.frame,
            padding,
            items,
        })
    }
}

impl TryFrom<LayoutItemRaw> for LayoutItem {
    type Error = TemplateError;

    fn try_from(raw: LayoutItemRaw) -> Result<Self, Self::Error> {
        match raw {
            LayoutItemRaw::Text(raw) => {
                // Also checked in `TemplateDefinition::validate`, which covers items built by any
                // other route; here so an API caller gets the error with its JSON path.
                if let Some(crate::raw::Dynamic::Literal(weight)) = raw.font_weight {
                    if !(100..=900).contains(&weight) || weight % 100 != 0 {
                        return Err(TemplateError::Validation {
                            path: "text.font_weight".to_string(),
                            msg: format!(
                                "font_weight must be a multiple of 100 between 100 and 900, got {weight}"
                            ),
                        });
                    }
                }
                Ok(LayoutItem::Text {
                    value: raw.value,
                    placement: raw.placement.into_placement("text", None)?,
                    font_size: raw.font_size,
                    font_weight: raw.font_weight,
                    multiline: raw.multiline,
                    alignment: raw.alignment,
                    when: raw.when,
                })
            }
            LayoutItemRaw::Qr(raw) => Ok(LayoutItem::Qr {
                value: raw.value,
                placement: raw.placement.into_placement("qr", None)?,
                params: raw.params,
                when: raw.when,
            }),
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
                    placement: raw.placement.into_placement("image", None)?,
                    fit: raw.fit,
                    when: raw.when,
                }),
            },
            LayoutItemRaw::Line(raw) => Ok(LayoutItem::Line {
                at: raw.at,
                to: raw.to,
                thickness: raw.thickness,
                when: raw.when,
            }),
            LayoutItemRaw::Container(raw) => LayoutItem::try_from(raw),
        }
    }
}

impl TryFrom<RawDimension> for DynamicDimension {
    type Error = TemplateError;

    fn try_from(raw: RawDimension) -> Result<Self, Self::Error> {
        Ok(match raw {
            RawDimension::Fixed(d) => DynamicDimension::Fixed(d),
            RawDimension::Dynamic { min, max } => DynamicDimension::Dynamic { min, max },
        })
    }
}

impl TryFrom<RawTemplateFormat> for TemplateFormat {
    type Error = TemplateError;

    fn try_from(raw: RawTemplateFormat) -> Result<Self, Self::Error> {
        match raw {
            RawTemplateFormat::Sheet {
                paper_width,
                paper_height,
                label_width,
                label_height,
                positions,
            } => Ok(TemplateFormat::Sheet {
                paper_width,
                paper_height,
                label_width,
                label_height,
                positions,
            }),
            RawTemplateFormat::Single {
                width,
                height,
                media_width,
            } => Ok(TemplateFormat::Single {
                width: DynamicDimension::try_from(width)?,
                height: DynamicDimension::try_from(height)?,
                media_width,
            }),
        }
    }
}

impl TryFrom<RawParamSpec> for ParamSpec {
    type Error = TemplateError;

    fn try_from(raw: RawParamSpec) -> Result<Self, Self::Error> {
        let param_type = match raw.param_type {
            crate::raw::RawParamType::String => ParamType::String {
                multiline: raw.multiline.unwrap_or(false),
            },
            crate::raw::RawParamType::Length => ParamType::Length,
            crate::raw::RawParamType::Integer => ParamType::Integer,
            crate::raw::RawParamType::Number => ParamType::Number,
            crate::raw::RawParamType::Boolean => ParamType::Boolean,
            crate::raw::RawParamType::Enum => ParamType::Enum {
                values: raw.values.unwrap_or_default(),
            },
        };

        let default = raw.default.map(|v| match v {
            serde_yaml_ng::Value::Bool(b) => ParamValue::Boolean(b),
            serde_yaml_ng::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    if matches!(param_type, ParamType::Integer) {
                        ParamValue::Integer(i)
                    } else {
                        ParamValue::Float(i as f32)
                    }
                } else if let Some(f) = n.as_f64() {
                    ParamValue::Float(f as f32)
                } else {
                    ParamValue::String(n.to_string())
                }
            }
            serde_yaml_ng::Value::String(s) => ParamValue::String(s),
            other => ParamValue::String(format!("{other:?}")),
        });

        Ok(ParamSpec {
            param_type,
            default,
            min: raw.min,
            max: raw.max,
            description: raw.description,
        })
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

        let mut params = std::collections::BTreeMap::new();
        if let Some(raw_params) = raw.params {
            for (key, spec_raw) in raw_params {
                let spec = ParamSpec::try_from(spec_raw)
                    .map_err(|err| err.with_prefix(&format!("params.{key}")))?;
                params.insert(key, spec);
            }
        }
        if let Some(raw_options) = raw.options {
            for (key, values) in raw_options.0 {
                params.entry(key).or_insert(ParamSpec {
                    param_type: ParamType::Enum { values },
                    default: None,
                    min: None,
                    max: None,
                    description: None,
                });
            }
        }

        let format = TemplateFormat::try_from(raw.format)?;

        Ok(TemplateDefinition {
            id: raw.id,
            name: raw.name,
            description: raw.description.unwrap_or_default(),
            group: raw.group.map(|g| g.trim().to_string()),
            unit: raw.unit,
            dpi: raw.dpi,
            format,
            params,
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
    fn text_with_name_fails_deserialization() {
        assert!(try_build(
            "  - type: text\n    name: id\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n"
        )
        .is_err());
    }

    #[test]
    fn text_with_both_fails_deserialization() {
        assert!(try_build("  - type: text\n    name: id\n    value: \"{id}\"\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_err());
    }

    #[test]
    fn text_with_neither_fails_deserialization() {
        assert!(
            try_build("  - type: text\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n")
                .is_err()
        );
    }

    #[test]
    fn qr_with_value_ok() {
        assert!(
            try_build("  - type: qr\n    value: \"{id}\"\n    at: [0,0]\n    size: [10,10]\n")
                .is_ok()
        );
    }

    #[test]
    fn qr_with_name_fails_deserialization() {
        assert!(
            try_build("  - type: qr\n    name: id\n    at: [0,0]\n    size: [10,10]\n").is_err()
        );
    }

    #[test]
    fn text_with_to_instead_of_size_ok() {
        assert!(try_build(
            "  - type: text\n    value: \"x\"\n    at: [0,0]\n    to: [10,5]\n    font_size: 8\n"
        )
        .is_ok());
    }

    #[test]
    fn text_with_both_size_and_to_errors() {
        assert!(try_build("  - type: text\n    value: \"x\"\n    at: [0,0]\n    size: [10,5]\n    to: [10,5]\n    font_size: 8\n").is_err());
    }

    #[test]
    fn text_with_neither_size_nor_to_errors() {
        assert!(
            try_build("  - type: text\n    value: \"x\"\n    at: [0,0]\n    font_size: 8\n")
                .is_err()
        );
    }

    /// max_w/max_h exist only to resolve `auto`, and a `to` box has no auto axis. Accepting them
    /// would imply a clamp that never happens.
    #[test]
    fn to_with_max_w_errors() {
        assert!(try_build("  - type: text\n    value: \"x\"\n    at: [0,0]\n    to: [10,5]\n    max_w: 8\n    font_size: 8\n").is_err());
    }

    /// A container with neither keeps today's fill-the-parent default.
    #[test]
    fn container_with_neither_defaults_to_auto() {
        assert!(try_build("  - type: container\n    at: [0,0]\n    items: []\n").is_ok());
    }
}
