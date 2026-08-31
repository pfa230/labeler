use crate::errors::TemplateError;
use crate::models::{
    DynamicDimension, Extent, Flow, FlowDirection, FlowOverflow, Layout, LayoutItem, Padding,
    ParamSpec, ParamType, ParamValue, Placement, Size, SizeValue, TemplateFormat,
};
use crate::raw::{
    ContainerRaw, LayoutItemRaw, PaddingRaw, PlacementRaw, RawDimension, RawParamSpec,
    RawSizeValue, RawTemplateFormat, TemplateDefinitionRaw,
};
use crate::templates::TemplateContent;

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

        let is_flow = flow.is_some();
        let mut items = Vec::with_capacity(self.items.len());
        for (idx, item) in self.items.into_iter().enumerate() {
            let node = LayoutItem::try_from_raw(item, is_flow)
                .map_err(|err| err.with_prefix(&format!("items[{idx}]")))?;
            items.push(node);
        }

        Ok(LayoutItem::Container {
            placement,
            when: self.when.or(self.option),
            frame: self.frame,
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
                Ok(LayoutItem::Text {
                    value: raw.value,
                    placement: raw.placement.into_placement("text", None, is_packed)?,
                    font_size: raw.font_size,
                    font_weight: raw.font_weight,
                    ink: raw.ink,
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
                Ok(LayoutItem::Line {
                    at: raw.at,
                    to: raw.to,
                    thickness: raw.thickness,
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
            if raw.choices.is_some() {
                return Err(TemplateError::Validation {
                    path: "enum".to_string(),
                    msg: "enum is not supported on datetime parameters".to_string(),
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
        // behavior for every other type is what it was before `datetime` existed. In particular
        // `enum:` (`choices`) is still parsed and still unused: only `values:` builds an enum.
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
        assert!(try_build_param("type: datetime\nenum: [a, b]\n").is_err());
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

    /// `enum:` is parsed so `deny_unknown_fields` accepts it and so a `datetime` parameter can
    /// refuse it, but only `values:` builds an enum's allowed values. That was true before the
    /// `datetime` type existed and this pins it, since widening it would change which templates
    /// validate.
    #[test]
    fn enum_values_come_from_values_only() {
        let from_values = try_build_param("type: enum\nvalues: [a, b]\n").unwrap();
        assert_eq!(
            from_values.param_type,
            crate::models::ParamType::Enum {
                values: vec!["a".to_string(), "b".to_string()]
            }
        );

        let from_choices = try_build_param("type: enum\nenum: [a, b]\n").unwrap();
        assert_eq!(
            from_choices.param_type,
            crate::models::ParamType::Enum { values: Vec::new() }
        );

        // The documented `integer` + `enum:` pairing still parses and still leaves the type alone.
        let integer_choices =
            try_build_param("type: integer\ndefault: 400\nenum: [100, 400, 700]\n").unwrap();
        assert_eq!(
            integer_choices.param_type,
            crate::models::ParamType::Integer
        );
    }
}
