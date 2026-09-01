use crate::errors::TemplateError;
use crate::models::{
    Color, DynamicDimension, DynamicValue, Extent, Flow, FlowDirection, FlowOverflow, Layout,
    LayoutItem, Padding, ParamSpec, ParamType, ParamValue, Placement, Size, SizeValue, Stroke,
    TemplateFormat,
};
use crate::raw::{
    ContainerRaw, LayoutItemRaw, PaddingRaw, PlacementRaw, RawDimension, RawParamSpec,
    RawSizeValue, RawTemplateFormat, StrokeRaw, TemplateDefinitionRaw,
};
use crate::templates::TemplateContent;

impl TryFrom<StrokeRaw> for Stroke {
    type Error = TemplateError;

    fn try_from(raw: StrokeRaw) -> Result<Self, Self::Error> {
        let thickness = match raw.thickness {
            None => {
                return Err(TemplateError::Validation {
                    path: "thickness".to_string(),
                    msg: "stroke thickness is required".to_string(),
                });
            }
            Some(None) => {
                return Err(TemplateError::Validation {
                    path: "thickness".to_string(),
                    msg: "stroke thickness cannot be null".to_string(),
                });
            }
            Some(Some(t)) => t,
        };

        let color =
            match raw.color {
                None => DynamicValue::Literal(Color::black()),
                Some(None) => {
                    return Err(TemplateError::Validation {
                        path: "color".to_string(),
                        msg: "stroke color cannot be null".to_string(),
                    });
                }
                Some(Some(raw_dyn)) => match raw_dyn {
                    DynamicValue::Ref(r) => DynamicValue::Ref(r),
                    DynamicValue::Literal(raw_color) => {
                        let c = raw_color.0.parse::<Color>().map_err(|e| {
                            TemplateError::Validation {
                                path: "color".to_string(),
                                msg: e,
                            }
                        })?;
                        DynamicValue::Literal(c)
                    }
                },
            };

        Ok(Stroke { thickness, color })
    }
}

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
        is_packed: bool,
    ) -> Result<Placement, TemplateError> {
        if is_packed {
            if self.at.is_some() {
                return Err(TemplateError::Validation {
                    path: "at".to_string(),
                    msg: "packed child cannot carry at".to_string(),
                });
            }
            if self.to.is_some() {
                return Err(TemplateError::Validation {
                    path: "to".to_string(),
                    msg: "packed child cannot carry to".to_string(),
                });
            }
        }
        let extent = match (self.size, self.to) {
            (Some(_), Some(_)) => {
                return Err(TemplateError::Validation {
                    path: kind.to_string(),
                    msg: "set exactly one of size or to, not both".to_string(),
                })
            }
            (Some(raw_size), None) => {
                let mut size_vals = Vec::with_capacity(2);
                for (axis, sv) in raw_size.0.into_iter().enumerate() {
                    match sv {
                        RawSizeValue::Auto => {
                            return Err(TemplateError::Validation {
                                path: format!("size[{axis}]"),
                                msg: "`auto` was renamed: use `content` to hug the item's own size, or `fill` to stretch to the frame".to_string(),
                            });
                        }
                        RawSizeValue::Content => size_vals.push(SizeValue::Content),
                        RawSizeValue::Fill => size_vals.push(SizeValue::Fill),
                        RawSizeValue::Dynamic(dv) => size_vals.push(SizeValue::Dynamic(dv)),
                    }
                }
                Extent::Size(Size([size_vals.remove(0), size_vals.remove(0)]))
            }
            (None, Some(to)) => Extent::To(to),
            (None, None) => default_extent.ok_or_else(|| TemplateError::Validation {
                path: kind.to_string(),
                msg: "must set one of size or to".to_string(),
            })?,
        };
        let at = if is_packed {
            None
        } else {
            Some(self.at.unwrap_or_default())
        };
        Ok(Placement {
            at,
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

impl ContainerRaw {
    pub(crate) fn try_into_container(self, is_packed: bool) -> Result<LayoutItem, TemplateError> {
        let flow = match self.flow {
            Some(Some(flow_raw)) => {
                let direction = match flow_raw.direction.as_deref() {
                    Some("row") => FlowDirection::Row,
                    Some("column") => FlowDirection::Column,
                    None => {
                        return Err(TemplateError::Validation {
                            path: "flow.direction".to_string(),
                            msg: "flow direction is required ('row' or 'column')".to_string(),
                        });
                    }
                    Some(other) => {
                        return Err(TemplateError::Validation {
                            path: "flow.direction".to_string(),
                            msg: format!(
                                "unknown flow direction '{other}': must be 'row' or 'column'"
                            ),
                        });
                    }
                };
                let gap = match flow_raw.gap {
                    Some(g) if !g.is_finite() || g < 0.0 => {
                        return Err(TemplateError::Validation {
                            path: "flow.gap".to_string(),
                            msg: "flow gap must be >= 0 and finite".to_string(),
                        });
                    }
                    Some(g) => g,
                    None => 0.0,
                };
                let line_gap = match flow_raw.line_gap {
                    Some(g) if !g.is_finite() || g < 0.0 => {
                        return Err(TemplateError::Validation {
                            path: "flow.line_gap".to_string(),
                            msg: "flow line_gap must be >= 0 and finite".to_string(),
                        });
                    }
                    Some(g) => g,
                    None => 0.0,
                };
                let overflow = match flow_raw.overflow.unwrap_or_default() {
                    FlowOverflow::Invalid => {
                        return Err(TemplateError::Validation {
                            path: "flow.overflow".to_string(),
                            msg: "flow overflow must be 'fail' or 'trim'".to_string(),
                        });
                    }
                    overflow => overflow,
                };
                Some(Flow {
                    direction,
                    gap,
                    wrap: flow_raw.wrap,
                    line_gap,
                    overflow,
                })
            }
            Some(None) => {
                return Err(TemplateError::Validation {
                    path: "flow.direction".to_string(),
                    msg: "flow direction is required ('row' or 'column')".to_string(),
                });
            }
            None => None,
        };

        // A container with neither `size` nor `to` defaults to size: [fill, fill]
        let default_extent = Some(Extent::Size(Size([SizeValue::Fill, SizeValue::Fill])));
        let placement = self
            .placement
            .into_placement("container", default_extent, is_packed)?;
        let padding = match self.padding {
            None => Padding::ZERO,
            Some(padding) => Padding::try_from(padding)?,
        };

        let stroke = match self.stroke {
            Some(None) => {
                return Err(TemplateError::Validation {
                    path: "stroke".to_string(),
                    msg: "stroke cannot be null".to_string(),
                });
            }
            Some(Some(raw_stroke)) => {
                let stroke =
                    Stroke::try_from(raw_stroke).map_err(|err| err.with_prefix("stroke"))?;
                Some(stroke)
            }
            None => None,
        };

        let background = match self.background {
            Some(None) => {
                return Err(TemplateError::Validation {
                    path: "background".to_string(),
                    msg: "background cannot be null".to_string(),
                });
            }
            Some(Some(raw_dyn)) => {
                let dyn_color = match raw_dyn {
                    DynamicValue::Ref(r) => DynamicValue::Ref(r),
                    DynamicValue::Literal(raw_color) => {
                        let c = raw_color.0.parse::<Color>().map_err(|e| {
                            TemplateError::Validation {
                                path: "background".to_string(),
                                msg: e,
                            }
                        })?;
                        DynamicValue::Literal(c)
                    }
                };
                Some(dyn_color)
            }
            None => None,
        };

        let rounded = match self.rounded {
            Some(None) => {
                return Err(TemplateError::Validation {
                    path: "rounded".to_string(),
                    msg: "rounded cannot be null".to_string(),
                });
            }
            Some(Some(r)) => Some(r),
            None => None,
        };

        let is_flow = flow.is_some();
        let mut items = Vec::with_capacity(self.items.len());
        for (idx, item) in self.items.into_iter().enumerate() {
            let node = LayoutItem::try_from_raw(item, is_flow)
                .map_err(|err| err.with_prefix(&format!("items[{idx}]")))?;
            items.push(node);
        }

        Ok(LayoutItem::Container {
            placement,
            when: self.when,
            stroke,
            background,
            rounded,
            padding,
            flow,
            items,
        })
    }
}

impl TryFrom<ContainerRaw> for LayoutItem {
    type Error = TemplateError;

    fn try_from(raw: ContainerRaw) -> Result<Self, Self::Error> {
        raw.try_into_container(false)
    }
}

impl LayoutItem {
    pub(crate) fn try_from_raw(raw: LayoutItemRaw, is_packed: bool) -> Result<Self, TemplateError> {
        match raw {
            LayoutItemRaw::Text(raw) => {
                if raw.multiline.is_some() {
                    return Err(TemplateError::Validation {
                        path: "multiline".to_string(),
                        msg: "`multiline` on a text item was renamed: use `wrap: true` to enable soft wrapping, or `wrap: false` (the default) to keep hard breaks without soft wrapping".to_string(),
                    });
                }
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
                let color = match raw.color {
                    None | Some(None) => None,
                    Some(Some(raw_dyn)) => {
                        let dyn_color = match raw_dyn {
                            DynamicValue::Ref(r) => DynamicValue::Ref(r),
                            DynamicValue::Literal(raw_color) => {
                                let c = raw_color.0.parse::<Color>().map_err(|e| {
                                    TemplateError::Validation {
                                        path: "color".to_string(),
                                        msg: e,
                                    }
                                })?;
                                DynamicValue::Literal(c)
                            }
                        };
                        Some(dyn_color)
                    }
                };
                Ok(LayoutItem::Text {
                    value: raw.value,
                    placement: raw.placement.into_placement("text", None, is_packed)?,
                    font_size: raw.font_size,
                    font_weight: raw.font_weight,
                    color,
                    wrap: raw.wrap,
                    alignment: raw.alignment,
                    overflow: raw.overflow,
                    when: raw.when,
                })
            }
            LayoutItemRaw::Qr(raw) => Ok(LayoutItem::Qr {
                value: raw.value,
                placement: raw.placement.into_placement("qr", None, is_packed)?,
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
                    placement: raw.placement.into_placement("image", None, is_packed)?,
                    fit: raw.fit,
                    when: raw.when,
                }),
            },
            LayoutItemRaw::Line(raw) => {
                if is_packed {
                    return Err(TemplateError::Validation {
                        path: "".to_string(),
                        msg: "line cannot be a packed child".to_string(),
                    });
                }
                let stroke = match raw.stroke {
                    None => None,
                    Some(None) => {
                        return Err(TemplateError::Validation {
                            path: "stroke".to_string(),
                            msg: "stroke cannot be null".to_string(),
                        });
                    }
                    Some(Some(raw_stroke)) => Some(
                        Stroke::try_from(raw_stroke).map_err(|err| err.with_prefix("stroke"))?,
                    ),
                };
                Ok(LayoutItem::Line {
                    at: raw.at,
                    to: raw.to,
                    stroke,
                    when: raw.when,
                })
            }
            LayoutItemRaw::Container(raw) => raw.try_into_container(is_packed),
        }
    }
}

impl TryFrom<LayoutItemRaw> for LayoutItem {
    type Error = TemplateError;

    fn try_from(raw: LayoutItemRaw) -> Result<Self, Self::Error> {
        LayoutItem::try_from_raw(raw, false)
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

fn convert_raw_default(
    default_raw: Option<serde_yaml_ng::Value>,
    is_integer: bool,
) -> Option<ParamValue> {
    match default_raw {
        None => None,
        Some(serde_yaml_ng::Value::Bool(b)) => Some(ParamValue::Boolean(b)),
        Some(serde_yaml_ng::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                if is_integer {
                    Some(ParamValue::Integer(i))
                } else {
                    Some(ParamValue::Float(i as f32))
                }
            } else if let Some(f) = n.as_f64() {
                Some(ParamValue::Float(f as f32))
            } else {
                Some(ParamValue::String(n.to_string()))
            }
        }
        Some(serde_yaml_ng::Value::String(s)) => Some(ParamValue::String(s)),
        Some(other) => Some(ParamValue::String(format!("{other:?}"))),
    }
}

impl TryFrom<RawParamSpec> for ParamSpec {
    type Error = TemplateError;

    fn try_from(raw: RawParamSpec) -> Result<Self, Self::Error> {
        if raw.format.is_some() {
            return Err(TemplateError::Validation {
                path: "format".to_string(),
                msg: "format is not supported on parameters; choose format in the interpolation token (e.g. '{param.format_name}')".to_string(),
            });
        }

        // Collapse an explicit YAML null default: to None for every type before any type-specific check runs.
        let default_raw = match raw.default {
            Some(serde_yaml_ng::Value::Null) | None => None,
            Some(val) => Some(val),
        };

        if raw.param_type == crate::raw::RawParamType::Datetime {
            if raw.min.is_some() {
                return Err(TemplateError::Validation {
                    path: "min".to_string(),
                    msg: "min is not supported on datetime parameters".to_string(),
                });
            }
            if raw.max.is_some() {
                return Err(TemplateError::Validation {
                    path: "max".to_string(),
                    msg: "max is not supported on datetime parameters".to_string(),
                });
            }
            if raw.multiline.is_some() {
                return Err(TemplateError::Validation {
                    path: "multiline".to_string(),
                    msg: "multiline is not supported on datetime parameters".to_string(),
                });
            }
            if raw.values.is_some() {
                return Err(TemplateError::Validation {
                    path: "values".to_string(),
                    msg: "values is not supported on datetime parameters".to_string(),
                });
            }

            let time = match raw.time {
                None => false,
                Some(Some(b)) => b,
                Some(None) => {
                    return Err(TemplateError::Validation {
                        path: "time".to_string(),
                        msg: "time must be a boolean (true or false)".to_string(),
                    });
                }
            };

            let default = convert_raw_default(default_raw, false);

            return Ok(ParamSpec {
                param_type: ParamType::Datetime { time },
                default,
                min: None,
                max: None,
                description: raw.description,
            });
        }

        if raw.time.is_some() {
            return Err(TemplateError::Validation {
                path: "time".to_string(),
                msg: "time is only supported on datetime parameters".to_string(),
            });
        }

        // `.flatten()` collapses "absent" and "written empty" back into the one `None` the domain
        // model has always had. Presence mattered only to the datetime rules above; from here the
        // behavior for every other type is what it was before `datetime` existed.
        let multiline = raw.multiline.flatten().unwrap_or(false);
        let values = raw.values.flatten().unwrap_or_default();
        let min = raw.min.flatten();
        let max = raw.max.flatten();

        let param_type = match raw.param_type {
            crate::raw::RawParamType::String => ParamType::String { multiline },
            crate::raw::RawParamType::Length => ParamType::Length,
            crate::raw::RawParamType::Integer => ParamType::Integer,
            crate::raw::RawParamType::Number => ParamType::Number,
            crate::raw::RawParamType::Boolean => ParamType::Boolean,
            crate::raw::RawParamType::Enum => ParamType::Enum { values },
            crate::raw::RawParamType::Datetime => unreachable!(),
        };

        let default = convert_raw_default(default_raw, matches!(param_type, ParamType::Integer));

        Ok(ParamSpec {
            param_type,
            default,
            min,
            max,
            description: raw.description,
        })
    }
}

impl TryFrom<TemplateDefinitionRaw> for TemplateContent {
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

        let format = TemplateFormat::try_from(raw.format)?;

        Ok(TemplateContent {
            name: raw.name,
            description: raw.description.unwrap_or_default(),
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
    use crate::templates::TemplateContent;
    use std::str::FromStr;

    fn try_build(layout_yaml: &str) -> Result<TemplateContent, String> {
        let yaml = format!(
            "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 10\n  height: 10\nlayout:\n{layout_yaml}"
        );
        let raw: TemplateDefinitionRaw =
            serde_yaml_ng::from_str(&yaml).map_err(|e| e.to_string())?;
        TemplateContent::try_from(raw).map_err(|e| e.to_string())
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

    /// max_w/max_h beside `to` is accepted in conversion; resolver binds or ignores caps by source.
    #[test]
    fn to_with_max_w_is_accepted() {
        assert!(try_build("  - type: text\n    value: \"x\"\n    at: [0,0]\n    to: [10,5]\n    max_w: 8\n    font_size: 8\n").is_ok());
    }

    /// A container with neither keeps today's fill-the-parent default.
    #[test]
    fn container_with_neither_defaults_to_auto() {
        assert!(try_build("  - type: container\n    at: [0,0]\n    items: []\n").is_ok());
    }

    fn try_build_param(param_yaml: &str) -> Result<crate::models::ParamSpec, String> {
        let raw: crate::raw::RawParamSpec =
            serde_yaml_ng::from_str(param_yaml).map_err(|e| e.to_string())?;
        crate::models::ParamSpec::try_from(raw).map_err(|e| e.to_string())
    }

    #[test]
    fn datetime_param_valid_declarations() {
        let bare = try_build_param("type: datetime\n").unwrap();
        assert_eq!(
            bare.param_type,
            crate::models::ParamType::Datetime { time: false }
        );
        let serialized = serde_json::to_string(&bare.param_type).unwrap();
        assert!(
            serialized.contains("\"time\":false"),
            "time: false must be explicitly serialized: {serialized}"
        );

        let with_time_true = try_build_param("type: datetime\ntime: true\n").unwrap();
        assert_eq!(
            with_time_true.param_type,
            crate::models::ParamType::Datetime { time: true }
        );

        let with_time_false = try_build_param("type: datetime\ntime: false\n").unwrap();
        assert_eq!(
            with_time_false.param_type,
            crate::models::ParamType::Datetime { time: false }
        );
    }

    #[test]
    fn datetime_param_rejects_forbidden_attributes() {
        assert!(try_build_param("type: datetime\ndefault: \"2026-08-19\"\n").is_ok());
        assert!(try_build_param("type: datetime\ndefault:\n").is_ok());
        assert!(try_build_param("type: datetime\nformat: short_date\n").is_err());
        assert!(try_build_param("type: datetime\nmin: 0\n").is_err());
        assert!(try_build_param("type: datetime\nmax: 100\n").is_err());
        assert!(try_build_param("type: datetime\nmultiline: true\n").is_err());
        assert!(try_build_param("type: datetime\nvalues: [a, b]\n").is_err());
        assert!(try_build_param("type: datetime\ntime:\n").is_err());
        assert!(try_build_param("type: datetime\ntime: \"invalid\"\n").is_err());
    }

    #[test]
    fn non_datetime_param_rejects_time_and_format() {
        assert!(try_build_param("type: string\ntime: true\n").is_err());
        assert!(try_build_param("type: string\ntime:\n").is_err());
        assert!(try_build_param("type: integer\ntime: true\n").is_err());
        assert!(try_build_param("type: string\nformat: short_date\n").is_err());
        assert!(try_build_param("type: integer\nformat: standard\n").is_err());
    }

    /// Presence detection for the `datetime` rules must not cost the other types their typing:
    /// a malformed attribute stays a load-time error instead of being silently dropped, which
    /// would turn a slider into a plain number input with no range and no complaint.
    #[test]
    fn non_datetime_param_attributes_keep_their_types() {
        assert!(
            try_build_param("type: length\nmin: \"twenty\"\n").is_err(),
            "a non-numeric min must fail to load, not resolve to no min"
        );
        assert!(try_build_param("type: length\nmax: [1, 2]\n").is_err());
        assert!(
            try_build_param("type: string\nmultiline: \"yes\"\n").is_err(),
            "a non-boolean multiline must fail to load, not resolve to false"
        );
        assert!(try_build_param("type: enum\nvalues: 3\n").is_err());

        let ok = try_build_param("type: length\nmin: 25\nmax: 300\n").unwrap();
        assert_eq!(ok.min, Some(25.0));
        assert_eq!(ok.max, Some(300.0));

        // Written and left empty is the same as absent for every type but `datetime`.
        let empty = try_build_param("type: length\nmin:\n").unwrap();
        assert_eq!(empty.min, None);
    }

    #[test]
    fn enum_key_is_refused_as_unknown_field() {
        // `type: enum` with `values:` still builds an enum.
        let from_values = try_build_param("type: enum\nvalues: [a, b]\n").unwrap();
        assert_eq!(
            from_values.param_type,
            crate::models::ParamType::Enum {
                values: vec!["a".to_string(), "b".to_string()]
            }
        );

        for yaml in [
            "type: enum\nenum: [a, b]\n",
            "type: integer\ndefault: 400\nenum: [100, 400, 700]\n",
            "type: datetime\nenum: [\"2026-01-01\"]\n",
            "type: integer\nenum:\n",
        ] {
            let err = serde_yaml_ng::from_str::<crate::raw::RawParamSpec>(yaml)
                .expect_err("enum: must be refused as unknown field");
            let msg = err.to_string();
            assert!(
                msg.contains("enum"),
                "expected error to name `enum` for {yaml:?}, got: {msg}"
            );
            assert!(
                msg.contains("unknown field"),
                "expected unknown-field error for {yaml:?}, got: {msg}"
            );
        }
    }

    #[test]
    fn shape_paint_container_refusals_and_defaults() {
        // stroke: null
        let err = try_build("  - type: container\n    at: [0,0]\n    stroke:\n    items: []\n")
            .unwrap_err();
        assert!(
            err.contains("layout[0].stroke"),
            "expected layout[0].stroke in {err}"
        );
        assert!(
            err.contains("stroke cannot be null"),
            "expected message in {err}"
        );

        // background: null
        let err = try_build("  - type: container\n    at: [0,0]\n    background:\n    items: []\n")
            .unwrap_err();
        assert!(
            err.contains("layout[0].background"),
            "expected layout[0].background in {err}"
        );
        assert!(
            err.contains("background cannot be null"),
            "expected message in {err}"
        );

        // rounded: null
        let err = try_build("  - type: container\n    at: [0,0]\n    rounded:\n    items: []\n")
            .unwrap_err();
        assert!(
            err.contains("layout[0].rounded"),
            "expected layout[0].rounded in {err}"
        );
        assert!(
            err.contains("rounded cannot be null"),
            "expected message in {err}"
        );

        // stroke thickness null
        let err = try_build(
            "  - type: container\n    at: [0,0]\n    stroke:\n      thickness:\n    items: []\n",
        )
        .unwrap_err();
        assert!(
            err.contains("layout[0].stroke.thickness"),
            "expected path in {err}"
        );
        assert!(
            err.contains("stroke thickness cannot be null"),
            "expected message in {err}"
        );

        // stroke color null
        let err = try_build("  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 1.0\n      color:\n    items: []\n").unwrap_err();
        assert!(
            err.contains("layout[0].stroke.color"),
            "expected path in {err}"
        );
        assert!(
            err.contains("stroke color cannot be null"),
            "expected message in {err}"
        );

        // stroke missing thickness
        let err = try_build(
            "  - type: container\n    at: [0,0]\n    stroke:\n      color: red\n    items: []\n",
        )
        .unwrap_err();
        assert!(
            err.contains("layout[0].stroke.thickness"),
            "expected path in {err}"
        );
        assert!(
            err.contains("stroke thickness is required"),
            "expected message in {err}"
        );

        // valid stroke defaults color to black
        let template = try_build(
            "  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 0.5\n    items: []\n",
        )
        .unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        if let crate::models::LayoutItem::Container {
            stroke,
            background,
            rounded,
            ..
        } = &items[0]
        {
            let stroke = stroke.as_ref().expect("stroke should be present");
            assert_eq!(stroke.thickness, 0.5);
            assert_eq!(
                stroke.color,
                crate::models::DynamicValue::Literal(crate::models::Color::black())
            );
            assert_eq!(stroke.color.as_literal().unwrap().hex(), "#000000ff");
            assert!(background.is_none());
            assert!(rounded.is_none());
        } else {
            panic!("expected container");
        }

        // valid stroke with custom color, background, and rounded
        let template = try_build("  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 0.0001\n      color: '#f0c'\n    background: navy\n    rounded: 0.0001\n    items: []\n").unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        if let crate::models::LayoutItem::Container {
            stroke,
            background,
            rounded,
            ..
        } = &items[0]
        {
            let stroke = stroke.as_ref().unwrap();
            assert_eq!(stroke.thickness, 0.0001);
            assert_eq!(
                stroke.color,
                crate::models::DynamicValue::Literal(
                    crate::models::Color::from_str("#f0c").unwrap()
                )
            );
            assert_eq!(
                background.as_ref().unwrap(),
                &crate::models::DynamicValue::Literal(
                    crate::models::Color::from_str("navy").unwrap()
                )
            );
            assert_eq!(rounded.unwrap(), 0.0001);
        } else {
            panic!("expected container");
        }
    }

    #[test]
    fn shape_paint_line_refusals() {
        // line stroke required
        // line without stroke is accepted with stroke: None (omitted = no outline)
        let template = try_build("  - type: line\n    at: [0,0]\n    to: [5,5]\n").unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        if let crate::models::LayoutItem::Line { stroke, .. } = &items[0] {
            assert!(stroke.is_none());
        } else {
            panic!("expected line");
        }

        // line stroke null
        let err =
            try_build("  - type: line\n    at: [0,0]\n    to: [5,5]\n    stroke:\n").unwrap_err();
        assert!(err.contains("layout[0].stroke"), "expected path in {err}");
        assert!(
            err.contains("stroke cannot be null"),
            "expected message in {err}"
        );

        // line stroke thickness null
        let err = try_build(
            "  - type: line\n    at: [0,0]\n    to: [5,5]\n    stroke:\n      thickness:\n",
        )
        .unwrap_err();
        assert!(
            err.contains("layout[0].stroke.thickness"),
            "expected path in {err}"
        );
        assert!(
            err.contains("stroke thickness cannot be null"),
            "expected message in {err}"
        );

        // line stroke color null
        let err = try_build("  - type: line\n    at: [0,0]\n    to: [5,5]\n    stroke:\n      thickness: 1.0\n      color:\n").unwrap_err();
        assert!(
            err.contains("layout[0].stroke.color"),
            "expected path in {err}"
        );
        assert!(
            err.contains("stroke color cannot be null"),
            "expected message in {err}"
        );

        // line background rejected
        let err = try_build(
            "  - type: line\n    at: [0,0]\n    to: [5,5]\n    stroke:\n      thickness: 0.5\n    background: red\n",
        )
        .unwrap_err();
        assert!(
            err.contains("unknown field `background`"),
            "expected unknown field background in {err}"
        );

        // line rounded rejected
        let err = try_build(
            "  - type: line\n    at: [0,0]\n    to: [5,5]\n    stroke:\n      thickness: 0.5\n    rounded: 1.0\n",
        )
        .unwrap_err();
        assert!(
            err.contains("unknown field `rounded`"),
            "expected unknown field rounded in {err}"
        );

        // valid line with stroke defaults color to black
        let template = try_build(
            "  - type: line\n    at: [0,0]\n    to: [5,5]\n    stroke:\n      thickness: 0.5\n",
        )
        .unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        if let crate::models::LayoutItem::Line { stroke, .. } = &items[0] {
            let stroke = stroke.as_ref().unwrap();
            assert_eq!(stroke.thickness, 0.5);
            assert_eq!(
                stroke.color,
                crate::models::DynamicValue::Literal(crate::models::Color::black())
            );
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn padded_color_literals_convert_to_expected_colors() {
        let layout_yaml = r#"  - type: container
    at: [0, 0]
    size: [10, 10]
    background: " #F0F "
    stroke:
      thickness: 0.2
      color: " navy "
    items:
      - type: text
        value: "Hello"
        at: [0, 0]
        size: [10, 5]
        font_size: 8
        color: " red "
"#;
        let template = try_build(layout_yaml).expect("template should load");
        let crate::models::Layout::Items(items) = &template.layout;
        match &items[0] {
            crate::models::LayoutItem::Container {
                background,
                stroke,
                items: child_items,
                ..
            } => {
                let bg = background.as_ref().expect("background present");
                assert_eq!(bg.as_literal().unwrap().rgba(), [0xff, 0x00, 0xff, 0xff]);

                let stroke_val = stroke.as_ref().expect("stroke present");
                assert_eq!(
                    stroke_val.color.as_literal().unwrap().rgba(),
                    [0x00, 0x00, 0x80, 0xff]
                );

                match &child_items[0] {
                    crate::models::LayoutItem::Text { color, .. } => {
                        let text_col = color.as_ref().expect("text color present");
                        assert_eq!(
                            text_col.as_literal().unwrap().rgba(),
                            [0xff, 0x00, 0x00, 0xff]
                        );
                    }
                    _ => panic!("expected text child"),
                }
            }
            _ => panic!("expected container"),
        }
    }
}
