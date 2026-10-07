use std::collections::BTreeMap;
use std::num::NonZeroU32;

use url::Url;

use super::{
    BrowsePage, BrowseRequest, CellValue, ColumnDef, ConnectorError, ConnectorSchema, DisplayRow,
    FieldSpec, FieldType, FilterSpec, FilterType, LabelRow, MaterializeRequest, RelationshipSpec,
    ResourceDescriptor, ResourceSpec, RowRef, RowValue, Tier, View,
};
use crate::egress::Egress;
use crate::store::Connection;

static ENTITIES_COLUMNS: &[ColumnDef] = &[
    ColumnDef {
        key: "name",
        label: "Name",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "description",
        label: "Description",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "assetId",
        label: "Asset ID",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "quantity",
        label: "Quantity",
        ty: FieldType::Number,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "purchasePrice",
        label: "Price",
        ty: FieldType::Money,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "tags",
        label: "Tags",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: true,
    },
    ColumnDef {
        key: "location",
        label: "Location",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "manufacturer",
        label: "Manufacturer",
        ty: FieldType::Text,
        tier: Tier::Hydrated,
        multi_valued: false,
    },
    ColumnDef {
        key: "modelNumber",
        label: "Model",
        ty: FieldType::Text,
        tier: Tier::Hydrated,
        multi_valued: false,
    },
    ColumnDef {
        key: "serialNumber",
        label: "Serial",
        ty: FieldType::Text,
        tier: Tier::Hydrated,
        multi_valued: false,
    },
    ColumnDef {
        key: "item_url",
        label: "Homebox URL",
        ty: FieldType::Text,
        tier: Tier::Derived,
        multi_valued: false,
    },
];

static LOCATIONS_COLUMNS: &[ColumnDef] = &[
    ColumnDef {
        key: "name",
        label: "Name",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "description",
        label: "Description",
        ty: FieldType::Text,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "itemCount",
        label: "Items",
        ty: FieldType::Number,
        tier: Tier::Cheap,
        multi_valued: false,
    },
    ColumnDef {
        key: "location_url",
        label: "Homebox URL",
        ty: FieldType::Text,
        tier: Tier::Derived,
        multi_valued: false,
    },
];

pub static HOMEBOX_RESOURCES: &[ResourceDescriptor] = &[
    ResourceDescriptor {
        id: "entities",
        columns: ENTITIES_COLUMNS,
    },
    ResourceDescriptor {
        id: "locations",
        columns: LOCATIONS_COLUMNS,
    },
];

#[derive(Default)]
pub struct HomeboxConnector;

const PAGE_DEFAULT: u32 = 50;
const MATERIALIZE_CAP: usize = 200;

fn base(conn: &Connection) -> Result<Url, ConnectorError> {
    Url::parse(&conn.base_url)
        .map_err(|_| ConnectorError::ConnectionFailed("invalid base_url".into()))
}

fn external_base_url(conn: &Connection) -> &str {
    conn.public_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&conn.base_url)
        .trim_end_matches('/')
}

fn build_entity_url(base: &str, id: &str) -> String {
    let trimmed_base = base.trim_end_matches('/');
    let encoded_id = urlencoding::encode(id);
    format!("{trimmed_base}/entity/{encoded_id}")
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
}

