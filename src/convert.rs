use crate::errors::TemplateError;
use crate::models::{
    font_weight_ok, DynamicDimension, Extent, Flow, FlowDirection, FlowOverflow, Layout,
    LayoutItem, Padding, ParamSpec, ParamType, ParamValue, Placement, Shape, Size, SizeValue,
    TemplateFormat,
};
use crate::raw::{
    ContainerRaw, LayoutItemRaw, PaddingRaw, PlacementRaw, RawDimension, RawParamEntry,
    RawParamType, RawSizeValue, RawTemplateFormat, SheetFormatRaw, SingleFormatRaw,
    TemplateDefinitionRaw,
};
use crate::templates::TemplateContent;

impl PlacementRaw {
    /// `size` xor `to`, one of which every boxed item sets. `kind` is the item type (`text`, `qr`,
    /// `image`, `container`) and becomes the error path for a missing or doubled extent, the way
    /// `require_one_of` does it: there is no `placement:` key in the YAML, so naming one would point
    /// the author at something they cannot find.
    pub(crate) fn into_placement(
        self,
        kind: &str,
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
            (Some(raw_size), None) => Extent::Size(Size(raw_size.0.map(|value| match value {
                RawSizeValue::Content => SizeValue::Content,
                RawSizeValue::Fill => SizeValue::Fill,
                RawSizeValue::Dynamic(dv) => SizeValue::Dynamic(dv),
            }))),
            (None, Some(to)) => Extent::To(to),
            (None, None) => {
                return Err(TemplateError::Validation {
                    path: kind.to_string(),
                    msg: "must set one of size or to".to_string(),
                })
            }
        };
        let at = if is_packed {
            None
        } else {
            Some(self.at.unwrap_or_default())
        };
        if let (Some(at), Extent::To(to)) = (&at, &extent) {
            for (axis, coord) in AXIS_COORDS.into_iter().enumerate() {
                if at.0[axis].is_sign_negative() && !to.0[axis].is_sign_negative() {
                    return Err(TemplateError::Validation {
                        path: "to".to_string(),
                        msg: format!(
                            "an edge-relative at cannot pair with a non-negative to on {coord}"
                        ),
                    });
                }
            }
        }
        for (axis, (key, cap)) in [("max_w", self.max_w), ("max_h", self.max_h)]
            .into_iter()
            .enumerate()
        {
            let stretches = matches!(&extent, Extent::Size(size)
                if matches!(size.0[axis], SizeValue::Content | SizeValue::Fill));
            if cap.is_some() && !stretches {
                return Err(TemplateError::Validation {
                    path: key.to_string(),
                    msg: format!("{key} caps only a size component written content or fill"),
                });
            }
        }
        Ok(Placement {
            at,
            extent,
            max_w: self.max_w,
            max_h: self.max_h,
            rotate: self.rotate,
        })
    }
}

/// The coordinate each axis index names in a refusal.
const AXIS_COORDS: [&str; 2] = ["x", "y"];

/// An image has no intrinsic size (`layout`, "Intrinsic sizes"), so each axis of its box is
/// authored: a number or reference `size` component, or a `to` whose corners share a sign.
fn require_authored_image_box(placement: &Placement) -> Result<(), TemplateError> {
    for (axis, coord) in AXIS_COORDS.into_iter().enumerate() {
        let refused = match &placement.extent {
            Extent::Size(size) => {
                matches!(size.0[axis], SizeValue::Content | SizeValue::Fill).then_some("size")
            }
            // An edge-relative `at` with a non-negative `to` is refused for every item before this.
            Extent::To(to) => {
                let at_is_edge = placement
                    .at
                    .as_ref()
                    .is_some_and(|at| at.0[axis].is_sign_negative());
                (at_is_edge != to.0[axis].is_sign_negative()).then_some("to")
            }
        };
        if let Some(key) = refused {
            return Err(TemplateError::Validation {
                path: key.to_string(),
                msg: format!(
                    "an image has no intrinsic size, so its extent on {coord} must be a number, a reference, or a to whose corners share a sign"
                ),
            });
        }
    }
    Ok(())
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

        let sides = [padding.top, padding.right, padding.bottom, padding.left];
        if sides.iter().any(|side| *side < 0.0) {
            return Err(TemplateError::Validation {
                path: "padding".to_string(),
                msg: "padding values must be >= 0".to_string(),
            });
        }

        Ok(padding)
    }
}

