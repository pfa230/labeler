use std::collections::BTreeMap;

use url::Url;

use super::cursor::{self, CursorBinding, CursorClaims, SigningKey};
use super::{
    BrowsePage, BrowseRequest, CellValue, ConnectorError, ConnectorSchema, DisplayRow, FieldSpec,
    FieldType, FilterSpec, FilterType, LabelRow, MaterializeRequest, RelationshipSpec,
    ResourceSpec, RowRef, Tier, View,
};
use crate::egress::Egress;
use crate::store::Connection;

#[derive(Default)]
pub struct HomeboxConnector;

const PAGE_DEFAULT: u32 = 50;
const MATERIALIZE_CAP: usize = 200;

fn base(conn: &Connection) -> Result<Url, ConnectorError> {
    Url::parse(&conn.base_url)
        .map_err(|_| ConnectorError::ConnectionFailed("invalid base_url".into()))
}

struct EffectiveHomeboxFilters {
    q: Option<String>,
    parent: Option<String>,
    tags: Vec<String>,
}

impl EffectiveHomeboxFilters {
    fn parse(req: &BrowseRequest) -> Result<Self, ConnectorError> {
        let mut q = None;
        let mut parent = None;
        let mut tags = Vec::new();

        for (k, v) in &req.filters {
            match k.as_str() {
                "q" => q = v.as_single_trimmed("q")?,
                "parent" => parent = v.as_single_trimmed("parent")?,
                "tag" => {
                    for t in v.as_tokens() {
                        let trimmed = t.trim().to_string();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if trimmed.len() > 64 {
                            return Err(ConnectorError::InvalidFilter(
                                "tag filter exceeds max length of 64".into(),
                            ));
                        }
                        if !tags.contains(&trimmed) {
                            tags.push(trimmed);
                        }
                    }
                    if tags.len() > 16 {
                        return Err(ConnectorError::InvalidFilter(
                            "too many tags, max 16".into(),
                        ));
                    }
                }
                _ => {
                    return Err(ConnectorError::InvalidFilter(format!(
                        "unknown filter: {k}"
                    )))
                }
            }
        }

        if parent.is_some() && req.parent.is_some() {
            return Err(ConnectorError::InvalidFilter(
                "conflicting parent params".into(),
            ));
        }

        Ok(Self { q, parent, tags })
    }

    fn to_hash(&self, resource: &str, req_parent: Option<&str>) -> String {
        let parent_val = req_parent.or(self.parent.as_deref()).unwrap_or("");
        let mut m = serde_json::Map::new();
        if let Some(q) = &self.q {
            m.insert("q".into(), serde_json::Value::String(q.clone()));
        }
        if !self.tags.is_empty() {
            let mut sorted = self.tags.clone();
            sorted.sort();
            m.insert(
                "tags".into(),
                serde_json::Value::Array(
                    sorted.into_iter().map(serde_json::Value::String).collect(),
                ),
            );
        }
        let json_str = serde_json::to_string(&m).unwrap();
        crate::auth::sha256_hex(&format!("{}|{}|{}", resource, parent_val, json_str))
    }
}

