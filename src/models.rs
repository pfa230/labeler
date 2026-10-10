use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VariableValue {
    pub value: String,
}

#[derive(Serialize, ToSchema)]
pub struct ReloadResponse {
    pub count: usize,
    pub broken_count: usize,
}

#[derive(Serialize, ToSchema)]
pub struct BrokenTemplateSummary {
    /// The entry's name in the templates directory (e.g. `foo.yaml`).
    pub path: String,
    /// Human-readable refusal: not a template file, or a read, parse or validation error.
    pub error: String,
}

#[derive(Serialize, ToSchema)]
pub struct TemplateList {
    pub templates: Vec<TemplateSummary>,
    /// Entries in the templates directory that were refused: not a template file, or a template
    /// file that could not be read, parsed or validated.
    /// An empty list means all files loaded successfully.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub broken: Vec<BrokenTemplateSummary>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub categories: Vec<String>,
    pub unit: String,
    pub dpi: u32,
    pub params: Vec<ParamEntry>,
    pub format: TemplateFormat,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub categories: Vec<String>,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    pub params: Vec<ParamEntry>,
    pub variables: Vec<String>,
}

/// The input control a client shows for a parameter (`parameters` spec, type table).
#[derive(Debug, Serialize, ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParamControl {
    Text,
    Textarea,
    Integer,
    Number,
    Select,
    Checkbox,
    Date,
    Datetime,
    Image,
    List,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamType {
    String {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        multiline: bool,
    },
    Integer,
    Number,
    Boolean,
    Enum {
        values: Vec<String>,
    },
    Datetime {
        time: bool,
    },
    List,
}

impl ParamType {
    pub fn type_name(&self) -> &'static str {
        match self {
            ParamType::String { .. } => "string",
            ParamType::Integer => "integer",
            ParamType::Number => "number",
            ParamType::Boolean => "boolean",
            ParamType::Enum { .. } => "enum",
            ParamType::Datetime { .. } => "datetime",
            ParamType::List => "list",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
#[serde(untagged)]
pub enum ParamValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    List(Vec<String>),
    String(String),
}

impl From<&ParamValue> for Value {
    fn from(value: &ParamValue) -> Self {
        match value {
            ParamValue::Integer(i) => Value::from(*i),
            ParamValue::Float(f) => Value::from(*f),
            ParamValue::Boolean(b) => Value::from(*b),
            ParamValue::List(items) => Value::from(items.clone()),
            ParamValue::String(s) => Value::from(s.clone()),
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, PartialEq)]
pub struct ParamSpec {
    #[serde(flatten)]
    pub param_type: ParamType,
    /// A literal default holds its coerced value; a tokened default holds its declared text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ParamValue>,
    /// The instant of a literal `datetime` default, whose `default` holds its `%Y-%m-%d` form.
    #[serde(skip)]
    pub default_instant: Option<chrono::DateTime<chrono::Local>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ParamSpec {
    /// The declared text of a tokened default: a string default containing a brace.
    pub fn tokened_default(&self) -> Option<&str> {
        match &self.default {
            Some(ParamValue::String(s)) if s.contains('{') || s.contains('}') => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, PartialEq)]
pub struct ParamEntry {
    pub name: String,
    pub control: ParamControl,
    #[serde(flatten)]
    pub spec: ParamSpec,
}

#[derive(Debug, Clone, PartialEq, ToSchema)]
#[serde(untagged)]
pub enum DynamicValue<T> {
    Literal(T),
    Ref(String),
}

impl<T> DynamicValue<T> {
    pub fn literal(v: T) -> Self {
        DynamicValue::Literal(v)
    }

    pub fn param_ref(r: impl Into<String>) -> Self {
        DynamicValue::Ref(r.into())
    }

    pub fn as_literal(&self) -> Option<&T> {
        match self {
            DynamicValue::Literal(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_ref_name(&self) -> Option<&str> {
        match self {
            DynamicValue::Ref(r) => Some(r.as_str()),
            _ => None,
        }
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, DynamicValue::Ref(_))
    }
}

impl<T> From<T> for DynamicValue<T> {
    fn from(v: T) -> Self {
        DynamicValue::Literal(v)
    }
}

impl<T: Serialize> Serialize for DynamicValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            DynamicValue::Literal(v) => v.serialize(serializer),
            DynamicValue::Ref(r) => serializer.serialize_str(&format!("{{{r}}}")),
        }
    }
}

impl<'de, T> Deserialize<'de> for DynamicValue<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct DynamicValueVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> serde::de::Visitor<'de> for DynamicValueVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = DynamicValue<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a literal value or a '{param_name}' reference")
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                T::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(DynamicValue::Literal)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                T::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(DynamicValue::Literal)
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                T::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(DynamicValue::Literal)
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                T::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(DynamicValue::Literal)
            }