impl ContainerRaw {
    pub(crate) fn try_into_container(
        self,
        placement: PlacementRaw,
        is_packed: bool,
    ) -> Result<LayoutItem, TemplateError> {
        let flow = match self.flow {
            Some(flow_raw) => {
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
            None => None,
        };

        let placement = placement.into_placement("container", is_packed)?;
        let padding = match self.padding {
            None => Padding::ZERO,
            Some(padding) => Padding::try_from(padding)?,
        };

        let shape = match self.shape {
            Some(s) => match s.as_str() {
                "rect" => Shape::Rect,
                "ellipse" => Shape::Ellipse,
                _ => {
                    return Err(TemplateError::Validation {
                        path: "shape".to_string(),
                        msg: format!("unknown shape '{s}', accepted values are: rect, ellipse"),
                    });
                }
            },
            None => Shape::Rect,
        };

        let rounded = self.rounded;

        if rounded.is_some() && shape == Shape::Ellipse {
            return Err(TemplateError::Validation {
                path: "rounded".to_string(),
                msg: "rounded is only supported on rect containers".to_string(),
            });
        }

        let repeat = match self.repeat {
            Some(r) => {
                if !is_packed {
                    return Err(TemplateError::Validation {
                        path: "repeat".to_string(),
                        msg: "repeat is only valid on packed containers inside a flow layout"
                            .to_string(),
                    });
                }
                Some(r)
            }
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
            shape,
            stroke: self.stroke,
            background: self.background,
            rounded,
            padding,
            flow,
            repeat,
            items,
        })
    }
}

impl LayoutItem {
    pub(crate) fn try_from_raw(raw: LayoutItemRaw, is_packed: bool) -> Result<Self, TemplateError> {
        match raw {
            LayoutItemRaw::Text(placement, raw) => {
                // Also checked in `TemplateDefinition::validate`, which covers items built by any
                // other route; here so an API caller gets the error with its JSON path.
                if let Some(crate::raw::Dynamic::Literal(weight)) = raw.font_weight {
                    if !font_weight_ok(weight.into()) {
                        return Err(TemplateError::Validation {
                            path: "text.font_weight".to_string(),
                            msg: format!(
                                "font_weight must be a multiple of 100 between 100 and 900, got {weight}"
                            ),
                        });
                    }
                }
                if let Some(spacing) = raw.line_spacing {
                    if !spacing.is_finite() || spacing <= 0.0 {
                        return Err(TemplateError::Validation {
                            path: "line_spacing".to_string(),
                            msg: format!(
                                "line_spacing must be a finite number greater than 0, got {spacing}"
                            ),
                        });
                    }
                }
                Ok(LayoutItem::Text {
                    value: raw.value,
                    placement: placement.into_placement("text", is_packed)?,
                    font_size: raw.font_size,
                    font_weight: raw.font_weight,
                    color: raw.color,
                    wrap: raw.wrap,
                    line_spacing: raw.line_spacing,
                    alignment: raw.alignment,
                    overflow: raw.overflow,
                    when: raw.when,
                })
            }
            LayoutItemRaw::Qr(placement, raw) => Ok(LayoutItem::Qr {
                value: raw.value,
                placement: placement.into_placement("qr", is_packed)?,
                error_correction: raw.error_correction,
                module_size: raw.module_size,
                quiet_zone: raw.quiet_zone,
                when: raw.when,
            }),
            LayoutItemRaw::Image(placement, raw) => {
                let placement = placement.into_placement("image", is_packed)?;
                require_authored_image_box(&placement)?;
                Ok(LayoutItem::Image {
                    src: raw.src,
                    placement,
                    fit: raw.fit,
                    when: raw.when,
                })
            }
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
                    stroke: raw.stroke,
                    when: raw.when,
                })
            }
            LayoutItemRaw::Container(placement, raw) => {
                raw.try_into_container(placement, is_packed)
            }
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
            RawTemplateFormat::Sheet(SheetFormatRaw {
                paper_width,
                paper_height,
                label_width,
                label_height,
                positions,
            }) => Ok(TemplateFormat::Sheet {
                paper_width,
                paper_height,
                label_width,
                label_height,
                positions,
            }),
            RawTemplateFormat::Single(SingleFormatRaw {
                width,
                height,
                media_width,
            }) => Ok(TemplateFormat::Single {
                width: DynamicDimension::try_from(width)?,
                height,
                media_width,
            }),
        }
    }
}

/// Store a declared default on `spec`. A tokened default (a string containing a brace) keeps its
/// text, for `resolve_environment` to resolve per request. A literal default is judged now by the
/// supplied-value rule of the parameter's type, bounds included, and stored coerced; one the rule
/// refuses, or reads as an omission, is refused at `default`.
fn set_default(spec: &mut ParamSpec, raw: serde_yaml_ng::Value) -> Result<(), TemplateError> {
    if let serde_yaml_ng::Value::String(text) = &raw {
        if text.contains('{') || text.contains('}') {
            spec.default = Some(ParamValue::String(text.clone()));
            return Ok(());
        }
    }
    let refused = |msg: String| TemplateError::Validation {
        path: "default".to_string(),
        msg,
    };
    let value = serde_json::to_value(&raw)
        .map_err(|err| refused(format!("default cannot be read as a value: {err}")))?;
    match crate::render::coerce_param_value(&value, spec) {
        Ok(Some(coerced)) => {
            spec.default = Some(coerced.value);
            spec.default_instant = coerced.instant;
            Ok(())
        }
        // Only a blank string or a non-finite number reads as an omission here: YAML `null` is
        // refused before this.
        Ok(None) => Err(refused(
            "default must be a value of the parameter's type, not blank or non-finite".to_string(),
        )),
        Err(refusal) => Err(refused(format!("default {}", refusal.message))),
    }
}

