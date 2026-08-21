pub mod cursor;
pub mod homebox;

use std::collections::BTreeMap;

use crate::egress::Egress;
use crate::store::Connection;

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum View {
    Table,
    Tree,
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Number,
    Money,
    Date,
    Badge,
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Cheap,
    Hydrated,
    Derived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnDef {
    pub key: &'static str,
    pub label: &'static str,
    pub ty: FieldType,
    pub tier: Tier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceDescriptor {
    pub id: &'static str,
    pub columns: &'static [ColumnDef],
    pub dynamic_text_prefix: Option<&'static str>,
}

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Clone, Debug, PartialEq, Eq)]
pub struct FieldTransform {
    pub resource: String,
    pub source: String,
    pub pattern: String,
}

pub const COMPILED_REGEX_SIZE_LIMIT: usize = 65536;
pub const MAX_TRANSFORMS_PER_CONNECTION: usize = 32;
pub const MAX_PATTERN_BYTES: usize = 512;
pub const MAX_INPUT_SOURCE_BYTES: usize = 8192;

#[derive(Debug)]
pub struct CompiledTransform {
    pub resource: String,
    pub source: String,
    pub re: regex::Regex,
    pub capture_names: Vec<String>,
}

impl CompiledTransform {
    pub fn compile(transform: &FieldTransform) -> Result<Self, regex::Error> {
        let re = regex::RegexBuilder::new(&transform.pattern)
            .size_limit(COMPILED_REGEX_SIZE_LIMIT)
            .build()?;
        let capture_names = re
            .capture_names()
            .flatten()
            .map(ToString::to_string)
            .collect();
        Ok(Self {
            resource: transform.resource.clone(),
            source: transform.source.clone(),
            re,
            capture_names,
        })
    }

    pub fn apply<'a>(&'a self, source_val: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
        if source_val.is_empty() || source_val.len() > MAX_INPUT_SOURCE_BYTES {
            return None;
        }
        let caps = self.re.captures(source_val)?;
        let mut outputs = Vec::with_capacity(self.capture_names.len());
        for name in &self.capture_names {
            let m = caps.name(name)?;
            outputs.push((name.as_str(), m.as_str()));
        }
        Some(outputs)
    }
}

#[derive(Debug, Default)]
pub struct CompiledTransforms(pub Vec<CompiledTransform>);

impl CompiledTransforms {
    pub fn compile(transforms: &[FieldTransform]) -> Result<Self, regex::Error> {
        let mut compiled = Vec::with_capacity(transforms.len());
        for t in transforms {
            compiled.push(CompiledTransform::compile(t)?);
        }
        Ok(Self(compiled))
    }

    pub fn for_resource<'a>(
        &'a self,
        resource: &'a str,
    ) -> impl Iterator<Item = &'a CompiledTransform> {
        self.0.iter().filter(move |t| t.resource == resource)
    }

    pub fn apply_to_map(&self, resource: &str, data: &mut BTreeMap<String, String>) {
        let mut derived = Vec::new();
        for t in self.for_resource(resource) {
            if let Some(source_val) = data.get(&t.source) {
                if let Some(outputs) = t.apply(source_val) {
                    for (k, v) in outputs {
                        derived.push((k.to_string(), v.to_string()));
                    }
                }
            }
        }
        for (k, v) in derived {
            data.insert(k, v);
        }
    }

    pub fn apply_to_cells(&self, resource: &str, cells: &mut BTreeMap<String, CellValue>) {
        let mut derived = Vec::new();
        for t in self.for_resource(resource) {
            if let Some(CellValue::Text(source_val)) = cells.get(&t.source) {
                if let Some(outputs) = t.apply(source_val) {
                    for (k, v) in outputs {
                        derived.push((k.to_string(), CellValue::Text(v.to_string())));
                    }
                }
            }
        }
        for (k, v) in derived {
            cells.insert(k, v);
        }
    }
}

