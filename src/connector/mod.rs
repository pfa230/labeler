pub mod homebox;

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use crate::egress::Egress;
use crate::models::non_null;
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
    pub multi_valued: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceDescriptor {
    pub id: &'static str,
    pub columns: &'static [ColumnDef],
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Debug, PartialEq)]
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    pub ty: FieldType,
    pub tier: Tier,
    pub multi_valued: bool,
}

impl From<&ColumnDef> for FieldSpec {
    fn from(c: &ColumnDef) -> Self {
        FieldSpec {
            key: c.key.into(),
            label: c.label.into(),
            ty: c.ty,
            tier: c.tier,
            multi_valued: c.multi_valued,
        }
    }
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
    pub fields_incomplete: bool,
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
#[serde(deny_unknown_fields)]
pub struct RowRef {
    pub resource: String,
    pub key: String,
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug, PartialEq)]
#[serde(untagged)]
pub enum CellValue {
    Text(String),
    Number(f64),
    List(Vec<String>),
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
#[serde(deny_unknown_fields)]
pub struct BrowseParent {
    pub relationship: String,
    pub key: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowseRequest {
    pub resource: String,
    #[serde(default)]
    pub filters: BTreeMap<String, FilterValue>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(nullable = false)]
    pub parent: Option<BrowseParent>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(value_type = u32, minimum = 1, nullable = false)]
    pub page: Option<NonZeroU32>,
    #[serde(default, deserialize_with = "non_null")]
    #[schema(value_type = u32, minimum = 1, nullable = false)]
    pub page_size: Option<NonZeroU32>,
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug)]
pub struct BrowsePage {
    pub rows: Vec<DisplayRow>,
    pub has_more: bool,
    pub count: u64,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionPolicy {
    AsListed,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterializeRequest {
    pub rows: Vec<RowRef>,
    pub fields: Vec<String>,
    pub expansion: ExpansionPolicy,
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum RowValue {
    Text(String),
    List(Vec<String>),
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct LabelRow {
    pub source: RowRef,
    pub data: BTreeMap<String, RowValue>,
}

#[derive(Debug)]
pub enum ConnectorError {
    AuthFailed,
    ConnectionFailed(String),
    InvalidFilter(String),
    RowKeyInvalid(String),
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
            Timeout => ConnectorError::ConnectionFailed("timeout".into()),
            TooLarge => ConnectorError::Upstream("response too large".into()),
            Status(s) => ConnectorError::Upstream(format!("upstream status {s}")),
            Malformed => {
                ConnectorError::Upstream("upstream response is not the expected JSON".into())
            }
            Transport(m) => ConnectorError::ConnectionFailed(m),
        }
    }
}

/// Static-dispatch registry (one connector for now). Avoids `dyn` + async-trait; add arms for more.
pub enum Connectors {
    Homebox(homebox::HomeboxConnector),
}

impl Connectors {
    pub async fn schema(
        &self,
        conn: &Connection,
        egress: &Egress,
    ) -> Result<ConnectorSchema, ConnectorError> {
        match self {
            Connectors::Homebox(c) => c.schema(conn, egress).await,
        }
    }

    pub async fn browse(
        &self,
        conn: &Connection,
        egress: &Egress,
        req: BrowseRequest,
    ) -> Result<BrowsePage, ConnectorError> {
        match self {
            Connectors::Homebox(c) => c.browse(conn, egress, req).await,
        }
    }

    pub async fn materialize(
        &self,
        conn: &Connection,
        egress: &Egress,
        req: MaterializeRequest,
    ) -> Result<Vec<LabelRow>, ConnectorError> {
        match self {
            Connectors::Homebox(c) => c.materialize(conn, egress, req).await,
        }
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

    /// An upstream body that is not the JSON expected is a bad response, not an unreachable host.
    #[test]
    fn an_unparseable_upstream_body_is_a_bad_response() {
        let err = ConnectorError::from(crate::egress::EgressError::Malformed);
        assert!(matches!(err, ConnectorError::Upstream(_)), "{err:?}");
    }

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
}
