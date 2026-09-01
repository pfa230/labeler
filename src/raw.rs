use serde::Deserialize;
use std::collections::BTreeMap;

use crate::models::{
    Alignment, Color, DynamicValue, Fit, FlowOverflow, FontSize, Ink, Overflow, Position, QrParams,
    SheetPosition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorRaw(pub Color);

impl<'de> Deserialize<'de> for ColorRaw {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = ColorRaw;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a hex colour string ('#rgb', '#rgba', '#rrggbb', '#rrggbbaa') or one of the sixteen CSS Level 1 colour names",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                parse_color(v).map(ColorRaw).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(ColorVisitor)
    }
}

pub fn parse_color(s: &str) -> Result<Color, String> {
    if let Some(hex_part) = s.strip_prefix('#') {
        let hex_bytes = hex_part.as_bytes();
        for &b in hex_bytes {
            if !b.is_ascii_hexdigit() {
                return Err(format!("invalid hex character in colour '{s}'"));
            }
        }
        let double_hex = |b: u8| -> u8 {
            let val = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => unreachable!(),
            };
            (val << 4) | val
        };
        let parse_byte = |slice: &[u8]| -> u8 {
            let s_str = std::str::from_utf8(slice).unwrap();
            u8::from_str_radix(s_str, 16).unwrap()
        };

        match hex_bytes.len() {
            3 => {
                let r = double_hex(hex_bytes[0]);
                let g = double_hex(hex_bytes[1]);
                let b = double_hex(hex_bytes[2]);
                Ok(Color::rgba(r, g, b, 255))
            }
            4 => {
                let r = double_hex(hex_bytes[0]);
                let g = double_hex(hex_bytes[1]);
                let b = double_hex(hex_bytes[2]);
                let a = double_hex(hex_bytes[3]);
                Ok(Color::rgba(r, g, b, a))
            }
            6 => {
                let r = parse_byte(&hex_bytes[0..2]);
                let g = parse_byte(&hex_bytes[2..4]);
                let b = parse_byte(&hex_bytes[4..6]);
                Ok(Color::rgba(r, g, b, 255))
            }
            8 => {
                let r = parse_byte(&hex_bytes[0..2]);
                let g = parse_byte(&hex_bytes[2..4]);
                let b = parse_byte(&hex_bytes[4..6]);
                let a = parse_byte(&hex_bytes[6..8]);
                Ok(Color::rgba(r, g, b, a))
            }
            _ => Err(format!(
                "invalid hex colour '{s}': expected 3, 4, 6, or 8 hexadecimal digits"
            )),
        }
    } else {
        match s.to_ascii_lowercase().as_str() {
            "black" => Ok(Color::rgba(0x00, 0x00, 0x00, 0xff)),
            "silver" => Ok(Color::rgba(0xc0, 0xc0, 0xc0, 0xff)),
            "gray" => Ok(Color::rgba(0x80, 0x80, 0x80, 0xff)),
            "white" => Ok(Color::rgba(0xff, 0xff, 0xff, 0xff)),
            "maroon" => Ok(Color::rgba(0x80, 0x00, 0x00, 0xff)),
            "red" => Ok(Color::rgba(0xff, 0x00, 0x00, 0xff)),
            "purple" => Ok(Color::rgba(0x80, 0x00, 0x80, 0xff)),
            "fuchsia" => Ok(Color::rgba(0xff, 0x00, 0xff, 0xff)),
            "green" => Ok(Color::rgba(0x00, 0x80, 0x00, 0xff)),
            "lime" => Ok(Color::rgba(0x00, 0xff, 0x00, 0xff)),
            "olive" => Ok(Color::rgba(0x80, 0x80, 0x00, 0xff)),
            "yellow" => Ok(Color::rgba(0xff, 0xff, 0x00, 0xff)),
            "navy" => Ok(Color::rgba(0x00, 0x00, 0x80, 0xff)),
            "blue" => Ok(Color::rgba(0x00, 0x00, 0xff, 0xff)),
            "teal" => Ok(Color::rgba(0x00, 0x80, 0x80, 0xff)),
            "aqua" => Ok(Color::rgba(0x00, 0xff, 0xff, 0xff)),
            _ => Err(format!("unknown colour '{s}'")),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeRaw {
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub thickness: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub color: Option<Option<ColorRaw>>,
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
    #[serde(
        default,
        rename = "enum",
        deserialize_with = "deserialize_present_typed"
    )]
    pub choices: Option<Option<Vec<serde_yaml_ng::Value>>>,
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

pub(crate) fn deserialize_dynamic_ink<'de, D>(
    deserializer: D,
) -> Result<Option<Dynamic<Ink>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DynamicInkVisitor;

    impl<'de> serde::de::Visitor<'de> for DynamicInkVisitor {
        type Value = Option<Dynamic<Ink>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a named colour, '#'-prefixed hex, or a '{param_name}' reference")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let trimmed = v.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.len() >= 2 {
                let inner = trimmed[1..trimmed.len() - 1].trim();
                return Ok(Some(DynamicValue::Ref(inner.to_string())));
            }
            let ink: Ink = trimmed.parse().map_err(serde::de::Error::custom)?;
            Ok(Some(DynamicValue::Literal(ink)))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(DynamicInkVisitor)
        }
    }

    deserializer.deserialize_any(DynamicInkVisitor)
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
    pub params: Option<BTreeMap<String, RawParamSpec>>,
    #[serde(default)]
    pub options: Option<RawOptions>,
    pub layout: Vec<LayoutItemRaw>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct RawOptions(pub BTreeMap<String, Vec<String>>);

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
    #[serde(default, deserialize_with = "deserialize_dynamic_ink")]
    pub ink: Option<Dynamic<Ink>>,
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
    #[serde(default, deserialize_with = "deserialize_when_map")]
    pub option: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub stroke: Option<Option<StrokeRaw>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub background: Option<Option<ColorRaw>>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub rounded: Option<Option<f32>>,
    #[serde(default)]
    pub padding: Option<PaddingRaw>,
    #[serde(default, deserialize_with = "deserialize_present_typed")]
    pub flow: Option<Option<FlowRaw>>,
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
    use crate::models::Color;
    use std::str::FromStr;

    #[test]
    fn text_raw_ink_deserialization() {
        let make_yaml = |ink_str: &str| {
            format!(
                r#"
value: "Hello"
font_size: 12
ink: {ink_str}
"#
            )
        };

        // Valid literal named color
        let raw: TextRaw = serde_yaml_ng::from_str(&make_yaml("red")).unwrap();
        assert_eq!(
            raw.ink,
            Some(DynamicValue::Literal(Ink::from_str("red").unwrap()))
        );

        // Valid hex color
        let raw: TextRaw = serde_yaml_ng::from_str(&make_yaml("\"#ff4136\"")).unwrap();
        assert_eq!(
            raw.ink,
            Some(DynamicValue::Literal(Ink::from_str("#ff4136").unwrap()))
        );

        // Valid reference
        let raw: TextRaw = serde_yaml_ng::from_str(&make_yaml("\"{brand}\"")).unwrap();
        assert_eq!(raw.ink, Some(DynamicValue::Ref("brand".to_string())));

        // Refused ink strings and values: chartreuse, redmm, "#ff0000in", "ff0000", "#ff000", "", 16711680
        for bad in [
            "chartreuse",
            "redmm",
            "\"#ff0000in\"",
            "\"ff0000\"",
            "\"#ff000\"",
            "\"\"",
            "16711680",
            "true",
            "[255, 0, 0]",
        ] {
            let res = serde_yaml_ng::from_str::<TextRaw>(&make_yaml(bad));
            assert!(res.is_err(), "expected ink '{bad}' to be rejected");
        }

        // Absent ink is None
        let raw_no_ink: TextRaw =
            serde_yaml_ng::from_str("value: \"Hello\"\nfont_size: 12\n").unwrap();
        assert_eq!(raw_no_ink.ink, None);

        // Null ink is None
        let raw_null_ink: TextRaw =
            serde_yaml_ng::from_str("value: \"Hello\"\nfont_size: 12\nink: null\n").unwrap();
        assert_eq!(raw_null_ink.ink, None);
    }

    #[test]
    fn parse_color_accepted_hex_forms() {
        // 3-digit hex (doubling)
        let c3 = parse_color("#f0c").expect("3-digit hex");
        assert_eq!(c3, Color::rgba(0xff, 0x00, 0xcc, 0xff));
        assert_eq!(c3.hex(), "#ff00ccff");

        // 4-digit hex (doubling with alpha)
        let c4 = parse_color("#F0F8").expect("4-digit hex");
        assert_eq!(c4, Color::rgba(0xff, 0x00, 0xff, 0x88));
        assert_eq!(c4.hex(), "#ff00ff88");

        // 6-digit hex
        let c6 = parse_color("#ff00ff").expect("6-digit hex");
        assert_eq!(c6, Color::rgba(0xff, 0x00, 0xff, 0xff));
        assert_eq!(c6.hex(), "#ff00ffff");

        // 8-digit hex
        let c8 = parse_color("#FF00FF80").expect("8-digit hex");
        assert_eq!(c8, Color::rgba(0xff, 0x00, 0xff, 0x80));
        assert_eq!(c8.hex(), "#ff00ff80");
    }

    #[test]
    fn parse_color_sixteen_css_level_1_names() {
        let expected = [
            ("black", Color::rgba(0x00, 0x00, 0x00, 0xff), "#000000ff"),
            ("silver", Color::rgba(0xc0, 0xc0, 0xc0, 0xff), "#c0c0c0ff"),
            ("gray", Color::rgba(0x80, 0x80, 0x80, 0xff), "#808080ff"),
            ("white", Color::rgba(0xff, 0xff, 0xff, 0xff), "#ffffffff"),
            ("maroon", Color::rgba(0x80, 0x00, 0x00, 0xff), "#800000ff"),
            ("red", Color::rgba(0xff, 0x00, 0x00, 0xff), "#ff0000ff"),
            ("purple", Color::rgba(0x80, 0x00, 0x80, 0xff), "#800080ff"),
            ("fuchsia", Color::rgba(0xff, 0x00, 0xff, 0xff), "#ff00ffff"),
            ("green", Color::rgba(0x00, 0x80, 0x00, 0xff), "#008000ff"),
            ("lime", Color::rgba(0x00, 0xff, 0x00, 0xff), "#00ff00ff"),
            ("olive", Color::rgba(0x80, 0x80, 0x00, 0xff), "#808000ff"),
            ("yellow", Color::rgba(0xff, 0xff, 0x00, 0xff), "#ffff00ff"),
            ("navy", Color::rgba(0x00, 0x00, 0x80, 0xff), "#000080ff"),
            ("blue", Color::rgba(0x00, 0x00, 0xff, 0xff), "#0000ffff"),
            ("teal", Color::rgba(0x00, 0x80, 0x80, 0xff), "#008080ff"),
            ("aqua", Color::rgba(0x00, 0xff, 0xff, 0xff), "#00ffffff"),
        ];

        for (name, color, canonical) in expected {
            let parsed =
                parse_color(name).unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));
            assert_eq!(parsed, color, "parsed {name}");
            assert_eq!(parsed.hex(), canonical, "hex for {name}");
        }

        // Case-insensitivity
        assert_eq!(parse_color("Red").unwrap(), Color::rgba(0xff, 0, 0, 255));
        assert_eq!(parse_color("RED").unwrap(), Color::rgba(0xff, 0, 0, 255));
        assert_eq!(parse_color("rEd").unwrap(), Color::rgba(0xff, 0, 0, 255));

        // Task 1.3: Assert red is #ff0000ff and not Typst's #ff4136
        let red = parse_color("red").unwrap();
        assert_eq!(red, Color::rgba(255, 0, 0, 255));
        assert_ne!(red, Color::rgba(0xff, 0x41, 0x36, 255));
    }

    #[test]
    fn parse_color_refusals() {
        // Five digits
        assert!(parse_color("#ff00f").is_err());
        // Missing #
        assert!(parse_color("ff00ff").is_err());
        // Non-hex character
        assert!(parse_color("#gg0000").is_err());
        assert!(parse_color("#12g4").is_err());
        // Invalid lengths
        assert!(parse_color("#f").is_err());
        assert!(parse_color("#12").is_err());
        assert!(parse_color("#1234567").is_err());
        assert!(parse_color("#123456789").is_err());
        // Unknown name
        assert!(parse_color("chartreuse").is_err());
        assert!(parse_color("coral").is_err());
        assert!(parse_color("transparent").is_err());
    }
}
