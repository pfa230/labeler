use serde::de::IntoDeserializer;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};
use std::collections::BTreeMap;

use crate::models::{
    Alignment, Color, DynamicValue, ErrorCorrection, Fit, FlowOverflow, FontSize, Overflow,
    Position, SheetPosition, Stroke,
};

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RawParamType {
    String,
    Integer,
    Number,
    Boolean,
    Enum,
    Datetime,
    List,
}

/// One `params:` entry. Every attribute of every type is a field here, so serde names a bad key or
/// value by its own path; which attributes a type admits is checked in conversion.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawParamEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: RawParamType,
    #[serde(default)]
    pub default: Option<serde_yaml_ng::Value>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub multiline: Option<bool>,
    #[serde(default)]
    pub values: Option<Vec<String>>,
    #[serde(default)]
    pub time: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

pub type Dynamic<T> = DynamicValue<T>;

pub(crate) fn deserialize_when_map<'de, D>(
    deserializer: D,
) -> Result<Option<BTreeMap<String, String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WhenScalar {
        String(String),
        Bool(bool),
        Int(i64),
        Float(f64),
    }

    impl std::fmt::Display for WhenScalar {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                WhenScalar::String(s) => write!(f, "{s}"),
                WhenScalar::Bool(b) => write!(f, "{b}"),
                WhenScalar::Int(i) => write!(f, "{i}"),
                WhenScalar::Float(v) => write!(f, "{v}"),
            }
        }
    }

    let map = BTreeMap::<String, WhenScalar>::deserialize(deserializer)?;
    Ok(Some(
        map.into_iter().map(|(k, v)| (k, v.to_string())).collect(),
    ))
}

/// `format.width`: a number or reference, or a `{min, max}` range. Written by
/// hand because an untagged enum reports an unknown range key only as "did not match any variant".
#[derive(Debug, Clone, PartialEq)]
pub enum RawDimension {
    Dynamic {
        min: Option<Dynamic<f32>>,
        max: Option<Dynamic<f32>>,
    },
    Fixed(Dynamic<f32>),
}

impl<'de> Deserialize<'de> for RawDimension {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Range {
            #[serde(default)]
            min: Option<Dynamic<f32>>,
            #[serde(default)]
            max: Option<Dynamic<f32>>,
        }

        struct RawDimensionVisitor;