impl HomeboxConnector {
    pub async fn schema(
        &self,
        conn: &Connection,
        egress: &Egress,
    ) -> Result<ConnectorSchema, ConnectorError> {
        let mut columns = vec![
            field("name", "Name", FieldType::Text, Tier::Cheap),
            field("description", "Description", FieldType::Text, Tier::Cheap),
            field("assetId", "Asset ID", FieldType::Text, Tier::Cheap),
            field("quantity", "Quantity", FieldType::Number, Tier::Cheap),
            field("purchasePrice", "Price", FieldType::Money, Tier::Cheap),
            field("location", "Location", FieldType::Text, Tier::Cheap),
            field(
                "manufacturer",
                "Manufacturer",
                FieldType::Text,
                Tier::Hydrated,
            ),
            field("modelNumber", "Model", FieldType::Text, Tier::Hydrated),
            field("serialNumber", "Serial", FieldType::Text, Tier::Hydrated),
            field("item_url", "Homebox URL", FieldType::Text, Tier::Derived),
        ];
        let b = base(conn)?;
        let custom: Vec<String> = egress
            .get_json(&b, "/api/v1/entities/fields", &[], &conn.credential)
            .await
            .unwrap_or_default();
        for name in custom {
            columns.push(field(
                &format!("custom:{name}"),
                &name,
                FieldType::Text,
                Tier::Hydrated,
            ));
        }
        Ok(ConnectorSchema {
            version: "homebox-1".into(),
            resources: vec![
                ResourceSpec {
                    id: "entities".into(),
                    label: "Items".into(),
                    view: View::Table,
                    columns,
                    filters: vec![
                        FilterSpec {
                            key: "q".into(),
                            label: "Search".into(),
                            ty: FilterType::Search,
                        },
                        FilterSpec {
                            key: "parent".into(),
                            label: "Location".into(),
                            ty: FilterType::LocationId,
                        },
                        FilterSpec {
                            key: "tag".into(),
                            label: "Tags".into(),
                            ty: FilterType::LabelId,
                        },
                    ],
                },
                ResourceSpec {
                    id: "locations".into(),
                    label: "Locations".into(),
                    view: View::Table,
                    columns: vec![
                        field("name", "Name", FieldType::Text, Tier::Cheap),
                        field("description", "Description", FieldType::Text, Tier::Cheap),
                        field("itemCount", "Items", FieldType::Number, Tier::Cheap),
                        field(
                            "location_url",
                            "Homebox URL",
                            FieldType::Text,
                            Tier::Derived,
                        ),
                    ],
                    filters: vec![],
                },
            ],
            relationships: vec![RelationshipSpec {
                id: "location_children".into(),
                label: "Contents".into(),
                from: "locations".into(),
                to: "entities".into(),
            }],
        })
    }

    pub async fn browse(
        &self,
        conn: &Connection,
        egress: &Egress,
        key: &SigningKey,
        req: BrowseRequest,
    ) -> Result<BrowsePage, ConnectorError> {
        let b = base(conn)?;
        let eff = EffectiveHomeboxFilters::parse(&req)?;
        let filter_hash = eff.to_hash(&req.resource, req.parent.as_ref().map(|p| p.key.as_str()));

        let mut page_size = req.page_size.unwrap_or(PAGE_DEFAULT).clamp(1, 200);
        let page = match &req.cursor {
            Some(tok) => {
                let claims = cursor::verify(
                    key,
                    tok,
                    &CursorBinding {
                        connector: "homebox",
                        connection: &conn.id,
                        resource: &req.resource,
                        filter_hash: &filter_hash,
                    },
                )?;
                page_size = claims.page_size;
                claims.page
            }
            None => 1,
        };

        let is_location = req.resource == "locations";
        let mut query: Vec<(String, String)> = vec![
            ("isLocation".into(), is_location.to_string()),
            ("page".into(), page.to_string()),
            ("pageSize".into(), page_size.to_string()),
        ];
        if let Some(q) = &eff.q {
            query.push(("q".into(), q.clone()));
        }
        for t in &eff.tags {
            query.push(("tags".into(), t.clone()));
        }
        if let Some(p) = req.parent.as_ref() {
            query.push(("parentIds".into(), p.key.clone()));
        } else if let Some(p) = &eff.parent {
            query.push(("parentIds".into(), p.clone()));
        }

        let resp: EntityList = egress
            .get_json(&b, "/api/v1/entities", &query, &conn.credential)
            .await
            .map_err(|e| {
                if let crate::egress::EgressError::Transport(ref m) = e {
                    if m.starts_with("json:") {
                        return ConnectorError::UpstreamSchemaMismatch(m.clone());
                    }
                }
                ConnectorError::from(e)
            })?;
        let rows: Vec<DisplayRow> = resp
            .items
            .iter()
            .map(|e| summary_to_row(e, &req.resource, &conn.base_url))
            .collect();
        let total = resp.total.unwrap_or(0);
        let has_more = (page as u64) * (page_size as u64) < total;
        let next_cursor = has_more.then(|| {
            cursor::sign(
                key,
                &CursorClaims {
                    connector: "homebox".into(),
                    connection: conn.id.clone(),
                    resource: req.resource.clone(),
                    filter_hash,
                    page: page + 1,
                    page_size,
                },
            )
        });
        Ok(BrowsePage {
            rows,
            next_cursor,
            has_more,
            count: Some(total),
        })
    }