impl HomeboxConnector {
    pub fn resources(&self) -> &'static [ResourceDescriptor] {
        HOMEBOX_RESOURCES
    }

    pub async fn schema(
        &self,
        conn: &Connection,
        egress: &Egress,
    ) -> Result<ConnectorSchema, ConnectorError> {
        let b = base(conn)?;
        let custom_res: Result<Vec<String>, _> = egress
            .get_json(&b, "/api/v1/entities/fields", &[], &conn.credential)
            .await;
        let fields_incomplete = custom_res.is_err();
        let custom_columns: Vec<FieldSpec> = custom_res
            .unwrap_or_default()
            .iter()
            .map(|name| {
                field(
                    &format!("custom:{name}"),
                    name,
                    FieldType::Text,
                    Tier::Hydrated,
                )
            })
            .collect();
        // Homebox lists item and location custom fields together, so both resources get the set.
        let columns_with_custom = |desc: &ResourceDescriptor| -> Vec<FieldSpec> {
            desc.columns
                .iter()
                .map(FieldSpec::from)
                .chain(custom_columns.iter().cloned())
                .collect()
        };
        Ok(ConnectorSchema {
            version: "homebox-1".into(),
            resources: vec![
                ResourceSpec {
                    id: "entities".into(),
                    label: "Items".into(),
                    view: View::Table,
                    columns: columns_with_custom(&HOMEBOX_RESOURCES[0]),
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
                    fields_incomplete,
                },
                ResourceSpec {
                    id: "locations".into(),
                    label: "Locations".into(),
                    view: View::Table,
                    columns: columns_with_custom(&HOMEBOX_RESOURCES[1]),
                    filters: vec![],
                    fields_incomplete,
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
        req: BrowseRequest,
    ) -> Result<BrowsePage, ConnectorError> {
        let b = base(conn)?;
        let eff = EffectiveHomeboxFilters::parse(&req)?;
        let page = req.page.map_or(1, NonZeroU32::get);
        let page_size = req.page_size.map_or(PAGE_DEFAULT, NonZeroU32::get);

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
            .await?;
        let ext_base = external_base_url(conn);
        let rows: Vec<DisplayRow> = resp
            .items
            .iter()
            .map(|e| summary_to_row(e, &req.resource, ext_base))
            .collect();
        let has_more = (page as u64) * (page_size as u64) < resp.total;
        Ok(BrowsePage {
            rows,
            has_more,
            count: resp.total,
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
        let ext_base = external_base_url(conn);
        let mut out = Vec::with_capacity(req.rows.len());
        for r in &req.rows {
            // The key is interpolated into the upstream path; reject anything that could traverse
            // out of /v1/entities/{id} (URL path normalization would collapse `..` segments).
            if r.key.is_empty() || r.key.contains('/') || r.key.starts_with('.') {
                return Err(ConnectorError::RowKeyInvalid("invalid row key".into()));
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
                data.insert(
                    f.clone(),
                    extract_field(&detail, &r.resource, f, ext_base, &r.key),
                );
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
    total: u64,
}

#[derive(serde::Deserialize)]
struct TagSummary {
    name: String,
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
    #[serde(default)]
    tags: Option<Vec<TagSummary>>,
}

fn field(key: &str, label: &str, ty: FieldType, tier: Tier) -> FieldSpec {
    FieldSpec {
        key: key.into(),
        label: label.into(),
        ty,
        tier,
        multi_valued: false,
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
    let entity_url = build_entity_url(base_url, &e.id);
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
        let tag_names: Vec<String> = e
            .tags
            .as_ref()
            .map(|tags| tags.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default();
        cells.insert("tags".into(), CellValue::List(tag_names));
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
    }
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
    DisplayRow {
        id: RowRef {
            resource: resource.into(),
            key: e.id.clone(),
        },
        cells,
        url: Some(entity_url),
    }
}

fn json_name(v: &Option<serde_json::Value>) -> String {
    v.as_ref()
        .and_then(|t| t.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_field(
    detail: &serde_json::Value,
    resource: &str,
    key: &str,
    base_url: &str,
    id: &str,
) -> RowValue {
    if resource == "entities" && key == "tags" {
        let names = detail
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        t.get("name")
                            .and_then(|n| n.as_str())
                            .map(ToString::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return RowValue::List(names);
    }
    match key {
        "item_url" | "location_url" => RowValue::Text(build_entity_url(base_url, id)),
        "location" => RowValue::Text(json_name(&detail.get("parent").cloned())),
        k if k.starts_with("custom:") => {
            let want = &k["custom:".len()..];
            let val = detail
                .get("fields")
                .and_then(|f| f.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|f| f.get("name").and_then(|n| n.as_str()) == Some(want))
                })
                .and_then(|f| f.get("textValue").or_else(|| f.get("value")));
            scalar_text(val)
        }
        _ => scalar_text(detail.get(key)),
    }
}

/// A string as is, a number stringified, anything else (absent included) as `""`.
fn scalar_text(value: Option<&serde_json::Value>) -> RowValue {
    match value {
        Some(serde_json::Value::String(s)) => RowValue::Text(s.clone()),
        Some(serde_json::Value::Number(n)) => RowValue::Text(n.to_string()),
        _ => RowValue::Text(String::new()),
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
    fn effective_filters_parsing() {
        let mut req = BrowseRequest {
            resource: "entities".into(),
            filters: BTreeMap::new(),
            parent: None,
            page: None,
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
    }

    #[test]
    fn effective_filters_rejects_long_tags() {
        let mut req = BrowseRequest {
            resource: "entities".into(),
            filters: BTreeMap::new(),
            parent: None,
            page: None,
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
            page: None,
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
        let egress = crate::egress::Egress::new();
        let c = HomeboxConnector;
        let page = c
            .browse(
                &conn(&server.uri()),
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
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
        let egress = crate::egress::Egress::new();
        let c = HomeboxConnector;
        let page = c
            .browse(
                &conn(&server.uri()),
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
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
        let egress = crate::egress::Egress::new();
        let err = HomeboxConnector
            .browse(
                &conn(&server.uri()),
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
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
        let egress = crate::egress::Egress::new();
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
    async fn static_columns_match_descriptors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::new();
        let c = HomeboxConnector;
        let s = c.schema(&conn(&server.uri()), &egress).await.unwrap();
        let descriptors = c.resources();

        for desc in descriptors {
            let res = s
                .resources
                .iter()
                .find(|r| r.id == desc.id)
                .expect("resource present");
            let expected: Vec<FieldSpec> = desc.columns.iter().map(FieldSpec::from).collect();
            assert_eq!(
                res.columns, expected,
                "columns mismatch for resource {}",
                desc.id
            );
        }
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
        let egress = crate::egress::Egress::new();
        let rows = HomeboxConnector
            .materialize(
                &conn(&server.uri()),
                &egress,
                crate::connector::MaterializeRequest {
                    rows: vec![
                        crate::connector::RowRef {
                            resource: "entities".into(),
                            key: "e1".into(),
                        },
                        crate::connector::RowRef {
                            resource: "locations".into(),
                            key: "e1".into(),
                        },
                    ],
                    fields: vec![
                        "name".into(),
                        "manufacturer".into(),
                        "item_url".into(),
                        "tags".into(),
                    ],
                    expansion: crate::connector::ExpansionPolicy::AsListed,
                },
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].data["manufacturer"], RowValue::Text("Acme".into()));
        assert_eq!(
            rows[0].data["item_url"],
            RowValue::Text(format!("{}/entity/e1", server.uri()))
        );
        // On entities, tags yields RowValue::List
        assert_eq!(rows[0].data["tags"], RowValue::List(vec![]));
        // On locations (undeclared column), tags yields RowValue::Text("")
        assert_eq!(rows[1].data["tags"], RowValue::Text("".into()));
    }

    #[tokio::test]
    async fn materialize_rejects_traversal_key() {
        let egress = crate::egress::Egress::new();
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
            crate::connector::ConnectorError::RowKeyInvalid(_)
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
        let egress = crate::egress::Egress::new();
        let page = HomeboxConnector
            .browse(
                &conn(&server.uri()),
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
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
        let egress = crate::egress::Egress::new();
        let c = conn(&server.uri());
        let expected = format!("{}/entity/e1", c.base_url.trim_end_matches('/'));
        let page = HomeboxConnector
            .browse(
                &c,
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
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
                "items": [{"id":"l1","name":"Garage","description":"cold","itemCount": 7,
                           "fields": [{"name": "Code", "textValue": "BOX.123"}]}], "total": 1
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::new();
        let page = HomeboxConnector
            .browse(
                &conn(&server.uri()),
                &egress,
                crate::connector::BrowseRequest {
                    resource: "locations".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
                },
            )
            .await
            .unwrap();
        let row = &page.rows[0];
        assert_eq!(row.id.resource, "locations");
        assert!(matches!(row.cells.get("name"), Some(CellValue::Text(s)) if s == "Garage"));
        assert!(matches!(row.cells.get("itemCount"), Some(CellValue::Number(n)) if *n == 7.0));
        assert_eq!(
            row.cells.get("custom:Code"),
            Some(&CellValue::Text("BOX.123".into()))
        );
    }

    #[tokio::test]
    async fn browse_uses_public_url_for_row_urls_when_configured() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"e1","name":"Drill"}], "total": 1
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::new();
        let mut c = conn(&server.uri());
        c.public_url = Some("https://public.homebox.domain/".into());
        let page = HomeboxConnector
            .browse(
                &c,
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            page.rows[0].url.as_deref(),
            Some("https://public.homebox.domain/entity/e1")
        );
        assert_eq!(
            page.rows[0].cells.get("item_url").unwrap(),
            &CellValue::Text("https://public.homebox.domain/entity/e1".into())
        );
    }

    #[tokio::test]
    async fn browse_falls_back_to_base_url_when_public_url_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"e1","name":"Drill"}], "total": 1
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::new();
        let c = conn(&server.uri());
        let page = HomeboxConnector
            .browse(
                &c,
                &egress,
                crate::connector::BrowseRequest {
                    resource: "entities".into(),
                    filters: Default::default(),
                    parent: None,
                    page: None,
                    page_size: None,
                },
            )
            .await
            .unwrap();
        let expected = format!("{}/entity/e1", server.uri().trim_end_matches('/'));
        assert_eq!(page.rows[0].url.as_deref(), Some(expected.as_str()));
        assert_eq!(
            page.rows[0].cells.get("item_url").unwrap(),
            &CellValue::Text(expected)
        );
    }

    #[tokio::test]
    async fn materialize_uses_public_url_for_item_and_location_urls() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/loc1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"loc1","name":"Garage"
            })))
            .mount(&server)
            .await;
        let egress = crate::egress::Egress::new();
        let mut c = conn(&server.uri());
        c.public_url = Some("https://public.homebox.domain".into());

        let rows = HomeboxConnector
            .materialize(
                &c,
                &egress,
                crate::connector::MaterializeRequest {
                    rows: vec![crate::connector::RowRef {
                        resource: "entities".into(),
                        key: "e1".into(),
                    }],
                    fields: vec!["name".into(), "item_url".into()],
                    expansion: crate::connector::ExpansionPolicy::AsListed,
                },
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].data["item_url"],
            RowValue::Text("https://public.homebox.domain/entity/e1".into())
        );

        let loc_rows = HomeboxConnector
            .materialize(
                &c,
                &egress,
                crate::connector::MaterializeRequest {
                    rows: vec![crate::connector::RowRef {
                        resource: "locations".into(),
                        key: "loc1".into(),
                    }],
                    fields: vec!["name".into(), "location_url".into()],
                    expansion: crate::connector::ExpansionPolicy::AsListed,
                },
            )
            .await
            .unwrap();
        assert_eq!(loc_rows.len(), 1);
        assert_eq!(
            loc_rows[0].data["location_url"],
            RowValue::Text("https://public.homebox.domain/entity/loc1".into())
        );
    }

    #[test]
    fn build_entity_url_escapes_entity_id() {
        assert_eq!(
            build_entity_url("https://hb.example.com", "item 123"),
            "https://hb.example.com/entity/item%20123"
        );
        assert_eq!(
            build_entity_url("https://hb.example.com/", "a/b?c#d"),
            "https://hb.example.com/entity/a%2Fb%3Fc%23d"
        );
        assert_eq!(
            build_entity_url("http://localhost:7745", "simple-id_1"),
            "http://localhost:7745/entity/simple-id_1"
        );
    }

    #[test]
    fn external_base_url_fallback_and_trimming() {
        let mut c = conn("http://hb.lan:7745/");
        assert_eq!(external_base_url(&c), "http://hb.lan:7745");

        c.public_url = Some("  ".into());
        assert_eq!(external_base_url(&c), "http://hb.lan:7745");

        c.public_url = Some("https://homebox.domain.com/".into());
        assert_eq!(external_base_url(&c), "https://homebox.domain.com");
    }

    /// Custom fields are discovered once and reported on both resources, and so is a failed discovery.
    #[tokio::test]
    async fn schema_reports_custom_columns_and_discovery_failure_on_both_resources() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec!["Code"]))
            .mount(&server)
            .await;

        let egress = Egress::new();
        let connector = HomeboxConnector;

        let schema = connector
            .schema(&conn(&server.uri()), &egress)
            .await
            .unwrap();
        for resource in &schema.resources {
            assert!(!resource.fields_incomplete, "{}", resource.id);
            let custom = resource
                .columns
                .iter()
                .find(|col| col.key == "custom:Code")
                .unwrap_or_else(|| panic!("{} lacks custom:Code", resource.id));
            assert_eq!(custom.label, "Code");
            assert_eq!(custom.ty, FieldType::Text);
            assert_eq!(custom.tier, Tier::Hydrated);
        }
        let wire = serde_json::to_value(&schema).unwrap();
        for resource in wire["resources"].as_array().unwrap() {
            assert!(
                resource.get("dynamic_source_prefix").is_none(),
                "{resource}"
            );
            for column in resource["columns"].as_array().unwrap() {
                assert!(column.get("transform_source").is_none(), "{column}");
            }
        }

        let fail_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&fail_server)
            .await;
        let fail_schema = connector
            .schema(&conn(&fail_server.uri()), &egress)
            .await
            .unwrap();
        for resource in &fail_schema.resources {
            assert!(resource.fields_incomplete, "{}", resource.id);
            assert!(resource.columns.iter().any(|col| col.key == "name"));
            assert!(!resource
                .columns
                .iter()
                .any(|col| col.key.starts_with("custom:")));
        }
    }
}
