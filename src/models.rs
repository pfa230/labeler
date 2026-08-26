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
    /// Path of the YAML file relative to the templates directory (e.g. `foo.yaml` or `Shipping/pallet.yaml`).
    pub path: String,
    /// Human-readable parse error, validation error, or duplicate-id refusal.
    pub error: String,
}

#[derive(Serialize, ToSchema)]
pub struct TemplateList {
    pub templates: Vec<TemplateSummary>,
    /// Files in the templates directory that failed to parse, failed validation, or were refused
    /// because another file already holds their id.
    /// An empty list means all files loaded successfully.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub broken: Vec<BrokenTemplateSummary>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub unit: String,
    pub dpi: u32,
    pub params: BTreeMap<String, ParamSpec>,
    pub format: TemplateFormat,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub unit: String,
    pub dpi: u32,
    pub format: TemplateFormat,
    pub params: BTreeMap<String, ParamSpec>,
    pub layout: Layout,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateGroupUpdate {
    /// Nested on purpose: the outer `Option` is *presence of the key*, the inner one is its null.
    /// `{"group": null}` clears the group deliberately, `{}` is a malformed body, and a plain
    /// `Option<String>` cannot tell them apart, since serde reads a missing field of option type as
    /// `None` whether or not it carries `#[serde(default)]`. That collapse let `{}` silently
    /// ungroup a template (#164 review). Read it through [`TemplateGroupUpdate::group`].
    #[serde(default, deserialize_with = "deserialize_present_group")]
    #[schema(value_type = Option<String>)]
    group: Option<Option<String>>,
}

fn deserialize_present_group<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

impl TemplateGroupUpdate {
    /// The requested group, or `None` when the body omitted the key entirely.
    pub fn group(&self) -> Option<Option<&str>> {
        self.group.as_ref().map(|inner| inner.as_deref())
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamType {
    String {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        multiline: bool,
    },
    Length,
    Integer,
    Number,
    Boolean,
    Enum {
        values: Vec<String>,
    },
    Datetime {
        time: bool,
    },
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
#[serde(untagged)]
pub enum ParamValue {
    Integer(i64),
    Float(f32),
    Boolean(bool),
    String(String),
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub struct ParamSpec {
    #[serde(flatten)]
    pub param_type: ParamType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ParamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    T: Deserialize<'de> + std::str::FromStr,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct DynamicValueVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> serde::de::Visitor<'de> for DynamicValueVisitor<T>
        where
            T: Deserialize<'de> + std::str::FromStr,
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

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let trimmed = v.trim();
                if trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.len() >= 2 {
                    let inner = trimmed[1..trimmed.len() - 1].trim();
                    return Ok(DynamicValue::Ref(inner.to_string()));
                }
                if let Ok(val) = trimmed.parse::<T>() {
                    return Ok(DynamicValue::Literal(val));
                }
                if let Some(num_str) = trimmed
                    .strip_suffix("mm")
                    .or_else(|| trimmed.strip_suffix("in"))
                {
                    if let Ok(val) = num_str.trim().parse::<T>() {
                        return Ok(DynamicValue::Literal(val));
                    }
                }
                T::deserialize(serde::de::IntoDeserializer::into_deserializer(v))
                    .map(DynamicValue::Literal)
            }
        }

        deserializer.deserialize_any(DynamicValueVisitor(std::marker::PhantomData))
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(transparent)]
pub struct Options(pub BTreeMap<String, Vec<String>>);

impl Options {
    pub fn is_valid_selection(&self, selection: &BTreeMap<String, String>) -> bool {
        selection.iter().all(|(name, choice)| {
            self.0
                .get(name)
                .map(|values| values.iter().any(|entry| entry == choice))
                .unwrap_or(false)
        })
    }

    pub fn allowed(&self) -> &BTreeMap<String, Vec<String>> {
        &self.0
    }
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, PartialEq)]
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

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, PartialEq)]
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

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AutoSize {
    Auto,
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SizeValue {
    Auto(AutoSize),
    Dynamic(DynamicValue<f32>),
}

impl SizeValue {
    pub fn value(&self) -> Option<f32> {
        match self {
            SizeValue::Dynamic(DynamicValue::Literal(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, SizeValue::Auto(_))
    }

    pub fn fixed(val: f32) -> Self {
        SizeValue::Dynamic(DynamicValue::Literal(val))
    }

    pub fn param_ref(name: impl Into<String>) -> Self {
        SizeValue::Dynamic(DynamicValue::Ref(name.into()))
    }

    pub fn auto() -> Self {
        SizeValue::Auto(AutoSize::Auto)
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

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize, PartialEq)]
#[serde(transparent)]
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

/// How a box item's extent is expressed on the wire: `size:` (width and height) xor `to:` (the
/// opposite corner). An enum rather than two `Option`s so "exactly one" is a type invariant.
#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Extent {
    Size(Size),
    To(Position),
}

#[derive(Debug, Serialize, ToSchema, Clone)]
pub struct Placement {
    #[serde(default)]
    pub at: Position,
    #[serde(flatten)]
    pub extent: Extent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_h: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f32>,
}

impl Placement {
    /// The common case: an `at`/`size` placement with no bounds or rotation.
    pub fn sized(at: Position, size: Size) -> Self {
        Self {
            at,
            extent: Extent::Size(size),
            max_w: None,
            max_h: None,
            rotate: None,
        }
    }

    /// The `Size` when the extent is expressed that way, for the `auto`-aware paths.
    pub fn size_or_auto(&self) -> Option<&Size> {
        match &self.extent {
            Extent::Size(size) => Some(size),
            Extent::To(_) => None,
        }
    }

    /// True when the item's width cannot be known until the enclosing frame's width is.
    pub fn width_is_frame_dependent(&self) -> bool {
        match &self.extent {
            Extent::Size(size) => size.0[0].is_auto(),
            // `size = to.x - at.x`. Each edge-relative corner contributes one `frame_width` term,
            // so two of them cancel and the width is a constant. Only exactly one is
            // frame-dependent.
            Extent::To(to) => self.at.x().is_sign_negative() != to.x().is_sign_negative(),
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

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum Dimension {
    Fixed(f32),
    Dynamic { min: Option<f32>, max: Option<f32> },
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
#[serde(untagged)]
pub enum FontSize {
    Fixed(f32),
    Range { min: f32, max: f32 },
}

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct QrParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_correction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet_zone: Option<f32>,
}

#[derive(Debug, Serialize, ToSchema, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Copy, Debug, Serialize, ToSchema, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Serialize, ToSchema, Clone, Default, Deserialize)]
pub struct Alignment {
    #[serde(default)]
    pub horizontal: HorizontalAlign,
    #[serde(default)]
    pub vertical: VerticalAlign,
}

#[derive(Debug, Serialize, ToSchema, Clone, Copy, Default, Deserialize)]
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

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutItem {
    Text {
        value: String,
        #[serde(flatten)]
        placement: Placement,
        font_size: FontSize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_weight: Option<DynamicValue<u16>>,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<BTreeMap<String, String>>,
    },
    Qr {
        value: String,
        #[serde(flatten)]
        placement: Placement,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<QrParams>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<BTreeMap<String, String>>,
    },
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        src: Option<String>,
        #[serde(flatten)]
        placement: Placement,
        #[serde(default)]
        fit: Fit,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<BTreeMap<String, String>>,
    },
    Line {
        #[serde(default)]
        at: Position,
        to: Position,
        thickness: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<BTreeMap<String, String>>,
    },
    Container {
        #[serde(flatten)]
        placement: Placement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<BTreeMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        frame: Option<Frame>,
        #[serde(default)]
        padding: Padding,
        #[schema(no_recursion)]
        items: Vec<LayoutItem>,
    },
}

impl LayoutItem {
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

#[derive(Debug, Serialize, ToSchema, Clone, Copy, PartialEq, Deserialize)]
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

#[derive(Debug, Serialize, ToSchema, Clone, Deserialize)]
pub struct Frame {
    pub thickness: f32,
    #[serde(default)]
    pub rounded: bool,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(untagged)]
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
        height: DynamicDimension,
        #[serde(default)]
        media_width: Option<f32>,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenderLabelRequest {
    pub template: String,
    #[serde(flatten)]
    pub label: LabelInput,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchRequest {
    pub template: String,
    pub labels: Vec<LabelInput>,
    pub mode: String,
    #[serde(default)]
    pub printer: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub start_slot: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchRowError {
    pub index: usize,
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: Vec<BatchRowError>,
    pub jobs: usize,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LabelInput {
    pub data: HashMap<String, Value>,
}

fn default_print_copies() -> u32 {
    1
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PrintRequest {
    pub template: String,
    pub printer: String,
    #[serde(default)]
    pub data: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub fields: Option<HashMap<String, serde_json::Value>>,
    #[serde(default = "default_print_copies")]
    pub copies: u32,
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
    use super::{resolve_coord, AutoSize, Placement, Position, Size, SizeValue};

    /// GET /templates/{id} must hand back the shape the author wrote. `rename_all` is load-bearing:
    /// without it the flattened key is `Size`/`To`.
    #[test]
    fn placement_serializes_back_to_the_authored_key() {
        let sized = Placement::sized(
            Position([0.0, 0.0]),
            Size([SizeValue::from(10.0), SizeValue::from(5.0)]),
        );
        let json = serde_json::to_string(&sized).unwrap();
        assert!(json.contains("\"size\""), "got {json}");
        assert!(!json.contains("\"to\""), "got {json}");

        let cornered = Placement {
            at: Position([0.0, 0.0]),
            extent: super::Extent::To(Position([10.0, 5.0])),
            max_w: None,
            max_h: None,
            rotate: None,
        };
        let json = serde_json::to_string(&cornered).unwrap();
        assert!(json.contains("\"to\""), "got {json}");
        assert!(!json.contains("\"size\""), "got {json}");
    }

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

    fn placement(size: Size) -> Placement {
        Placement::sized(Position([0.0, 0.0]), size)
    }

    #[test]
    fn auto_width_is_frame_dependent_and_numeric_width_is_not() {
        let auto = placement(Size([
            SizeValue::Auto(AutoSize::Auto),
            SizeValue::from(8.0),
        ]));
        assert!(auto.width_is_frame_dependent());
        let fixed = placement(Size([SizeValue::from(20.0), SizeValue::from(8.0)]));
        assert!(!fixed.width_is_frame_dependent());
    }

    #[test]
    fn position_accessors_preserve_the_sign_bit() {
        let p = Position([-0.0, 5.0]);
        assert!(p.x().is_sign_negative());
        assert!(!p.y().is_sign_negative());
    }

    /// `size = to.x - at.x`, and each edge-relative corner adds one `frame_width` term, so two cancel.
    /// Only exactly one edge-relative corner makes the width frame-dependent.
    #[test]
    fn to_frame_dependence_is_the_xor_of_the_corners() {
        fn dep(at: [f32; 2], to: [f32; 2]) -> bool {
            Placement {
                at: Position(at),
                extent: super::Extent::To(Position(to)),
                max_w: None,
                max_h: None,
                rotate: None,
            }
            .width_is_frame_dependent()
        }
        assert!(
            !dep([0.0, 0.0], [30.0, 5.0]),
            "two plain corners are a constant width"
        );
        assert!(
            !dep([-20.0, 0.0], [-0.0, 5.0]),
            "two edge corners cancel: a fixed 20-unit box"
        );
        assert!(
            dep([0.0, 0.0], [-0.0, 5.0]),
            "spanning to the right edge follows the frame"
        );
        assert!(
            dep([-20.0, 0.0], [90.0, 5.0]),
            "one edge corner leaves a frame term"
        );
    }
}