    pub async fn materialize(
        &self,
        conn: &Connection,
        egress: &Egress,
        req: MaterializeRequest,
    ) -> Result<Vec<LabelRow>, ConnectorError> {
        if req.rows.len() > MATERIALIZE_CAP {
            return Err(ConnectorError::BudgetExceeded);
        }
        let b = base(conn)?;
        let mut out = Vec::with_capacity(req.rows.len());
        for r in &req.rows {
            // The key is interpolated into the upstream path; reject anything that could traverse
            // out of /v1/entities/{id} (URL path normalization would collapse `..` segments).
            if r.key.is_empty() || r.key.contains('/') || r.key.starts_with('.') {
                return Err(ConnectorError::InvalidFilter("invalid row key".into()));
            }
            let detail: serde_json::Value = egress
                .get_json(
                    &b,
                    &format!("/api/v1/entities/{}", r.key),
                    &[],
                    &conn.credential,
                )
                .await?;
            let mut data = BTreeMap::new();
            for f in &req.fields {
                data.insert(f.clone(), extract_field(&detail, f, &conn.base_url, &r.key));
            }
            out.push(LabelRow {
                source: r.clone(),
                data,
            });
        }
        Ok(out)
    }
}

#[derive(serde::Deserialize)]
struct EntityList {
    items: Vec<EntitySummary>,
    total: Option<u64>,
}

#[derive(serde::Deserialize)]
struct EntitySummary {
    id: String,
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "assetId")]
    asset_id: Option<String>,
    #[serde(default)]
    quantity: Option<f64>,
    #[serde(default, rename = "purchasePrice")]
    purchase_price: Option<f64>,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default, rename = "modelNumber")]
    model_number: Option<String>,
    #[serde(default, rename = "serialNumber")]
    serial_number: Option<String>,
    #[serde(default, rename = "itemCount")]
    item_count: Option<f64>,
    #[serde(default)]
    parent: Option<serde_json::Value>,
    #[serde(default)]
    fields: Option<Vec<serde_json::Value>>,
}

fn field(key: &str, label: &str, ty: FieldType, tier: Tier) -> FieldSpec {
    FieldSpec {
        key: key.into(),
        label: label.into(),
        ty,
        tier,
    }
}