            /// A string is a reference only when it is exactly `{name}`; any other string is a
            /// literal only if `T` itself reads strings, so a numeric `T` refuses `"20"`.
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if let Some(name) = v.strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
                    if crate::interpolation::is_valid_ident(name) {
                        return Ok(DynamicValue::Ref(name.to_string()));
                    }
                }
                T::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(DynamicValue::Literal)
                    .map_err(|_: E| E::invalid_value(serde::de::Unexpected::Str(v), &self))
            }
        }

        deserializer.deserialize_any(DynamicValueVisitor(std::marker::PhantomData))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Default for Point {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// Resolve a template coordinate against the frame it is placed in. A **sign-negative** value is
/// measured inward from the frame's far edge: `-0.0` is the edge itself, `-2.0` is 2 units inside
/// it. The test is the sign bit and not `< 0.0`, because `-0.0 < 0.0` is false and `-0.0` is how a
/// template spells "the far edge". Total by design: a result below zero is a validation error the
/// caller raises, not something this function can decide.
pub fn resolve_coord(v: f32, frame_extent: f32) -> f32 {
    if v.is_sign_negative() {
        frame_extent + v
    } else {
        v
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Position(pub [f32; 2]);

impl Default for Position {
    fn default() -> Self {
        Self([0.0, 0.0])
    }
}

impl Position {
    pub fn point(&self) -> Point {
        Point {
            x: self.0[0],
            y: self.0[1],
        }
    }

    pub fn x(&self) -> f32 {
        self.0[0]
    }

    pub fn y(&self) -> f32 {
        self.0[1]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SizeValue {
    Content,
    Fill,
    Dynamic(DynamicValue<f32>),
}

impl SizeValue {
    pub fn fixed(val: f32) -> Self {
        SizeValue::Dynamic(DynamicValue::Literal(val))
    }

    pub fn param_ref(name: impl Into<String>) -> Self {
        SizeValue::Dynamic(DynamicValue::Ref(name.into()))
    }

    pub fn content() -> Self {
        SizeValue::Content
    }

    pub fn fill() -> Self {
        SizeValue::Fill
    }
}

impl From<f32> for SizeValue {
    fn from(value: f32) -> Self {
        SizeValue::Dynamic(DynamicValue::Literal(value))
    }
}

impl From<DynamicValue<f32>> for SizeValue {
    fn from(value: DynamicValue<f32>) -> Self {
        SizeValue::Dynamic(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Size(pub [SizeValue; 2]);

/// Orthogonal rotation interpreted from the wire `rotate` degrees (counter-clockwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// Canonicalize wire degrees to an orthogonal rotation. `None` for non-finite or
    /// non-multiple-of-90 (within `EPS`). Handles negatives and >360 via `rem_euclid`.
    pub fn from_degrees(deg: f32) -> Option<Rotation> {
        if !deg.is_finite() {
            return None;
        }
        const EPS: f32 = 1.0e-3;
        let norm = deg.rem_euclid(360.0);
        for (target, rot) in [
            (0.0, Rotation::R0),
            (90.0, Rotation::R90),
            (180.0, Rotation::R180),
            (270.0, Rotation::R270),
            (360.0, Rotation::R0),
        ] {
            if (norm - target).abs() < EPS {
                return Some(rot);
            }
        }
        None
    }

    /// 90/270 swap width and height.
    pub fn swaps_axes(self) -> bool {
        matches!(self, Rotation::R90 | Rotation::R270)
    }

    /// Anything other than `R0` triggers the rotated render/validation path.
    pub fn is_rotated(self) -> bool {
        !matches!(self, Rotation::R0)
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowDirection {
    Row,
    Column,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FlowOverflow {
    #[default]
    Fail,
    Trim,
    /// Parsing sentinel. Conversion refuses it before a `Flow` enters the domain model.
    Invalid,
}

impl<'de> Deserialize<'de> for FlowOverflow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "fail" => Self::Fail,
            "trim" => Self::Trim,
            _ => Self::Invalid,
        })
    }
}

#[cfg(test)]
mod flow_overflow_tests {
    use super::FlowOverflow;

    #[test]
    fn an_unknown_spelling_parses_to_the_invalid_sentinel() {
        assert_eq!(
            serde_yaml_ng::from_str::<FlowOverflow>("discard").unwrap(),
            FlowOverflow::Invalid
        );
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    pub direction: FlowDirection,
    #[serde(default)]
    pub gap: f32,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub line_gap: f32,
    #[serde(default)]
    pub overflow: FlowOverflow,
}

/// How a box item's extent is expressed on the wire: `size:` (width and height) xor `to:` (the
/// opposite corner). An enum rather than two `Option`s so "exactly one" is a type invariant.
#[derive(Debug, Clone, PartialEq)]
pub enum Extent {
    Size(Size),
    To(Position),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub at: Option<Position>,
    pub extent: Extent,
    pub max_w: Option<f32>,
    pub max_h: Option<f32>,
    pub rotate: Option<f32>,
}

impl Placement {
    /// The common case: an `at`/`size` placement with no bounds or rotation.
    pub fn sized(at: Position, size: Size) -> Self {
        Self {
            at: Some(at),
            extent: Extent::Size(size),
            max_w: None,
            max_h: None,
            rotate: None,
        }
    }

    /// A packed child placement: no anchor, sized with no bounds or rotation.
    pub fn packed(size: Size) -> Self {
        Self {
            at: None,
            extent: Extent::Size(size),
            max_w: None,
            max_h: None,
            rotate: None,
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct SheetPosition(pub [f32; 2]);

impl SheetPosition {
    pub fn point(&self) -> Point {
        Point {
            x: self.0[0],
            y: self.0[1],
        }
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DynamicDimension {
    Dynamic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<DynamicValue<f32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<DynamicValue<f32>>,
    },
    Fixed(DynamicValue<f32>),
}

impl From<f32> for DynamicDimension {
    fn from(val: f32) -> Self {
        DynamicDimension::Fixed(DynamicValue::Literal(val))
    }
}

impl From<Dimension> for DynamicDimension {
    fn from(dim: Dimension) -> Self {
        match dim {
            Dimension::Fixed(val) => DynamicDimension::Fixed(DynamicValue::Literal(val)),
            Dimension::Dynamic { min, max } => DynamicDimension::Dynamic {
                min: min.map(DynamicValue::Literal),
                max: max.map(DynamicValue::Literal),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum Dimension {
    Fixed(f32),
    Dynamic { min: Option<f32>, max: Option<f32> },
}

#[derive(Debug, Clone)]
pub enum FontSize {
    Fixed(f32),
    Range { min: f32, max: f32 },
}

/// A number or a `{min, max}` range. Written by hand because an untagged enum reports an unknown
/// range key only as "did not match any variant".
impl<'de> Deserialize<'de> for FontSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Range {
            min: f32,
            max: f32,
        }

        struct FontSizeVisitor;

        impl<'de> serde::de::Visitor<'de> for FontSizeVisitor {
            type Value = FontSize;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or a {min, max} range")
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(FontSize::Fixed(v as f32))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(FontSize::Fixed(v as f32))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(FontSize::Fixed(v as f32))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let Range { min, max } =
                    Range::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(FontSize::Range { min, max })
            }
        }

        deserializer.deserialize_any(FontSizeVisitor)
    }
}

/// A `qr` item's error correction level (`layout`, "The qr item"): exactly `L`, `M`, `Q` or `H`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum ErrorCorrection {
    L,
    #[default]
    M,
    Q,
    H,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Copy, Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    #[serde(default)]
    pub horizontal: HorizontalAlign,
    #[serde(default)]
    pub vertical: VerticalAlign,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

impl Fit {
    pub fn as_typst(&self) -> &'static str {
        match self {
            Fit::Contain => "contain",
            Fit::Cover => "cover",
            Fit::Stretch => "stretch",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Overflow {
    #[default]
    Ellipsis,
    Fail,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Rect,
    Ellipse,
}

/// A literal colour (`layout`, "The colour vocabulary"): one of five lowercase names or `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Color {
    rgb: [u8; 3],
}

impl Color {
    pub fn black() -> Self {
        Self { rgb: [0, 0, 0] }
    }

    #[cfg(test)]
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { rgb: [r, g, b] }
    }

    pub fn rgb(&self) -> [u8; 3] {
        self.rgb
    }

    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.rgb[0], self.rgb[1], self.rgb[2])
    }
}

impl std::str::FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rgb = match s {
            "black" => [0x00, 0x00, 0x00],
            "white" => [0xff, 0xff, 0xff],
            "red" => [0xff, 0x00, 0x00],
            "green" => [0x00, 0x80, 0x00],
            "blue" => [0x00, 0x00, 0xff],
            _ => {
                let digits = s
                    .strip_prefix('#')
                    .filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))
                    .ok_or_else(|| {
                        format!(
                            "unknown colour '{s}': expected black, white, red, green, blue or '#rrggbb'"
                        )
                    })?;
                let byte = |i: usize| {
                    u8::from_str_radix(&digits[i..i + 2], 16).expect("six checked hex digits")
                };
                [byte(0), byte(2), byte(4)]
            }
        };
        Ok(Color { rgb })
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("one of black, white, red, green, blue, or a '#rrggbb' string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse::<Color>().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(ColorVisitor)
    }
}

/// The weights `text` accepts (`text`, "Text item keys"): a multiple of 100 from 100 to 900. The
/// one rule for a literal `font_weight`, a referenced parameter's default and a supplied value.
pub fn font_weight_ok(weight: i64) -> bool {
    (100..=900).contains(&weight) && weight % 100 == 0
}

#[derive(Debug, Clone)]
pub enum LayoutItem {
    Text {
        value: String,
        placement: Placement,
        font_size: FontSize,
        font_weight: Option<DynamicValue<u16>>,
        color: Option<Color>,
        wrap: bool,
        line_spacing: Option<f32>,
        alignment: Alignment,
        overflow: Overflow,
        when: Option<BTreeMap<String, String>>,
    },
    Qr {
        value: String,
        placement: Placement,
        error_correction: ErrorCorrection,
        module_size: Option<f32>,
        quiet_zone: f32,
        when: Option<BTreeMap<String, String>>,
    },
    Image {
        src: String,
        placement: Placement,
        fit: Fit,
        when: Option<BTreeMap<String, String>>,
    },
    Line {
        at: Position,
        to: Position,
        stroke: Stroke,
        when: Option<BTreeMap<String, String>>,
    },
    Container {
        placement: Placement,
        when: Option<BTreeMap<String, String>>,
        shape: Shape,
        stroke: Option<Stroke>,
        background: Option<Color>,
        rounded: Option<f32>,
        padding: Padding,
        flow: Option<Flow>,
        repeat: Option<String>,
        items: Vec<LayoutItem>,
    },
}

impl LayoutItem {
    /// The placement an item is positioned by, or `None` for a `line`, which carries two endpoints
    /// instead of a box. Structural, not semantic: it says which shape the model uses, and nothing
    /// about what any extent means.
    pub fn placement(&self) -> Option<&Placement> {
        match self {
            LayoutItem::Text { placement, .. }
            | LayoutItem::Qr { placement, .. }
            | LayoutItem::Image { placement, .. }
            | LayoutItem::Container { placement, .. } => Some(placement),
            LayoutItem::Line { .. } => None,
        }
    }

    pub fn when(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            LayoutItem::Text { when, .. }
            | LayoutItem::Qr { when, .. }
            | LayoutItem::Image { when, .. }
            | LayoutItem::Line { when, .. }
            | LayoutItem::Container { when, .. } => when.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const ZERO: Padding = Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
}

impl Default for Padding {
    fn default() -> Self {
        Padding::ZERO
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub thickness: f32,
    #[serde(default = "Color::black")]
    pub color: Color,
}

#[derive(Debug, Clone)]
pub enum Layout {
    Items(Vec<LayoutItem>),
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateFormat {
    Sheet {
        paper_width: f32,
        paper_height: f32,
        label_width: f32,
        label_height: f32,
        positions: Vec<SheetPosition>,
    },
    Single {
        width: DynamicDimension,
        height: DynamicValue<f32>,
        #[serde(default)]
        media_width: Option<f32>,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderLabelRequest {
    pub template: String,
    pub data: HashMap<String, Value>,
}

/// `POST /render`: labels for one template to a file.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub template: String,
    pub labels: Vec<LabelInput>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub start_slot: Option<u32>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub format: Option<String>,
}

/// `POST /print`: labels for one template to a printer.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrintRequest {
    pub template: String,
    pub labels: Vec<LabelInput>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub start_slot: Option<u32>,
    pub printer: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchRowError {
    pub index: usize,
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchSummary {
    pub total: usize,
    pub sent: usize,
    pub failed: Vec<BatchRowError>,
    pub jobs: usize,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelInput {
    pub data: HashMap<String, Value>,
}

/// Deserialize a present key as `Some`. Paired with `#[serde(default)]`, an absent key stays `None`
/// while a key written as `null` fails as `json_malformed`, which the errors spec requires and
/// serde's plain `Option` would accept as absent.
pub fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// A required JSON value that neither is `null` nor holds a key written as `null` at any depth:
/// `serde_json::Value` reads `null` as a value, so without this a body of `{"value": null}` or
/// `{"value": {"short": null}}` would reach the handler instead of failing as `json_malformed`.
pub fn non_null_value<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Err(serde::de::Error::invalid_type(
            serde::de::Unexpected::Unit,
            &"a value other than null",
        ));
    }
    if holds_null_key(&value) {
        return Err(serde::de::Error::custom("a key inside the value is null"));
    }
    Ok(value)
}

/// Whether an object inside `value`, at any depth, has a key whose value is `null`. A `null` array
/// element is not a key, so only the objects inside an array are searched.
fn holds_null_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .values()
            .any(|child| child.is_null() || holds_null_key(child)),
        Value::Array(items) => items.iter().any(holds_null_key),
        _ => false,
    }
}

/// A printer's render overrides; an absent field is negotiated with the printer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderProfile {
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(nullable = false)]
    pub color_mode: Option<String>,
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(nullable = false)]
    pub resolution: Option<u32>,
}

/// How to reach a printer: the `POST /printers/probe` body, and what a driver is built from.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrinterConnection {
    pub uri: String,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub username: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub password: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub ca_cert: Option<String>,
    #[serde(default)]
    pub insecure: bool,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub render: Option<RenderProfile>,
}

// serde cannot combine `flatten` with `deny_unknown_fields`, so the two printer bodies spell the
// connection fields out and `connection()` is the one place they are copied.

/// `POST /printers` body.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPrinter {
    pub id: String,
    pub name: String,
    pub uri: String,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub username: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub password: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub ca_cert: Option<String>,
    #[serde(default)]
    pub insecure: bool,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub render: Option<RenderProfile>,
}

impl NewPrinter {
    pub fn connection(&self) -> PrinterConnection {
        PrinterConnection {
            uri: self.uri.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            ca_cert: self.ca_cert.clone(),
            insecure: self.insecure,
            render: self.render.clone(),
        }
    }
}

/// `PUT /printers/{id}` body: the whole record without `id`. An omitted `password` keeps the
/// stored one, `""` clears it.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrinterUpdate {
    pub name: String,
    pub uri: String,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub username: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub password: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub ca_cert: Option<String>,
    #[serde(default)]
    pub insecure: bool,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub render: Option<RenderProfile>,
}

impl PrinterUpdate {
    pub fn connection(&self) -> PrinterConnection {
        PrinterConnection {
            uri: self.uri.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            ca_cert: self.ca_cert.clone(),
            insecure: self.insecure,
            render: self.render.clone(),
        }
    }
}

/// A configured printer as the API returns it. There is no `password` field, so it cannot leak.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Printer {
    pub id: String,
    pub name: String,
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub ca_cert: Option<String>,
    pub insecure: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub render: Option<RenderProfile>,
}

impl Printer {
    pub fn new(id: String, name: String, conn: PrinterConnection) -> Self {
        Self {
            id,
            name,
            uri: conn.uri,
            username: conn.username,
            ca_cert: conn.ca_cert,
            insecure: conn.insecure,
            render: conn.render,
        }
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::Rotation;

    #[test]
    fn from_degrees_maps_orthogonal_and_wraps() {
        assert_eq!(Rotation::from_degrees(0.0), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(90.0), Some(Rotation::R90));
        assert_eq!(Rotation::from_degrees(180.0), Some(Rotation::R180));
        assert_eq!(Rotation::from_degrees(270.0), Some(Rotation::R270));
        assert_eq!(Rotation::from_degrees(360.0), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(-90.0), Some(Rotation::R270));
        assert_eq!(Rotation::from_degrees(-0.0), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(359.9999), Some(Rotation::R0));
        assert_eq!(Rotation::from_degrees(450.0), Some(Rotation::R90));
    }

    #[test]
    fn from_degrees_rejects_non_orthogonal_and_non_finite() {
        assert_eq!(Rotation::from_degrees(45.0), None);
        assert_eq!(Rotation::from_degrees(f32::NAN), None);
        assert_eq!(Rotation::from_degrees(f32::INFINITY), None);
        assert_eq!(Rotation::from_degrees(f32::NEG_INFINITY), None);
    }

    #[test]
    fn axis_and_rotated_predicates() {
        assert!(Rotation::R90.swaps_axes() && Rotation::R270.swaps_axes());
        assert!(!Rotation::R0.swaps_axes() && !Rotation::R180.swaps_axes());
        assert!(Rotation::R90.is_rotated() && Rotation::R180.is_rotated());
        assert!(!Rotation::R0.is_rotated());
    }
}

#[cfg(test)]
mod placement_tests {
    use super::{resolve_coord, Position};

    /// The edge sentinel is the sign bit, not `< 0.0`: `-0.0 < 0.0` is false, so a `< 0.0` test would
    /// silently read "the far edge" as "the origin". YAML `-0` and `-0.0` both arrive sign-negative.
    #[test]
    fn resolve_coord_reads_the_sign_bit() {
        assert_eq!(resolve_coord(0.0, 100.0), 0.0);
        assert_eq!(resolve_coord(20.0, 100.0), 20.0);
        assert_eq!(resolve_coord(-0.0, 100.0), 100.0);
        assert_eq!(resolve_coord(-2.0, 100.0), 98.0);
        // Rejecting an inset larger than the frame is the caller's job; the helper stays total.
        assert_eq!(resolve_coord(-120.0, 100.0), -20.0);
    }

    #[test]
    fn position_accessors_preserve_the_sign_bit() {
        let p = Position([-0.0, 5.0]);
        assert!(p.x().is_sign_negative());
        assert!(!p.y().is_sign_negative());
    }
}

#[cfg(test)]
mod color_tests {
    use super::Color;

    #[test]
    fn the_five_names_resolve_to_stated_values() {
        for (name, rgb, hex) in [
            ("black", [0x00, 0x00, 0x00], "#000000"),
            ("white", [0xff, 0xff, 0xff], "#ffffff"),
            ("red", [0xff, 0x00, 0x00], "#ff0000"),
            ("green", [0x00, 0x80, 0x00], "#008000"),
            ("blue", [0x00, 0x00, 0xff], "#0000ff"),
        ] {
            let color: Color = name.parse().unwrap();
            assert_eq!(color.rgb(), rgb, "failed for name '{name}'");
            assert_eq!(color.hex(), hex, "failed hex for '{name}'");
        }
    }

    #[test]
    fn invalid_colour_strings_are_rejected() {
        let invalid = [
            "chartreuse",
            "eastern",
            "orange",
            "ff00ff",
            "#ff00f",
            "#gg0000",
            "",
            "   ",
            "re d",
            "# f0f",
            "#1234567",
            "#ff",
            "#f",
        ];
        for s in invalid {
            assert!(s.parse::<Color>().is_err(), "expected '{s}' to be rejected");
        }
    }

    #[test]
    fn a_colour_is_one_of_five_names_or_six_digit_hex() {
        let mut misses: Vec<String> = ["RED", " red ", "#f0f", "#ff00ff80", "navy"]
            .into_iter()
            .filter(|s| s.parse::<Color>().is_ok())
            .map(|s| format!("'{s}' parses"))
            .collect();
        let mixed_case: Color = "#FF00ff".parse().unwrap();
        if mixed_case.hex() != "#ff00ff" {
            misses.push(format!("'#FF00ff' is {}", mixed_case.hex()));
        }
        assert!(misses.is_empty(), "{}", misses.join("\n"));
    }

    #[test]
    fn non_string_is_rejected_in_deserialization() {
        assert!(serde_json::from_str::<Color>("16711680").is_err());
        assert!(serde_json::from_str::<Color>("true").is_err());
        assert!(serde_json::from_str::<Color>("[255, 0, 0]").is_err());
        assert!(serde_json::from_str::<Color>("{\"r\": 255}").is_err());
    }
}