/// A `list` default: a sequence of strings, element errors naming the element's position.
fn list_default(default_raw: serde_yaml_ng::Value) -> Result<ParamValue, TemplateError> {
    let serde_yaml_ng::Value::Sequence(seq) = default_raw else {
        return Err(TemplateError::Validation {
            path: "default".to_string(),
            msg: "default for a list parameter must be a sequence of strings".to_string(),
        });
    };
    let mut items = Vec::with_capacity(seq.len());
    for (idx, elem) in seq.into_iter().enumerate() {
        match elem {
            serde_yaml_ng::Value::String(s) => items.push(s),
            _ => {
                return Err(TemplateError::Validation {
                    path: format!("default[{idx}]"),
                    msg: format!("list default element at position {idx} must be a string"),
                });
            }
        }
    }
    Ok(ParamValue::List(items))
}

impl TryFrom<RawParamEntry> for ParamSpec {
    type Error = TemplateError;

    fn try_from(raw: RawParamEntry) -> Result<Self, Self::Error> {
        // The parameters spec's type table: the attributes each type admits besides `description`.
        let (type_name, attributes): (&str, &[&str]) = match raw.param_type {
            RawParamType::String => ("string", &["default", "multiline"]),
            RawParamType::Integer => ("integer", &["default", "min", "max"]),
            RawParamType::Number => ("number", &["default", "min", "max"]),
            RawParamType::Boolean => ("boolean", &["default"]),
            RawParamType::Enum => ("enum", &["values", "default"]),
            RawParamType::Datetime => ("datetime", &["default", "time"]),
            RawParamType::List => ("list", &["default"]),
        };
        let present = [
            ("default", raw.default.is_some()),
            ("min", raw.min.is_some()),
            ("max", raw.max.is_some()),
            ("multiline", raw.multiline.is_some()),
            ("values", raw.values.is_some()),
            ("time", raw.time.is_some()),
        ];
        if let Some((key, _)) = present
            .iter()
            .find(|(key, is_present)| *is_present && !attributes.contains(key))
        {
            return Err(TemplateError::Validation {
                path: key.to_string(),
                msg: format!("{key} is not an attribute of a {type_name} parameter"),
            });
        }

        let param_type = match raw.param_type {
            RawParamType::String => ParamType::String {
                multiline: raw.multiline.unwrap_or(false),
            },
            RawParamType::Integer => ParamType::Integer,
            RawParamType::Number => ParamType::Number,
            RawParamType::Boolean => ParamType::Boolean,
            RawParamType::Enum => ParamType::Enum {
                values: raw.values.unwrap_or_default(),
            },
            RawParamType::Datetime => ParamType::Datetime {
                time: raw.time.unwrap_or(false),
            },
            RawParamType::List => ParamType::List,
        };

        if let ParamType::Enum { values } = &param_type {
            let refused = |msg: &str| TemplateError::Validation {
                path: "values".to_string(),
                msg: msg.to_string(),
            };
            if values.is_empty() {
                return Err(refused("enum values must not be empty"));
            }
            if values.iter().any(|value| value.trim().is_empty()) {
                return Err(refused("enum values must not contain an empty value"));
            }
        }

        let mut spec = ParamSpec {
            param_type,
            default: None,
            default_instant: None,
            min: raw.min,
            max: raw.max,
            description: raw.description,
        };
        match raw.default {
            None => {}
            Some(default_raw) if spec.param_type == ParamType::List => {
                spec.default = Some(list_default(default_raw)?);
            }
            Some(serde_yaml_ng::Value::Sequence(_)) => {
                return Err(TemplateError::Validation {
                    path: "default".to_string(),
                    msg: "sequence default is only supported on list parameters".to_string(),
                });
            }
            Some(default_raw) => set_default(&mut spec, default_raw)?,
        }
        Ok(spec)
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

        let mut params = indexmap::IndexMap::new();
        for entry in raw.params {
            let key = entry.name.clone();
            if params.contains_key(&key) {
                return Err(TemplateError::Validation {
                    path: format!("params.{key}"),
                    msg: format!("duplicate parameter name '{key}'"),
                });
            }
            let spec = ParamSpec::try_from(entry)
                .map_err(|err| err.with_prefix(&format!("params.{key}")))?;
            params.insert(key, spec);
        }

        let format = TemplateFormat::try_from(raw.format)?;

        let mut repeated_in_scope = Vec::new();
        validate_repetition_layout(&items, &params, "layout", &mut repeated_in_scope)?;

        let content = TemplateContent {
            name: raw.name,
            description: raw.description.unwrap_or_default(),
            categories: raw.categories,
            unit: raw.unit,
            dpi: raw.dpi,
            format,
            params,
            layout: Layout::Items(items),
        };
        for name in content.weight_params() {
            // An undeclared or mistyped reference is refused by `validate_references`.
            let default = content
                .params
                .get(name)
                .and_then(|spec| spec.default.as_ref());
            if let Some(&ParamValue::Integer(weight)) = default {
                if !font_weight_ok(weight) {
                    return Err(TemplateError::Validation {
                        path: format!("params.{name}.default"),
                        msg: format!(
                            "a font_weight reads this parameter, so its default must be a multiple of 100 between 100 and 900, got {weight}"
                        ),
                    });
                }
            }
        }
        Ok(content)
    }
}

