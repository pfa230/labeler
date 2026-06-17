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

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Number,
    Money,
    Date,
    Badge,
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Cheap,
    Hydrated,
    Derived,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
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

#[derive(serde::Serialize, utoipa::ToSchema, Debug)]
#[serde(untagged)]
pub enum CellValue {
    Text(String),
    Number(f64),
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug)]
pub struct DisplayRow {
    pub id: RowRef,
    pub cells: BTreeMap<String, CellValue>,
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
    pub filters: BTreeMap<String, String>,
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

#[derive(serde::Serialize, utoipa::ToSchema)]
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
        key: &cursor::SigningKey,
        req: BrowseRequest,
    ) -> Result<BrowsePage, ConnectorError> {
        match self {
            Connectors::Homebox(c) => c.browse(conn, egress, key, req).await,
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