pub fn validate_transforms(
    descriptors: &[ResourceDescriptor],
    transforms: &[FieldTransform],
) -> Result<(), (usize, String)> {
    if transforms.len() > MAX_TRANSFORMS_PER_CONNECTION {
        return Err((
            MAX_TRANSFORMS_PER_CONNECTION,
            format!(
                "connection exceeds maximum of {} transforms",
                MAX_TRANSFORMS_PER_CONNECTION
            ),
        ));
    }

    let mut seen_derived_by_resource: std::collections::HashMap<
        &str,
        std::collections::HashSet<String>,
    > = std::collections::HashMap::new();

    for (idx, t) in transforms.iter().enumerate() {
        if t.pattern.len() > MAX_PATTERN_BYTES {
            return Err((
                idx,
                format!(
                    "transform pattern exceeds maximum length of {} bytes",
                    MAX_PATTERN_BYTES
                ),
            ));
        }

        let desc = descriptors
            .iter()
            .find(|d| d.id == t.resource)
            .ok_or_else(|| (idx, format!("unknown resource '{}'", t.resource)))?;

        let is_declared_text = desc
            .columns
            .iter()
            .any(|c| c.key == t.source && c.ty == FieldType::Text);
        let is_dynamic_text = desc
            .dynamic_text_prefix
            .is_some_and(|prefix| t.source.starts_with(prefix));

        if !is_declared_text && !is_dynamic_text {
            if desc.columns.iter().any(|c| c.key == t.source) {
                return Err((
                    idx,
                    format!("source field '{}' is not a text field", t.source),
                ));
            } else {
                return Err((idx, format!("unknown source field '{}'", t.source)));
            }
        }

        let re = match regex::RegexBuilder::new(&t.pattern)
            .size_limit(COMPILED_REGEX_SIZE_LIMIT)
            .build()
        {
            Ok(re) => re,
            Err(e) => return Err((idx, format!("invalid regex pattern: {e}"))),
        };

        let named_captures: Vec<&str> = re.capture_names().flatten().collect();
        if named_captures.is_empty() {
            return Err((
                idx,
                "pattern must declare at least one named capture group".to_string(),
            ));
        }

        let resource_seen = seen_derived_by_resource.entry(desc.id).or_default();
        let mut rule_seen = std::collections::HashSet::new();

        for name in named_captures {
            if desc.columns.iter().any(|c| c.key == name) {
                return Err((
                    idx,
                    format!(
                        "derived field '{name}' collides with an existing field on resource '{}'",
                        t.resource
                    ),
                ));
            }

            if name == "datetime" || name.starts_with("datetime.") || name.starts_with("vars.") {
                return Err((
                    idx,
                    format!("derived field '{name}' uses a reserved template namespace"),
                ));
            }

            if !rule_seen.insert(name) {
                return Err((
                    idx,
                    format!("derived field '{name}' is declared multiple times in the same rule"),
                ));
            }

            if resource_seen.contains(name) {
                return Err((
                    idx,
                    format!(
                        "derived field '{name}' is already derived by another rule on resource '{}'",
                        t.resource
                    ),
                ));
            }
            resource_seen.insert(name.to_string());
        }
    }

    Ok(())
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Debug, PartialEq)]
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    pub ty: FieldType,
    pub tier: Tier,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    Search,
    LocationId,
    LabelId,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct FilterSpec {
    pub key: String,
    pub label: String,
    pub ty: FilterType,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ResourceSpec {
    pub id: String,
    pub label: String,
    pub view: View,
    pub columns: Vec<FieldSpec>,
    pub filters: Vec<FilterSpec>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct RelationshipSpec {
    pub id: String,
    pub label: String,
    pub from: String,
    pub to: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ConnectorSchema {
    pub version: String,
    pub resources: Vec<ResourceSpec>,
    pub relationships: Vec<RelationshipSpec>,
}

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Clone, Debug)]
pub struct RowRef {
    pub resource: String,
    pub key: String,
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug, PartialEq)]
#[serde(untagged)]
pub enum CellValue {
    Text(String),
    Number(f64),
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug)]
pub struct DisplayRow {
    pub id: RowRef,
    pub cells: BTreeMap<String, CellValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum FilterValue {
    Single(String),
    Multiple(Vec<String>),
}

impl FilterValue {
    pub fn as_tokens(&self) -> Vec<String> {
        match self {
            FilterValue::Single(s) => vec![s.clone()],
            FilterValue::Multiple(v) => v.clone(),
        }
    }

