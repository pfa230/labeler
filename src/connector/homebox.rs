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
                            label: "Label".into(),
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
        let page_size = req.page_size.unwrap_or(PAGE_DEFAULT).min(200);
        let filter_hash = hash_filters(&req);
        let page = match &req.cursor {
            Some(tok) => {
                cursor::verify(
                    key,
                    tok,
                    &CursorBinding {
                        connector: "homebox",
                        connection: &conn.id,
                        resource: &req.resource,
                        filter_hash: &filter_hash,
                    },
                )?
                .page
            }
            None => 1,
        };

        let is_location = req.resource == "locations";
        let mut query: Vec<(String, String)> = vec![
            ("isLocation".into(), is_location.to_string()),
            ("page".into(), page.to_string()),
            ("pageSize".into(), page_size.to_string()),
        ];
        if let Some(q) = req.filters.get("q") {
            query.push(("q".into(), q.clone()));
        }
        if let Some(tag) = req.filters.get("tag") {
            query.push(("tags".into(), tag.clone()));
        }
        if let Some(p) = req.parent.as_ref() {
            query.push(("parentIds".into(), p.key.clone()));
        } else if let Some(p) = req.filters.get("parent") {
            query.push(("parentIds".into(), p.clone()));
        }

        let resp: EntityList = egress
            .get_json(&b, "/api/v1/entities", &query, &conn.credential)
            .await?;
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
    #[serde(default, rename = "itemCount")]
    item_count: Option<f64>,
    #[serde(default)]
    parent: Option<serde_json::Value>,
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
    if resource == "locations" {
        if let Some(n) = e.item_count {
            cells.insert("itemCount".into(), CellValue::Number(n));
        }
    } else {
        cells.insert(
            "assetId".into(),
            CellValue::Text(e.asset_id.clone().unwrap_or_default()),
        );
        if let Some(q) = e.quantity {
            cells.insert("quantity".into(), CellValue::Number(q));
        }
        cells.insert("location".into(), CellValue::Text(json_name(&e.parent)));
    }
    DisplayRow {
        id: RowRef {
            resource: resource.into(),
            key: e.id.clone(),
        },
        cells,
        url: Some(format!(
            "{}/entity/{}",
            base_url.trim_end_matches('/'),
            e.id
        )),
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

fn hash_filters(req: &BrowseRequest) -> String {
    let parent = req.parent.as_ref().map(|p| p.key.as_str()).unwrap_or("");
    let mut parts: Vec<String> = req
        .filters
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    parts.sort();
    crate::auth::sha256_hex(&format!("{}|{}|{}", req.resource, parent, parts.join("&")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Connection;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn conn(base: &str) -> Connection {
        Connection {
            id: "c1".into(),
            connector: "homebox".into(),
            name: "h".into(),
            base_url: base.into(),
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