fn validate_repetition_layout(
    items: &[LayoutItem],
    params: &indexmap::IndexMap<String, ParamSpec>,
    path_prefix: &str,
    repeated_in_scope: &mut Vec<String>,
) -> Result<(), TemplateError> {
    for (idx, item) in items.iter().enumerate() {
        let path = format!("{path_prefix}[{idx}]");
        validate_repetition_item(item, params, &path, repeated_in_scope)?;
    }
    Ok(())
}

fn validate_repetition_item(
    item: &LayoutItem,
    params: &indexmap::IndexMap<String, ParamSpec>,
    path: &str,
    repeated_in_scope: &mut Vec<String>,
) -> Result<(), TemplateError> {
    match item {
        LayoutItem::Container { repeat, items, .. } => {
            if let Some(rep_name) = repeat {
                let repeat_path = format!("{path}.repeat");
                let spec = params
                    .get(rep_name)
                    .ok_or_else(|| TemplateError::Validation {
                        path: repeat_path.clone(),
                        msg: format!("repeat references undeclared parameter '{rep_name}'"),
                    })?;

                if !matches!(spec.param_type, ParamType::List) {
                    return Err(TemplateError::Validation {
                        path: repeat_path.clone(),
                        msg: format!(
                            "repeat references parameter '{rep_name}' declared as type: {}, but repeat requires a list",
                            spec.param_type.type_name()
                        ),
                    });
                }

                if repeated_in_scope.iter().any(|r| r == rep_name) {
                    return Err(TemplateError::Validation {
                        path: repeat_path,
                        msg: format!(
                            "nested repeat over '{rep_name}' is not allowed: parameter is already repeated by an enclosing container"
                        ),
                    });
                }

                repeated_in_scope.push(rep_name.clone());
                let res = validate_repetition_layout(
                    items,
                    params,
                    &format!("{path}.items"),
                    repeated_in_scope,
                );
                repeated_in_scope.pop();
                res?;
            } else {
                validate_repetition_layout(
                    items,
                    params,
                    &format!("{path}.items"),
                    repeated_in_scope,
                )?;
            }
        }
        LayoutItem::Text { value, .. } | LayoutItem::Qr { value, .. } => {
            check_scoped_tokens(value, &format!("{path}.value"), repeated_in_scope)?;
        }
        LayoutItem::Image { src, .. } => {
            check_scoped_tokens(src, &format!("{path}.src"), repeated_in_scope)?;
        }
        LayoutItem::Line { .. } => {}
    }
    Ok(())
}

