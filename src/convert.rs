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

impl TryFrom<LayoutItemRaw> for LayoutItem {
    type Error = TemplateError;

    fn try_from(raw: LayoutItemRaw) -> Result<Self, Self::Error> {
        match raw {
            LayoutItemRaw::Text(TextRaw {
                name,
                placement,
                font_size,
                multiline,
                alignment,
            }) => Ok(LayoutItem::Text {
                name,
                placement,
                font_size,
                multiline,
                alignment,
            }),
            LayoutItemRaw::Qr(raw) => Ok(LayoutItem::Qr {
                name: raw.name,
                placement: raw.placement,
                params: raw.params,
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
                    placement: raw.placement,
                    fit: raw.fit,
                }),
            },
            LayoutItemRaw::Line(raw) => Ok(LayoutItem::Line {
                placement: raw.placement,
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