        impl<'de> serde::de::Visitor<'de> for RawDimensionVisitor {
            type Value = RawDimension;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number, a '{param_name}' reference, or a {min, max} range")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Dynamic::<f32>::deserialize(v.into_deserializer()).map(RawDimension::Fixed)
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Dynamic::<f32>::deserialize(v.into_deserializer()).map(RawDimension::Fixed)
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Dynamic::<f32>::deserialize(v.into_deserializer()).map(RawDimension::Fixed)
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Dynamic::<f32>::deserialize(v.into_deserializer()).map(RawDimension::Fixed)
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let Range { min, max } =
                    Range::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(RawDimension::Dynamic { min, max })
            }
        }

        deserializer.deserialize_any(RawDimensionVisitor)
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SheetFormatRaw {
    pub paper_width: f32,
    pub paper_height: f32,
    pub label_width: f32,
    pub label_height: f32,
    pub positions: Vec<SheetPosition>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SingleFormatRaw {
    pub width: RawDimension,
    pub height: Dynamic<f32>,
    #[serde(default)]
    pub media_width: Option<f32>,
}

/// Tagged by `type`; deserialized through `take_type` so a bad value names its key.
#[derive(Debug, Clone)]
pub enum RawTemplateFormat {
    Sheet(SheetFormatRaw),
    Single(SingleFormatRaw),
}

impl<'de> Deserialize<'de> for RawTemplateFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (kind, rest) = take_type::<D::Error>(Mapping::deserialize(deserializer)?)?;
        match kind.as_str() {
            "sheet" => Ok(RawTemplateFormat::Sheet(from_mapping(rest)?)),
            "single" => Ok(RawTemplateFormat::Single(from_mapping(rest)?)),
            other => Err(unknown_type(other, &["sheet", "single"])),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDefinitionRaw {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub unit: String,
    pub dpi: u32,
    pub format: RawTemplateFormat,
    #[serde(default)]
    pub params: Vec<RawParamEntry>,
    pub layout: Vec<LayoutItemRaw>,
}

pub type RawTemplate = TemplateDefinitionRaw;

/// Tagged by `type`. A boxed item's placement keys and its own keys deserialize separately, each
/// through `from_mapping`, because serde's derived tag and `flatten` both buffer the mapping and
/// lose the path to a bad value inside it.
#[derive(Debug)]
pub enum LayoutItemRaw {
    Text(PlacementRaw, TextRaw),
    Qr(PlacementRaw, QrRaw),
    Image(PlacementRaw, ImageRaw),
    Line(LineRaw),
    Container(PlacementRaw, ContainerRaw),
}

impl<'de> Deserialize<'de> for LayoutItemRaw {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (kind, mut rest) = take_type::<D::Error>(Mapping::deserialize(deserializer)?)?;
        if kind == "line" {
            return Ok(LayoutItemRaw::Line(from_mapping(rest)?));
        }
        let mut placement = Mapping::new();
        for key in PLACEMENT_KEYS {
            if let Some(value) = rest.remove(key) {
                placement.insert(Value::from(key), value);
            }
        }
        let placement = from_mapping(placement)?;
        match kind.as_str() {
            "text" => Ok(LayoutItemRaw::Text(placement, from_mapping(rest)?)),
            "qr" => Ok(LayoutItemRaw::Qr(placement, from_mapping(rest)?)),
            "image" => Ok(LayoutItemRaw::Image(placement, from_mapping(rest)?)),
            "container" => Ok(LayoutItemRaw::Container(placement, from_mapping(rest)?)),
            other => Err(unknown_type(
                other,
                &["text", "qr", "image", "line", "container"],
            )),
        }
    }
}

/// Remove and read the `type` tag of a tagged mapping, returning it with the remaining keys.
fn take_type<E: serde::de::Error>(mut mapping: Mapping) -> Result<(String, Mapping), E> {
    let tag = mapping
        .remove("type")
        .ok_or_else(|| E::missing_field("type"))?;
    let kind = String::deserialize(tag).map_err(|err| E::custom(format!("type: {err}")))?;
    Ok((kind, mapping))
}

fn unknown_type<E: serde::de::Error>(kind: &str, expected: &[&str]) -> E {
    let expected = expected
        .iter()
        .map(|kind| format!("`{kind}`"))
        .collect::<Vec<_>>()
        .join(", ");
    E::custom(format!(
        "type: unknown variant `{kind}`, expected one of {expected}"
    ))
}

/// Deserialize `mapping` as `T`, prefixing an error with its path inside `mapping`, so each nesting
/// level contributes one `<path>: ` segment to the message.
fn from_mapping<T, E>(mapping: Mapping) -> Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    serde_path_to_error::deserialize(Value::Mapping(mapping)).map_err(|err| {
        let path = err.path().to_string();
        let inner = err.into_inner();
        if path == "." {
            E::custom(inner)
        } else {
            E::custom(format!("{path}: {inner}"))
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRaw {
    pub value: String,
    pub font_size: FontSize,
    #[serde(default)]
    pub font_weight: Option<Dynamic<u16>>,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub line_spacing: Option<f32>,
    #[serde(default)]
    pub alignment: Alignment,
    #[serde(default)]
    pub overflow: Overflow,
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrRaw {
    pub value: String,
    #[serde(default)]
    pub error_correction: ErrorCorrection,
    #[serde(default)]
    pub module_size: Option<f32>,
    #[serde(default)]
    pub quiet_zone: f32,
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRaw {
    pub src: String,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineRaw {
    #[serde(default)]
    pub at: Position,
    pub to: Position,
    pub stroke: Stroke,
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct FlowRaw {
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub gap: Option<f32>,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub line_gap: Option<f32>,
    #[serde(default)]
    pub overflow: Option<FlowOverflow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerRaw {
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub stroke: Option<Stroke>,
    #[serde(default)]
    pub background: Option<Color>,
    #[serde(default)]
    pub rounded: Option<f32>,
    #[serde(default)]
    pub padding: Option<PaddingRaw>,
    #[serde(default)]
    pub flow: Option<FlowRaw>,
    #[serde(default)]
    pub repeat: Option<String>,
    pub items: Vec<LayoutItemRaw>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PaddingRaw {
    Uniform(f32),
    Trbl([f32; 4]),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RawSizeValue {
    Content,
    Fill,
    Dynamic(DynamicValue<f32>),
}

impl<'de> Deserialize<'de> for RawSizeValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawSizeValueVisitor;

        impl<'de> serde::de::Visitor<'de> for RawSizeValueVisitor {
            type Value = RawSizeValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("'content', 'fill', a number, or a '{param_name}' reference")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "content" => Ok(RawSizeValue::Content),
                    "fill" => Ok(RawSizeValue::Fill),
                    _ => DynamicValue::<f32>::deserialize(v.into_deserializer())
                        .map(RawSizeValue::Dynamic)
                        .map_err(|_: E| E::invalid_value(serde::de::Unexpected::Str(v), &self)),
                }
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DynamicValue::<f32>::deserialize(v.into_deserializer()).map(RawSizeValue::Dynamic)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DynamicValue::<f32>::deserialize(v.into_deserializer()).map(RawSizeValue::Dynamic)
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DynamicValue::<f32>::deserialize(v.into_deserializer()).map(RawSizeValue::Dynamic)
            }
        }

        deserializer.deserialize_any(RawSizeValueVisitor)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawSize(pub [RawSizeValue; 2]);

impl<'de> Deserialize<'de> for RawSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let arr = <[RawSizeValue; 2]>::deserialize(deserializer)?;
        Ok(RawSize(arr))
    }
}

/// The keys `LayoutItemRaw` routes to `PlacementRaw` rather than to the item's own struct.
const PLACEMENT_KEYS: [&str; 6] = ["at", "size", "to", "max_w", "max_h", "rotate"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRaw {
    #[serde(default)]
    pub at: Option<Position>,
    #[serde(default)]
    pub size: Option<RawSize>,
    #[serde(default)]
    pub to: Option<Position>,
    #[serde(default)]
    pub max_w: Option<f32>,
    #[serde(default)]
    pub max_h: Option<f32>,
    #[serde(default)]
    pub rotate: Option<f32>,
}