fn summary_to_row(e: &EntitySummary, resource: &str, base_url: &str) -> DisplayRow {
    let mut cells = BTreeMap::new();
    cells.insert(
        "name".into(),
        CellValue::Text(e.name.clone().unwrap_or_default()),
    );
    cells.insert(
        "description".into(),
        CellValue::Text(e.description.clone().unwrap_or_default()),
    );
    let entity_url = format!("{}/entity/{}", base_url.trim_end_matches('/'), e.id);
    if resource == "locations" {
        if let Some(n) = e.item_count {
            cells.insert("itemCount".into(), CellValue::Number(n));
        }
        cells.insert("location_url".into(), CellValue::Text(entity_url.clone()));
    } else {
        cells.insert(
            "assetId".into(),
            CellValue::Text(e.asset_id.clone().unwrap_or_default()),
        );
        if let Some(q) = e.quantity {
            cells.insert("quantity".into(), CellValue::Number(q));
        }
        if let Some(p) = e.purchase_price {
            cells.insert("purchasePrice".into(), CellValue::Number(p));
        }
        cells.insert("location".into(), CellValue::Text(json_name(&e.parent)));
        if let Some(ref m) = e.manufacturer {
            cells.insert("manufacturer".into(), CellValue::Text(m.clone()));
        }
        if let Some(ref m) = e.model_number {
            cells.insert("modelNumber".into(), CellValue::Text(m.clone()));
        }
        if let Some(ref s) = e.serial_number {
            cells.insert("serialNumber".into(), CellValue::Text(s.clone()));
        }
        cells.insert("item_url".into(), CellValue::Text(entity_url.clone()));

        if let Some(ref fields) = e.fields {
            for f in fields {
                if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                    let val = f
                        .get("textValue")
                        .or_else(|| f.get("value"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    cells.insert(format!("custom:{name}"), CellValue::Text(val));
                }
            }
        }
    }
    DisplayRow {
        id: RowRef {
            resource: resource.into(),
            key: e.id.clone(),
        },
        cells,
        url: Some(entity_url),
    }
}

fn type_name(v: &Option<serde_json::Value>) -> String {
    v.as_ref()
        .and_then(|t| t.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string()
}
fn json_name(v: &Option<serde_json::Value>) -> String {
    v.as_ref()
        .and_then(|t| t.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_field(detail: &serde_json::Value, key: &str, base_url: &str, id: &str) -> String {
    match key {
        "item_url" | "location_url" => {
            format!("{}/entity/{}", base_url.trim_end_matches('/'), id)
        }
        "location" => json_name(&detail.get("parent").cloned()),
        "entityType" => type_name(&detail.get("entityType").cloned()),
        k if k.starts_with("custom:") => {
            let want = &k["custom:".len()..];
            detail
                .get("fields")
                .and_then(|f| f.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|f| f.get("name").and_then(|n| n.as_str()) == Some(want))
                        .and_then(|f| f.get("textValue").or_else(|| f.get("value")))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default()
        }
        _ => match detail.get(key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            _ => String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::FilterValue;
    use crate::store::Connection;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn effective_filters_parsing_and_hashing() {
        let mut req = BrowseRequest {
            resource: "entities".into(),
            filters: BTreeMap::new(),
            parent: None,
            cursor: None,
            page_size: None,
        };
        req.filters
            .insert("q".into(), FilterValue::Single(" search ".into()));
        req.filters.insert(
            "tag".into(),
            FilterValue::Multiple(vec!["  t2  ".into(), "t1".into(), "t1".into()]),
        );

        let eff = EffectiveHomeboxFilters::parse(&req).unwrap();
        assert_eq!(eff.q.as_deref(), Some("search"));
        assert_eq!(eff.tags, vec!["t2", "t1"]);

        let hash1 = eff.to_hash("entities", None);

        // Reverse tag order should yield same hash due to sorting
        req.filters.insert(
            "tag".into(),
            FilterValue::Multiple(vec!["t1".into(), "t2".into()]),
        );
        let eff2 = EffectiveHomeboxFilters::parse(&req).unwrap();
        let hash2 = eff2.to_hash("entities", None);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn effective_filters_rejects_long_tags() {
        let mut req = BrowseRequest {
            resource: "entities".into(),
            filters: BTreeMap::new(),
            parent: None,
            cursor: None,
            page_size: None,
        };
        let long_tag = "a".repeat(65);
        req.filters
            .insert("tag".into(), FilterValue::Single(long_tag));
        assert!(matches!(
            EffectiveHomeboxFilters::parse(&req),
            Err(ConnectorError::InvalidFilter(_))
        ));
    }

    #[test]
    fn effective_filters_rejects_conflicting_parents() {
        let mut req = BrowseRequest {
            resource: "entities".into(),
            filters: BTreeMap::new(),
            parent: Some(crate::connector::BrowseParent {
                relationship: "r".into(),
                key: "k1".into(),
            }),
            cursor: None,
            page_size: None,
        };
        req.filters
            .insert("parent".into(), FilterValue::Single("k2".into()));
        assert!(matches!(
            EffectiveHomeboxFilters::parse(&req),
            Err(ConnectorError::InvalidFilter(_))
        ));
    }

    fn conn(base: &str) -> Connection {
        Connection {
            id: "c1".into(),
            connector: "homebox".into(),
            name: "h".into(),
            base_url: base.into(),
            public_url: None,
            credential: "hb_key".into(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn browse_sends_bearer_and_maps_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(header("authorization", "Bearer hb_key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id":"e1","name":"Drill","description":"","entityType":{"name":"item"},"assetId":"000-001","quantity":1},
                    {"id":"e2","name":"Shelf","entityType":{"name":"location"}}
                ],
                "total": 2
            })))
            .mount(&server).await;
        let egress = crate::egress::Egress::with_loopback();
        let key = crate::connector::cursor::SigningKey::random();
        let c = HomeboxConnector;
        let page = c
            .browse(
                &conn(&server.uri()),
                &egress,
                &key,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    cursor: None,
                    page_size: Some(50),
                },
            )
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].id.key, "e1");
    }

    #[tokio::test]
    async fn browse_populates_all_schema_columns_including_custom_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "e1",
                        "name": "Drill",
                        "description": "Cordless power drill",
                        "assetId": "000-001",
                        "quantity": 2,
                        "purchasePrice": 99.95,
                        "manufacturer": "DeWalt",
                        "modelNumber": "DCD771C2",
                        "serialNumber": "SN12345",
                        "parent": {"id": "loc1", "name": "Garage"},
                        "fields": [
                            {"name": "Warranty", "textValue": "2028-01-01"},
                            {"name": "Voltage", "value": "20V"}
                        ]
                    }
                ],
                "total": 1
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::with_loopback();
        let key = crate::connector::cursor::SigningKey::random();
        let c = HomeboxConnector;
        let page = c
            .browse(
                &conn(&server.uri()),
                &egress,
                &key,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    cursor: None,
                    page_size: Some(50),
                },
            )
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        let cells = &page.rows[0].cells;
        assert_eq!(cells.get("name").unwrap(), &CellValue::Text("Drill".into()));
        assert_eq!(
            cells.get("description").unwrap(),
            &CellValue::Text("Cordless power drill".into())
        );
        assert_eq!(
            cells.get("assetId").unwrap(),
            &CellValue::Text("000-001".into())
        );
        assert_eq!(cells.get("quantity").unwrap(), &CellValue::Number(2.0));
        assert_eq!(
            cells.get("purchasePrice").unwrap(),
            &CellValue::Number(99.95)
        );
        assert_eq!(
            cells.get("manufacturer").unwrap(),
            &CellValue::Text("DeWalt".into())
        );
        assert_eq!(
            cells.get("modelNumber").unwrap(),
            &CellValue::Text("DCD771C2".into())
        );
        assert_eq!(
            cells.get("serialNumber").unwrap(),
            &CellValue::Text("SN12345".into())
        );
        assert_eq!(
            cells.get("location").unwrap(),
            &CellValue::Text("Garage".into())
        );
        assert_eq!(
            cells.get("item_url").unwrap(),
            &CellValue::Text(format!("{}/entity/e1", server.uri()))
        );
        assert_eq!(
            cells.get("custom:Warranty").unwrap(),
            &CellValue::Text("2028-01-01".into())
        );
        assert_eq!(
            cells.get("custom:Voltage").unwrap(),
            &CellValue::Text("20V".into())
        );
    }

    #[tokio::test]
    async fn auth_failure_maps_to_authfailed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::with_loopback();
        let key = crate::connector::cursor::SigningKey::random();
        let err = HomeboxConnector
            .browse(
                &conn(&server.uri()),
                &egress,
                &key,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    cursor: None,
                    page_size: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, crate::connector::ConnectorError::AuthFailed));
    }

    #[tokio::test]
    async fn schema_discovers_custom_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!(["Calibration Date", "Internal SKU"])),
            )
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::with_loopback();
        let s = HomeboxConnector
            .schema(&conn(&server.uri()), &egress)
            .await
            .unwrap();
        let entities = s.resources.iter().find(|r| r.id == "entities").unwrap();
        assert!(entities
            .columns
            .iter()
            .any(|f| f.label == "Calibration Date"));
    }

    #[tokio::test]
    async fn materialize_hydrates_selected_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill","manufacturer":"Acme","serialNumber":"SN9","entityType":{"name":"item"}
            })))
            .mount(&server).await;
        let egress = crate::egress::Egress::with_loopback();
        let rows = HomeboxConnector
            .materialize(
                &conn(&server.uri()),
                &egress,
                crate::connector::MaterializeRequest {
                    rows: vec![crate::connector::RowRef {
                        resource: "entities".into(),
                        key: "e1".into(),
                    }],
                    fields: vec!["name".into(), "manufacturer".into(), "item_url".into()],
                    expansion: crate::connector::ExpansionPolicy::AsListed,
                },
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].data["manufacturer"], "Acme");
        assert!(rows[0].data["item_url"].ends_with("/entity/e1"));
    }

    #[tokio::test]
    async fn materialize_rejects_traversal_key() {
        let egress = crate::egress::Egress::with_loopback();
        let err = HomeboxConnector
            .materialize(
                &conn("http://hb.lan:7745"),
                &egress,
                crate::connector::MaterializeRequest {
                    rows: vec![crate::connector::RowRef {
                        resource: "entities".into(),
                        key: "../fields".into(),
                    }],
                    fields: vec!["name".into()],
                    expansion: crate::connector::ExpansionPolicy::AsListed,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            crate::connector::ConnectorError::InvalidFilter(_)
        ));
    }

    #[tokio::test]
    async fn items_browse_sends_islocation_false() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(query_param("isLocation", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"e1","name":"Drill"}], "total": 1
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::with_loopback();
        let key = crate::connector::cursor::SigningKey::random();
        let page = HomeboxConnector
            .browse(
                &conn(&server.uri()),
                &egress,
                &key,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    cursor: None,
                    page_size: Some(50),
                },
            )
            .await
            .unwrap();
        assert_eq!(page.rows[0].id.resource, "entities");
    }

    #[tokio::test]
    async fn browse_row_has_homebox_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"e1","name":"Drill"}], "total": 1
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::with_loopback();
        let key = crate::connector::cursor::SigningKey::random();
        let c = conn(&server.uri());
        let expected = format!("{}/entity/e1", c.base_url.trim_end_matches('/'));
        let page = HomeboxConnector
            .browse(
                &c,
                &egress,
                &key,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    cursor: None,
                    page_size: Some(50),
                },
            )
            .await
            .unwrap();
        assert_eq!(page.rows[0].url.as_deref(), Some(expected.as_str()));
    }

    #[tokio::test]
    async fn locations_browse_sends_islocation_true_and_maps_cells() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(query_param("isLocation", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"l1","name":"Garage","description":"cold","itemCount": 7}], "total": 1
            })))
            .mount(&server).await;
        let egress = crate::egress::Egress::with_loopback();
        let key = crate::connector::cursor::SigningKey::random();
        let page = HomeboxConnector
            .browse(
                &conn(&server.uri()),
                &egress,
                &key,
                crate::connector::BrowseRequest {
                    resource: "locations".into(),
                    filters: Default::default(),
                    parent: None,
                    cursor: None,
                    page_size: Some(50),
                },
            )
            .await
            .unwrap();
        let row = &page.rows[0];
        assert_eq!(row.id.resource, "locations");
        assert!(matches!(row.cells.get("name"), Some(CellValue::Text(s)) if s == "Garage"));
        assert!(matches!(row.cells.get("itemCount"), Some(CellValue::Number(n)) if *n == 7.0));
    }
}