    pub fn as_single_trimmed(&self, key: &str) -> Result<Option<String>, ConnectorError> {
        match self {
            FilterValue::Single(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            FilterValue::Multiple(_) => Err(ConnectorError::InvalidFilter(format!(
                "filter {} cannot have multiple values",
                key
            ))),
        }
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct BrowseParent {
    pub relationship: String,
    pub key: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct BrowseRequest {
    pub resource: String,
    #[serde(default)]
    pub filters: BTreeMap<String, FilterValue>,
    #[serde(default)]
    pub parent: Option<BrowseParent>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub page_size: Option<u32>,
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug)]
pub struct BrowsePage {
    pub rows: Vec<DisplayRow>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub count: Option<u64>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionPolicy {
    AsListed,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct MaterializeRequest {
    pub rows: Vec<RowRef>,
    pub fields: Vec<String>,
    pub expansion: ExpansionPolicy,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct LabelRow {
    pub source: RowRef,
    pub data: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum ConnectorError {
    AuthFailed,
    Forbidden,
    ConnectionFailed(String),
    InvalidFilter(String),
    UpstreamSchemaMismatch(String),
    RateLimited,
    BudgetExceeded,
    Upstream(String),
}

impl From<crate::egress::EgressError> for ConnectorError {
    fn from(e: crate::egress::EgressError) -> Self {
        use crate::egress::EgressError::*;
        match e {
            Status(401) | Status(403) => ConnectorError::AuthFailed,
            Status(429) => ConnectorError::RateLimited,
            Blocked(m) => ConnectorError::ConnectionFailed(m),
            Timeout => ConnectorError::ConnectionFailed("timeout".into()),
            TooLarge => ConnectorError::Upstream("response too large".into()),
            Status(s) => ConnectorError::Upstream(format!("upstream status {s}")),
            Transport(m) => ConnectorError::ConnectionFailed(m),
        }
    }
}

/// Static-dispatch registry (one connector for now). Avoids `dyn` + async-trait; add arms for more.
pub enum Connectors {
    Homebox(homebox::HomeboxConnector),
}

impl Connectors {
    pub fn resources(&self) -> &'static [ResourceDescriptor] {
        match self {
            Connectors::Homebox(c) => c.resources(),
        }
    }

    pub fn validate_transforms(
        &self,
        transforms: &[FieldTransform],
    ) -> Result<(), (usize, String)> {
        validate_transforms(self.resources(), transforms)
    }

    pub async fn schema(
        &self,
        conn: &Connection,
        egress: &Egress,
    ) -> Result<ConnectorSchema, ConnectorError> {
        let mut schema = match self {
            Connectors::Homebox(c) => c.schema(conn, egress).await?,
        };
        let compiled = CompiledTransforms::compile(&conn.transforms).unwrap_or_default();
        for res in &mut schema.resources {
            for t in compiled.for_resource(&res.id) {
                for name in &t.capture_names {
                    if !res.columns.iter().any(|c| &c.key == name) {
                        res.columns.push(FieldSpec {
                            key: name.clone(),
                            label: name.clone(),
                            ty: FieldType::Text,
                            tier: Tier::Derived,
                        });
                    }
                }
            }
        }
        Ok(schema)
    }

    pub async fn browse(
        &self,
        conn: &Connection,
        egress: &Egress,
        key: &cursor::SigningKey,
        req: BrowseRequest,
    ) -> Result<BrowsePage, ConnectorError> {
        let resource = req.resource.clone();
        let mut page = match self {
            Connectors::Homebox(c) => c.browse(conn, egress, key, req).await?,
        };
        let compiled = CompiledTransforms::compile(&conn.transforms).unwrap_or_default();
        for row in &mut page.rows {
            compiled.apply_to_cells(&resource, &mut row.cells);
        }
        Ok(page)
    }

    pub async fn materialize(
        &self,
        conn: &Connection,
        egress: &Egress,
        req: MaterializeRequest,
    ) -> Result<Vec<LabelRow>, ConnectorError> {
        let compiled = CompiledTransforms::compile(&conn.transforms).unwrap_or_default();
        let requested_fields = req.fields.clone();

        let mut downstream_fields = Vec::new();
        let mut needed_sources = std::collections::HashSet::new();
        let mut derived_field_names = std::collections::HashSet::new();

        for t in &compiled.0 {
            if req.rows.iter().any(|r| r.resource == t.resource) {
                for name in &t.capture_names {
                    derived_field_names.insert(name.clone());
                    if requested_fields.contains(name) {
                        needed_sources.insert(t.source.clone());
                    }
                }
            }
        }

        for f in &requested_fields {
            if !derived_field_names.contains(f) && !downstream_fields.contains(f) {
                downstream_fields.push(f.clone());
            }
        }
        for s in needed_sources {
            if !downstream_fields.contains(&s) {
                downstream_fields.push(s);
            }
        }

        let downstream_req = MaterializeRequest {
            rows: req.rows,
            fields: downstream_fields,
            expansion: req.expansion,
        };

        let mut rows = match self {
            Connectors::Homebox(c) => c.materialize(conn, egress, downstream_req).await?,
        };

        for row in &mut rows {
            compiled.apply_to_map(&row.source.resource, &mut row.data);
        }

        let requested_set: std::collections::HashSet<_> = requested_fields.into_iter().collect();
        for row in &mut rows {
            row.data.retain(|k, _| requested_set.contains(k));
        }

        Ok(rows)
    }
}

pub struct ConnectorRegistry {
    homebox: Connectors,
}
impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self {
            homebox: Connectors::Homebox(homebox::HomeboxConnector),
        }
    }
}
impl ConnectorRegistry {
    pub fn get(&self, id: &str) -> Option<&Connectors> {
        match id {
            "homebox" => Some(&self.homebox),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_value_as_tokens() {
        assert_eq!(FilterValue::Single("foo".into()).as_tokens(), vec!["foo"]);
        assert_eq!(
            FilterValue::Multiple(vec!["foo".into(), "bar".into()]).as_tokens(),
            vec!["foo", "bar"]
        );
    }

    #[test]
    fn filter_value_as_single_trimmed() {
        let single = FilterValue::Single("  foo  ".into());
        assert_eq!(single.as_single_trimmed("tag").unwrap(), Some("foo".into()));

        let single_empty = FilterValue::Single("   ".into());
        assert_eq!(single_empty.as_single_trimmed("tag").unwrap(), None);

        let multi = FilterValue::Multiple(vec!["foo".into(), "bar".into()]);
        assert!(matches!(
            multi.as_single_trimmed("tag"),
            Err(ConnectorError::InvalidFilter(_))
        ));
    }

    #[test]
    fn transform_pass_box_split() {
        let t = FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+?)\s*\|\s*(?<location_name>.*)$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t]).unwrap();
        let mut map = BTreeMap::new();
        map.insert("location".into(), "BOX.123 | Motorcycle parts".into());
        compiled.apply_to_map("entities", &mut map);

        assert_eq!(map.get("location_id").unwrap(), "BOX.123");
        assert_eq!(map.get("location_name").unwrap(), "Motorcycle parts");
        assert_eq!(map.get("location").unwrap(), "BOX.123 | Motorcycle parts");
    }

    #[test]
    fn transform_pass_no_match_leaves_keys_absent() {
        let t = FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>BOX\.\d+)\s*\|\s*(?<location_name>.*)$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t]).unwrap();
        let mut map = BTreeMap::new();
        map.insert("location".into(), "Just a simple location".into());
        compiled.apply_to_map("entities", &mut map);

        assert!(!map.contains_key("location_id"));
        assert!(!map.contains_key("location_name"));
    }

