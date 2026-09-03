use serde::Deserialize;
use std::collections::BTreeMap;

use crate::models::{
    Alignment, DynamicValue, Fit, FlowOverflow, FontSize, Overflow, Position, QrParams,
    SheetPosition,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RawColor(pub String);

impl<'de> Deserialize<'de> for RawColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawColorVisitor;
        impl<'de> serde::de::Visitor<'de> for RawColorVisitor {
            type Value = RawColor;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a colour string")
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(RawColor(v.to_string()))
            }
        }
        deserializer.deserialize_str(RawColorVisitor)
    }
}

/// `RawColor` deliberately fails `FromStr` unconditionally so that `DynamicValueVisitor::visit_str`
/// bypasses its `trimmed.parse::<T>()` fast-path and falls through to `RawColor::deserialize`,
/// preserving exact untrimmed string literal values (e.g. `" red "`, `" #ff0000 "`)
/// so that the declared string reaches the model and the read-back keeps its padding.
impl std::str::FromStr for RawColor {
    type Err = &'static str;

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err("raw colour strings are parsed via deserialization to preserve exact spelling")
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeRaw {
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub thickness: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub color: Option<Option<DynamicValue<RawColor>>>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RawParamType {
    String,
    Length,
    Integer,
    Number,
    Boolean,
    Enum,
    Datetime,
    List,
}

/// Every attribute a `datetime` parameter forbids is `Option<Option<T>>`: the outer layer is
/// presence (`None` = the key is absent) and the inner is the value (`Some(None)` = the key is
/// written and empty). Presence is what the datetime rules key off, and the inner type is what
/// keeps a malformed value a load-time error rather than a silently dropped field.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawParamSpec {
    #[serde(rename = "type")]
    pub param_type: RawParamType,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub default: Option<serde_yaml_ng::Value>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub min: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub max: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub multiline: Option<Option<bool>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub values: Option<Option<Vec<String>>>,
    /// Untyped on purpose: `format` is rejected on every parameter type, so any value at all must
    /// reach the pointed error message rather than a serde type error.
    #[serde(default, deserialize_with = "deserialize_present")]
    pub format: Option<serde_yaml_ng::Value>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub time: Option<Option<bool>>,
    #[serde(default)]
    pub description: Option<String>,
}

fn deserialize_present<'de, D>(deserializer: D) -> Result<Option<serde_yaml_ng::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_yaml_ng::Value::deserialize(deserializer).map(Some)
}

/// Presence-preserving and still typed: absent stays `None` via `#[serde(default)]`, an explicit
/// null becomes `Some(None)`, and a value of the wrong type is a deserialization error.
pub(crate) fn deserialize_present_typed<'de, D, T>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
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

    let map = Option::<BTreeMap<String, WhenScalar>>::deserialize(deserializer)?;
    Ok(map.map(|m| m.into_iter().map(|(k, v)| (k, v.to_string())).collect()))
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum RawDimension {
    Dynamic {
        #[serde(default)]
        min: Option<Dynamic<f32>>,
        #[serde(default)]
        max: Option<Dynamic<f32>>,
    },
    Fixed(Dynamic<f32>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RawTemplateFormat {
    Sheet {
        paper_width: f32,
        paper_height: f32,
        label_width: f32,
        label_height: f32,
        positions: Vec<SheetPosition>,
    },
    Single {
        width: RawDimension,
        height: RawDimension,
        #[serde(default)]
        media_width: Option<f32>,
    },
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RawParamEntry {
    pub name: String,
    #[serde(flatten)]
    pub spec: RawParamSpec,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDefinitionRaw {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub unit: String,
    pub dpi: u32,
    pub format: RawTemplateFormat,
    #[serde(default)]
    pub params: Vec<RawParamEntry>,
    pub layout: Vec<LayoutItemRaw>,
    #[serde(default)]
    pub version: Option<String>,
}

pub type RawTemplate = TemplateDefinitionRaw;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayoutItemRaw {
    Text(TextRaw),
    Qr(QrRaw),
    Image(ImageRaw),
    Line(LineRaw),
    Container(ContainerRaw),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRaw {
    pub value: String,
    #[serde(flatten)]
    pub placement: PlacementRaw,
    pub font_size: FontSize,
    #[serde(default)]
    pub font_weight: Option<Dynamic<u16>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub color: Option<Option<DynamicValue<RawColor>>>,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub multiline: Option<serde_yaml_ng::Value>,
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
    #[serde(flatten)]
    pub placement: PlacementRaw,
    #[serde(default)]
    pub params: Option<QrParams>,
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRaw {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub src: Option<String>,
    #[serde(flatten)]
    pub placement: PlacementRaw,
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
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub stroke: Option<Option<StrokeRaw>>,
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
    #[serde(flatten)]
    pub placement: PlacementRaw,
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub when: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub shape: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub stroke: Option<Option<StrokeRaw>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub background: Option<Option<DynamicValue<RawColor>>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub rounded: Option<Option<f32>>,
    #[serde(default)]
    pub padding: Option<PaddingRaw>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub flow: Option<Option<FlowRaw>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub repeat: Option<Option<String>>,
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
    Auto,
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
                formatter
                    .write_str("'content', 'fill', 'auto', a number, or a '{param_name}' reference")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let trimmed = v.trim();
                if trimmed == "content" {
                    return Ok(RawSizeValue::Content);
                }
                if trimmed == "fill" {
                    return Ok(RawSizeValue::Fill);
                }
                if trimmed == "auto" {
                    return Ok(RawSizeValue::Auto);
                }
                DynamicValue::<f32>::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(RawSizeValue::Dynamic)
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DynamicValue::<f32>::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(RawSizeValue::Dynamic)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DynamicValue::<f32>::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(RawSizeValue::Dynamic)
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DynamicValue::<f32>::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(RawSizeValue::Dynamic)
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

#[cfg(test)]
mod raw_tests {
    use super::*;

    #[test]
    fn text_raw_color_deserialization() {
        let make_yaml = |color_str: &str| {
            format!(
                r#"
value: "Hello"
font_size: 12
color: {color_str}
"#
            )
        };

        // Valid literal named color
        let raw: TextRaw = serde_yaml_ng::from_str(&make_yaml("red")).unwrap();
        assert_eq!(
            raw.color,
            Some(Some(DynamicValue::Literal(RawColor("red".to_string()))))
        );

        // Valid hex color
        let raw: TextRaw = serde_yaml_ng::from_str(&make_yaml("\"#ff4136\"")).unwrap();
        assert_eq!(
            raw.color,
            Some(Some(DynamicValue::Literal(RawColor("#ff4136".to_string()))))
        );

        // Valid reference
        let raw: TextRaw = serde_yaml_ng::from_str(&make_yaml("\"{brand}\"")).unwrap();
        assert_eq!(
            raw.color,
            Some(Some(DynamicValue::Ref("brand".to_string())))
        );

        // Non-string types rejected at deserialization
        for bad in ["16711680", "true", "[255, 0, 0]"] {
            let res = serde_yaml_ng::from_str::<TextRaw>(&make_yaml(bad));
            assert!(res.is_err(), "expected non-string '{bad}' to be rejected");
        }

        // Absent color is None
        let raw_no_color: TextRaw =
            serde_yaml_ng::from_str("value: \"Hello\"\nfont_size: 12\n").unwrap();
        assert_eq!(raw_no_color.color, None);

        // Null color is Some(None)
        let raw_null_color: TextRaw =
            serde_yaml_ng::from_str("value: \"Hello\"\nfont_size: 12\ncolor: null\n").unwrap();
        assert_eq!(raw_null_color.color, Some(None));
    }
}