fn check_scoped_tokens(
    text: &str,
    path: &str,
    repeated_in_scope: &[String],
) -> Result<(), TemplateError> {
    if repeated_in_scope.is_empty() {
        return Ok(());
    }
    for scanned in crate::interpolation::scan_tokens(text) {
        if let Ok(token) = crate::interpolation::parse(scanned.raw) {
            if let crate::interpolation::Source::Bare(name) = token.source {
                if repeated_in_scope.iter().any(|r| r == name) {
                    match token.reader {
                        Some(crate::interpolation::Reader::Join(_)) => {
                            return Err(TemplateError::Validation {
                                path: path.to_string(),
                                msg: format!(
                                    "template contains '{}': join cannot be used on '{name}' inside a repeat over that parameter",
                                    scanned.raw
                                ),
                            });
                        }
                        Some(crate::interpolation::Reader::Format(fmt)) => {
                            return Err(TemplateError::Validation {
                                path: path.to_string(),
                                msg: format!(
                                    "template contains '{}': format '{fmt}' can only be applied to an instant (sys.now or type: datetime parameter)",
                                    scanned.raw
                                ),
                            });
                        }
                        None => {}
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::models::Shape;
    use crate::templates::TemplateContent;

    fn try_build(layout_yaml: &str) -> Result<TemplateContent, String> {
        let yaml = format!(
            "name: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 10\n  height: 10\nlayout:\n{layout_yaml}"
        );
        crate::parse::parse_template(&yaml).map_err(|e| e.to_string())
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

    /// A cap binds only a `size` component written `content` or `fill`, so beside `to` it is refused.
    #[test]
    fn to_with_max_w_is_refused() {
        let err = try_build("  - type: text\n    value: \"x\"\n    at: [0,0]\n    to: [10,5]\n    max_w: 8\n    font_size: 8\n").unwrap_err();
        assert!(
            err.contains("layout[0].max_w"),
            "expected the cap's path in {err}"
        );
    }

    #[test]
    fn container_with_neither_is_refused() {
        let err = try_build("  - type: container\n    at: [0,0]\n    items: []\n").unwrap_err();
        assert!(
            err.contains("layout[0]"),
            "expected the item's path in {err}"
        );
    }

    /// Load `param_yaml` as the attributes of one parameter `p` of an otherwise empty template.
    fn try_build_param(param_yaml: &str) -> Result<crate::models::ParamSpec, String> {
        let yaml = format!(
            "name: T\nunit: mm\ndpi: 200\nformat: {{ type: single, width: 10, height: 10 }}\nparams:\n  - name: p\n    {}\nlayout: []\n",
            param_yaml.lines().collect::<Vec<_>>().join("\n    ")
        );
        let content = crate::parse::parse_template(&yaml).map_err(|e| e.to_string())?;
        Ok(content.params["p"].clone())
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
        assert!(try_build_param("type: datetime\ndefault:\n").is_err());
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
            try_build_param("type: number\nmin: \"twenty\"\n").is_err(),
            "a non-numeric min must fail to load, not resolve to no min"
        );
        assert!(try_build_param("type: number\nmax: [1, 2]\n").is_err());
        assert!(
            try_build_param("type: string\nmultiline: \"yes\"\n").is_err(),
            "a non-boolean multiline must fail to load, not resolve to false"
        );
        assert!(try_build_param("type: enum\nvalues: 3\n").is_err());

        let ok = try_build_param("type: number\nmin: 25\nmax: 300\n").unwrap();
        assert_eq!(ok.min, Some(25.0));
        assert_eq!(ok.max, Some(300.0));
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
        ] {
            let msg = try_build_param(yaml).expect_err("enum: must be refused as unknown field");
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
    fn list_param_conversions_and_refusals() {
        let ok = try_build_param(
            "type: list\ndefault: [CONSUMABLE, KIDS]\ndescription: \"Asset tags\"\n",
        )
        .unwrap();
        assert_eq!(ok.param_type, crate::models::ParamType::List);
        assert_eq!(
            ok.default,
            Some(crate::models::ParamValue::List(vec![
                "CONSUMABLE".to_string(),
                "KIDS".to_string()
            ]))
        );
        assert_eq!(ok.description, Some("Asset tags".to_string()));

        // default: [] is a present, empty list
        let empty_list = try_build_param("type: list\ndefault: []\n").unwrap();
        assert_eq!(
            empty_list.default,
            Some(crate::models::ParamValue::List(vec![]))
        );

        let absent_key = try_build_param("type: list\n").unwrap();
        assert_eq!(absent_key.default, None);

        // Attributes outside the list row refused
        for forbidden in [
            "min: 0",
            "max: 100",
            "multiline: true",
            "values: [a, b]",
            "time: true",
            "time: false",
            "format: whatever",
        ] {
            let yaml = format!("type: list\n{forbidden}\n");
            let err =
                try_build_param(&yaml).expect_err(&format!("expected refusal for {forbidden}"));
            assert!(!err.is_empty(), "expected error message for {forbidden}");
        }

        // Scalar default, mapping default, non-string element kinds refused with parameter named
        let t_scalar = try_build_template_with_param("tags", "type: list\ndefault: \"CONSUMABLE\"")
            .unwrap_err();
        assert!(
            t_scalar.contains("params.tags.default"),
            "expected param name in error: {t_scalar}"
        );

        let t_map =
            try_build_template_with_param("tags", "type: list\ndefault: { a: b }").unwrap_err();
        assert!(
            t_map.contains("params.tags.default"),
            "expected param name in error: {t_map}"
        );

        let t_num_elem =
            try_build_template_with_param("codes", "type: list\ndefault: [1, true]").unwrap_err();
        assert!(
            t_num_elem.contains("params.codes.default[0]"),
            "expected element pos 0 in error: {t_num_elem}"
        );

        let t_bool_elem =
            try_build_template_with_param("codes", "type: list\ndefault: [\"ok\", true]")
                .unwrap_err();
        assert!(
            t_bool_elem.contains("params.codes.default[1]"),
            "expected element pos 1 in error: {t_bool_elem}"
        );

        let t_null_elem =
            try_build_template_with_param("codes", "type: list\ndefault: [\"ok\", null]")
                .unwrap_err();
        assert!(
            t_null_elem.contains("params.codes.default[1]"),
            "expected element pos 1 in error: {t_null_elem}"
        );

        let t_nested =
            try_build_template_with_param("tags", "type: list\ndefault: [[a, b]]").unwrap_err();
        assert!(
            t_nested.contains("params.tags.default[0]"),
            "expected element pos 0 in error: {t_nested}"
        );

        // Sequence default on non-list types is refused naming parameter
        let t_str_seq =
            try_build_template_with_param("title", "type: string\ndefault: [A, B]").unwrap_err();
        assert!(
            t_str_seq.contains("params.title.default"),
            "expected param name in error: {t_str_seq}"
        );
        assert!(
            t_str_seq.contains("sequence default is only supported on list parameters"),
            "expected sequence default message: {t_str_seq}"
        );

        let t_int_seq =
            try_build_template_with_param("count", "type: integer\ndefault: [1, 2]").unwrap_err();
        assert!(
            t_int_seq.contains("params.count.default"),
            "expected param name in error: {t_int_seq}"
        );

        let t_dt_seq = try_build_template_with_param(
            "printed_on",
            "type: datetime\ndefault: [\"2026-01-01\"]",
        )
        .unwrap_err();
        assert!(
            t_dt_seq.contains("params.printed_on.default"),
            "expected param name in error: {t_dt_seq}"
        );
    }

    fn try_build_template_with_param(
        param_name: &str,
        param_yaml: &str,
    ) -> Result<crate::templates::TemplateContent, String> {
        let yaml = format!(
            "name: test\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 50\n  height: 50\nparams:\n  - name: {param_name}\n    {}\nlayout: []\n",
            param_yaml.lines().collect::<Vec<_>>().join("\n    ")
        );
        crate::parse::parse_template(&yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn shape_paint_container_refusals_and_defaults() {
        // stroke missing thickness
        let err = try_build(
            "  - type: container\n    at: [0,0]\n    size: [10,10]\n    stroke:\n      color: red\n    items: []\n",
        )
        .unwrap_err();
        assert!(
            err.contains("stroke") && err.contains("missing field `thickness`"),
            "expected the key and the missing field in {err}"
        );

        // valid stroke defaults color to black
        let template = try_build(
            "  - type: container\n    at: [0,0]\n    size: [10,10]\n    stroke:\n      thickness: 0.5\n    items: []\n",
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
            assert_eq!(stroke.color, crate::models::Color::black());
            assert_eq!(stroke.color.hex(), "#000000");
            assert!(background.is_none());
            assert!(rounded.is_none());
        } else {
            panic!("expected container");
        }

        // valid stroke with custom color, background, and rounded
        let template = try_build("  - type: container\n    at: [0,0]\n    size: [10,10]\n    stroke:\n      thickness: 0.0001\n      color: '#FF00cc'\n    background: blue\n    rounded: 0.0001\n    items: []\n").unwrap();
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
            assert_eq!(stroke.color.hex(), "#ff00cc");
            assert_eq!(background.as_ref().unwrap().hex(), "#0000ff");
            assert_eq!(rounded.unwrap(), 0.0001);
        } else {
            panic!("expected container");
        }
    }

    #[test]
    fn shape_paint_line_refusals() {
        // line stroke required
        let err = try_build("  - type: line\n    at: [0,0]\n    to: [5,5]\n").unwrap_err();
        assert!(
            err.contains("missing field `stroke`"),
            "expected missing field stroke in {err}"
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
            assert_eq!(stroke.thickness, 0.5);
            assert_eq!(stroke.color, crate::models::Color::black());
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn container_shape_conversion_and_refusals() {
        // 1. Default is rect when shape is omitted
        let template =
            try_build("  - type: container\n    at: [0,0]\n    size: [10,10]\n    items: []\n")
                .unwrap();
        let crate::models::Layout::Items(items) = &template.layout;
        if let crate::models::LayoutItem::Container { shape, .. } = &items[0] {
            assert_eq!(*shape, Shape::Rect);
        } else {
            panic!("expected container");
        }

        // 2. Each accepted value parses
        for (val, expected) in [("rect", Shape::Rect), ("ellipse", Shape::Ellipse)] {
            let yaml =
                format!("  - type: container\n    at: [0,0]\n    size: [10,10]\n    shape: {val}\n    items: []\n");
            let template = try_build(&yaml).unwrap();
            let crate::models::Layout::Items(items) = &template.layout;
            if let crate::models::LayoutItem::Container { shape, .. } = &items[0] {
                assert_eq!(*shape, expected);
            } else {
                panic!("expected container");
            }
        }

        // 3. polygon is refused naming value and set
        let err =
            try_build("  - type: container\n    at: [0,0]\n    size: [10,10]\n    shape: polygon\n    items: []\n")
                .unwrap_err();
        assert!(err.contains("layout[0].shape"), "expected path in {err}");
        assert!(err.contains("polygon"), "expected value in {err}");
        assert!(
            err.contains("accepted values are: rect, ellipse"),
            "expected accepted set in {err}"
        );

        // 4. Rect (case-sensitive) is refused naming value and set
        let err = try_build("  - type: container\n    at: [0,0]\n    size: [10,10]\n    shape: Rect\n    items: []\n")
            .unwrap_err();
        assert!(err.contains("layout[0].shape"), "expected path in {err}");
        assert!(err.contains("Rect"), "expected value in {err}");
        assert!(
            err.contains("accepted values are: rect, ellipse"),
            "expected accepted set in {err}"
        );

        // 5. shape on text is refused
        let err = try_build(
            "  - type: text\n    value: hi\n    at: [0,0]\n    font_size: 8\n    shape: rect\n",
        )
        .unwrap_err();
        assert!(
            err.contains("unknown field `shape`"),
            "expected unknown field shape in {err}"
        );

        // 6. shape on qr is refused
        let err =
            try_build("  - type: qr\n    value: hi\n    at: [0,0]\n    shape: rect\n").unwrap_err();
        assert!(
            err.contains("unknown field `shape`"),
            "expected unknown field shape in {err}"
        );

        // 7. shape on image is refused
        let err = try_build(
            "  - type: image\n    src: logo.png\n    at: [0,0]\n    size: [5,5]\n    shape: rect\n",
        )
        .unwrap_err();
        assert!(
            err.contains("unknown field `shape`"),
            "expected unknown field shape in {err}"
        );

        // 8. shape on line is refused
        let err = try_build("  - type: line\n    at: [0,0]\n    to: [5,5]\n    shape: rect\n")
            .unwrap_err();
        assert!(
            err.contains("unknown field `shape`"),
            "expected unknown field shape in {err}"
        );

        // 9. rounded on ellipse is refused
        let err = try_build("  - type: container\n    at: [0,0]\n    size: [10,10]\n    shape: ellipse\n    rounded: 1.0\n    items: []\n").unwrap_err();
        assert!(err.contains("layout[0].rounded"), "expected path in {err}");
        assert!(
            err.contains("rounded is only supported on rect containers"),
            "expected message in {err}"
        );
    }

    fn try_build_with_params(
        params_yaml: &str,
        layout_yaml: &str,
    ) -> Result<crate::templates::TemplateContent, String> {
        let yaml = format!(
            "name: test\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 50\n  height: 50\nparams:\n{params_yaml}\nlayout:\n{layout_yaml}"
        );
        crate::parse::parse_template(&yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn repeat_parent_flow_refusals() {
        // 1.4: repeat on a root-level container is refused naming the key and the layout path
        let err = try_build_with_params(
            "  - name: items\n    type: list\n",
            "  - type: container\n    repeat: items\n    at: [0, 0]\n    size: [50, 50]\n    items: []\n",
        )
        .unwrap_err();
        assert!(err.contains("layout[0].repeat"), "expected path in {err}");
        assert!(
            err.contains("repeat is only valid on packed containers inside a flow layout"),
            "expected message in {err}"
        );

        // 1.4: repeat on a container inside an absolute-positioned container (no flow) is refused
        let err = try_build_with_params(
            "  - name: items\n    type: list\n",
            "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    items:\n      - type: container\n        repeat: items\n        size: [10, 10]\n        at: [0, 0]\n        items: []\n",
        )
        .unwrap_err();
        assert!(
            err.contains("layout[0].items[0].repeat"),
            "expected path in {err}"
        );
        assert!(
            err.contains("repeat is only valid on packed containers inside a flow layout"),
            "expected message in {err}"
        );
    }

    #[test]
    fn repeat_undeclared_and_type_refusals() {
        // 2.2: repeat naming an undeclared parameter is refused with expected path and message
        let err = try_build_with_params(
            "  - name: other\n    type: list\n",
            "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: { direction: column }\n    items:\n      - type: container\n        repeat: missing\n        size: [10, 10]\n        items: []\n",
        )
        .unwrap_err();
        assert!(
            err.contains("layout[0].items[0]"),
            "expected container path in {err}"
        );
        assert!(
            err.contains("repeat references undeclared parameter 'missing'"),
            "expected message in {err}"
        );

        // 2.3: repeat naming a string / integer / datetime / enum parameter is refused
        for (name, param_def, expected_type) in [
            ("title", "type: string", "string"),
            ("count", "type: integer", "integer"),
            ("printed_on", "type: datetime", "datetime"),
            ("mode", "type: enum\n    values: [a, b]", "enum"),
        ] {
            let params = format!("  - name: {name}\n    {param_def}\n");
            let layout = format!(
                "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: {{ direction: column }}\n    items:\n      - type: container\n        repeat: {name}\n        size: [10, 10]\n        items: []\n"
            );
            let err = try_build_with_params(&params, &layout).unwrap_err();
            assert!(
                err.contains("layout[0].items[0]"),
                "expected container path in {err}"
            );
            assert!(
                err.contains(&format!(
                    "repeat references parameter '{name}' declared as type: {expected_type}"
                )),
                "expected type in message for {name}: {err}"
            );
        }
    }

    #[test]
    fn repeat_nesting_and_scoped_token_refusals() {
        // 2.4: Nested repeat over the same list is refused at the inner container path
        let params = "  - name: tags\n    type: list\n";
        let layout = "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: { direction: column }\n    items:\n      - type: container\n        repeat: tags\n        size: [10, 10]\n        flow: { direction: column }\n        items:\n          - type: container\n            repeat: tags\n            size: [10, 10]\n            items: []\n";
        let err = try_build_with_params(params, layout).unwrap_err();
        assert!(
            err.contains("layout[0].items[0].items[0]"),
            "expected inner container path in {err}"
        );
        assert!(
            err.contains("nested repeat over 'tags' is not allowed"),
            "expected message in {err}"
        );

        // 2.4: Nested repeat over two different lists is accepted
        let params = "  - name: tags\n    type: list\n  - name: codes\n    type: list\n";
        let layout = "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: { direction: column }\n    items:\n      - type: container\n        repeat: tags\n        size: [10, 10]\n        flow: { direction: column }\n        items:\n          - type: container\n            repeat: codes\n            size: [10, 10]\n            items: []\n";
        assert!(try_build_with_params(params, layout).is_ok());

        // 2.5: {p:join(',')} inside a repeat over p is refused
        let layout = "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: { direction: column }\n    items:\n      - type: container\n        repeat: tags\n        size: [10, 10]\n        items:\n          - type: text\n            value: \"{tags:join(',')}\"\n            size: [10, 5]\n            font_size: 8\n";
        let err = try_build_with_params("  - name: tags\n    type: list\n", layout).unwrap_err();
        assert!(
            err.contains("layout[0].items[0].items[0]"),
            "expected text item path in {err}"
        );
        assert!(
            err.contains("join cannot be used on 'tags' inside a repeat over that parameter"),
            "expected message in {err}"
        );

        // 2.6: {p:long_date} inside a repeat over p is refused as format on non-instant
        let layout = "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: { direction: column }\n    items:\n      - type: container\n        repeat: tags\n        size: [10, 10]\n        items:\n          - type: text\n            value: \"{tags:long_date}\"\n            size: [10, 5]\n            font_size: 8\n";
        let err = try_build_with_params("  - name: tags\n    type: list\n", layout).unwrap_err();
        assert!(
            err.contains("layout[0].items[0].items[0]"),
            "expected text item path in {err}"
        );
        assert!(
            err.contains("format 'long_date' can only be applied to an instant"),
            "expected message in {err}"
        );

        // Bare {tags} inside repeat over tags is accepted
        let layout = "  - type: container\n    at: [0, 0]\n    size: [50, 50]\n    flow: { direction: column }\n    items:\n      - type: container\n        repeat: tags\n        size: [10, 10]\n        items:\n          - type: text\n            value: \"{tags}\"\n            size: [10, 5]\n            font_size: 8\n";
        assert!(try_build_with_params("  - name: tags\n    type: list\n", layout).is_ok());
    }

    #[test]
    fn duplicate_parameter_name_refused_in_declaration_order() {
        let yaml_dup = r#"
name: Test
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: title
    type: string
  - name: title
    type: string
layout: []
"#;
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml_dup).unwrap();
        let err = crate::templates::TemplateContent::try_from(raw).unwrap_err();
        match err {
            crate::errors::TemplateError::Validation { path, msg } => {
                assert_eq!(path, "params.title");
                assert_eq!(msg, "duplicate parameter name 'title'");
            }
            other => panic!("expected TemplateError::Validation, got {other:?}"),
        }

        // Reverse-alphabetical multi-duplicate: zebra before alpha
        let yaml_multi = r#"
name: Test
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: zebra
    type: string
  - name: zebra
    type: string
  - name: alpha
    type: string
  - name: alpha
    type: string
layout: []
"#;
        let raw: crate::raw::TemplateDefinitionRaw = serde_yaml_ng::from_str(yaml_multi).unwrap();
        let err = crate::templates::TemplateContent::try_from(raw).unwrap_err();
        match err {
            crate::errors::TemplateError::Validation { path, msg } => {
                assert_eq!(path, "params.zebra");
                assert_eq!(msg, "duplicate parameter name 'zebra'");
            }
            other => panic!("expected TemplateError::Validation, got {other:?}"),
        }
    }
}