    #[test]
    fn transform_pass_unparticipating_named_group_yields_nothing() {
        let t = FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?:(?<id>BOX\.\d+)|(?<name>[A-Z]+))$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t]).unwrap();
        let mut map = BTreeMap::new();
        map.insert("location".into(), "BOX.123".into());
        compiled.apply_to_map("entities", &mut map);

        // Even though branch 1 matches, `name` does not participate, so whole rule is non-match
        assert!(!map.contains_key("id"));
        assert!(!map.contains_key("name"));
    }

    #[test]
    fn transform_pass_group_capturing_empty_string_yields_empty_value() {
        let t = FieldTransform {
            resource: "entities".into(),
            source: "tag".into(),
            pattern: r"^(?<id>\d+):(?<extra>.*)$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t]).unwrap();
        let mut map = BTreeMap::new();
        map.insert("tag".into(), "123:".into());
        compiled.apply_to_map("entities", &mut map);

        assert_eq!(map.get("id").unwrap(), "123");
        assert_eq!(map.get("extra").unwrap(), "");
    }

    #[test]
    fn transform_pass_overlong_source_value_is_non_match() {
        let t = FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+)\s*\|\s*(?<location_name>.*)$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t]).unwrap();
        let mut map = BTreeMap::new();
        let long_val = format!("BOX.123 | {}", "a".repeat(8200));
        map.insert("location".into(), long_val);
        compiled.apply_to_map("entities", &mut map);

        assert!(!map.contains_key("location_id"));
        assert!(!map.contains_key("location_name"));
    }

    #[test]
    fn transform_pass_is_flat_and_cannot_chain() {
        let t1 = FieldTransform {
            resource: "entities".into(),
            source: "raw".into(),
            pattern: r"^(?<step1>.*)$".into(),
        };
        let t2 = FieldTransform {
            resource: "entities".into(),
            source: "step1".into(),
            pattern: r"^(?<step2>.*)$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t1, t2]).unwrap();
        let mut map = BTreeMap::new();
        map.insert("raw".into(), "hello".into());
        compiled.apply_to_map("entities", &mut map);

        assert_eq!(map.get("step1").unwrap(), "hello");
        assert!(!map.contains_key("step2"));
    }

    #[test]
    fn transform_pass_applies_to_cells() {
        let t = FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+?)\s*\|\s*(?<location_name>.*)$".into(),
        };
        let compiled = CompiledTransforms::compile(&[t]).unwrap();
        let mut cells = BTreeMap::new();
        cells.insert("location".into(), CellValue::Text("BOX.123 | Shelf".into()));
        compiled.apply_to_cells("entities", &mut cells);

        assert_eq!(
            cells.get("location_id").unwrap(),
            &CellValue::Text("BOX.123".into())
        );
        assert_eq!(
            cells.get("location_name").unwrap(),
            &CellValue::Text("Shelf".into())
        );
    }

    fn sample_descriptors() -> &'static [ResourceDescriptor] {
        static COLS_E: &[ColumnDef] = &[
            ColumnDef {
                key: "name",
                label: "Name",
                ty: FieldType::Text,
                tier: Tier::Cheap,
            },
            ColumnDef {
                key: "quantity",
                label: "Qty",
                ty: FieldType::Number,
                tier: Tier::Cheap,
            },
            ColumnDef {
                key: "location",
                label: "Loc",
                ty: FieldType::Text,
                tier: Tier::Cheap,
            },
        ];
        static COLS_L: &[ColumnDef] = &[ColumnDef {
            key: "name",
            label: "Name",
            ty: FieldType::Text,
            tier: Tier::Cheap,
        }];
        static DESCS: &[ResourceDescriptor] = &[
            ResourceDescriptor {
                id: "entities",
                columns: COLS_E,
                dynamic_text_prefix: Some("custom:"),
            },
            ResourceDescriptor {
                id: "locations",
                columns: COLS_L,
                dynamic_text_prefix: None,
            },
        ];
        DESCS
    }

    #[test]
    fn validate_rejects_more_than_32_rules() {
        let descs = sample_descriptors();
        let rules: Vec<FieldTransform> = (0..33)
            .map(|i| FieldTransform {
                resource: "entities".into(),
                source: "location".into(),
                pattern: format!(r"^(?<out{i}>.*)$"),
            })
            .collect();
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 32);
        assert!(err.1.contains("maximum of 32"));
    }

    #[test]
    fn validate_rejects_overlong_pattern() {
        let descs = sample_descriptors();
        let long_pattern = format!(r"^(?<id>{})$", "a".repeat(510));
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: long_pattern,
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("exceeds maximum length of 512"));
    }

    #[test]
    fn validate_rejects_unknown_resource() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "unknown_res".into(),
            source: "location".into(),
            pattern: r"^(?<id>.*)$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("unknown resource"));
    }

    #[test]
    fn validate_rejects_non_text_source() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "quantity".into(),
            pattern: r"^(?<id>.*)$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("not a text field"));
    }

    #[test]
    fn validate_rejects_unknown_source() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "nonexistent".into(),
            pattern: r"^(?<id>.*)$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("unknown source field"));
    }

    #[test]
    fn validate_accepts_dynamic_prefix_unproven() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "custom:Internal SKU".into(),
            pattern: r"^(?<sku>.*)$".into(),
        }];
        assert!(validate_transforms(descs, &rules).is_ok());
    }

    #[test]
    fn validate_rejects_invalid_regex() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<id>[0-9+$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("invalid regex pattern"));
    }

    #[test]
    fn validate_rejects_no_named_groups() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^([0-9]+)$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err
            .1
            .contains("must declare at least one named capture group"));
    }

    #[test]
    fn validate_rejects_collision_with_existing_column() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<name>.*)$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("collides with an existing field"));
    }

    #[test]
    fn validate_rejects_reserved_namespaces() {
        let descs = sample_descriptors();
        let reserved = ["datetime", "datetime.short", "vars.site"];
        for name in reserved {
            let rules = vec![FieldTransform {
                resource: "entities".into(),
                source: "location".into(),
                pattern: format!(r"^(?<{name}>.*)$"),
            }];
            let err = validate_transforms(descs, &rules).unwrap_err();
            assert_eq!(err.0, 0);
            assert!(
                err.1.contains("reserved template namespace"),
                "name: {name}"
            );
        }
    }

    #[test]
    fn validate_rejects_duplicate_field_in_same_rule() {
        let descs = sample_descriptors();
        let rules = vec![FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            // Regex crate rejects duplicate capture group names at parse/build time
            pattern: r"^(?:(?<loc>A)|(?<loc>B))$".into(),
        }];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("duplicate") || err.1.contains("declared multiple times"));
    }

    #[test]
    fn validate_rejects_duplicate_field_across_rules_on_same_resource() {
        let descs = sample_descriptors();
        let rules = vec![
            FieldTransform {
                resource: "entities".into(),
                source: "location".into(),
                pattern: r"^(?<loc_id>.*)$".into(),
            },
            FieldTransform {
                resource: "entities".into(),
                source: "name".into(),
                pattern: r"^(?<loc_id>.*)$".into(),
            },
        ];
        let err = validate_transforms(descs, &rules).unwrap_err();
        assert_eq!(err.0, 1);
        assert!(err.1.contains("already derived by another rule"));
    }

    #[test]
    fn validate_allows_same_derived_name_across_different_resources() {
        let descs = sample_descriptors();
        let rules = vec![
            FieldTransform {
                resource: "entities".into(),
                source: "location".into(),
                pattern: r"^(?<loc_id>.*)$".into(),
            },
            FieldTransform {
                resource: "locations".into(),
                source: "name".into(),
                pattern: r"^(?<loc_id>.*)$".into(),
            },
        ];
        assert!(validate_transforms(descs, &rules).is_ok());
    }
}
