use super::cursor::SigningKey;
use super::{
    BrowsePage, BrowseRequest, ConnectorError, ConnectorSchema, LabelRow, MaterializeRequest,
};
use crate::egress::Egress;
use crate::store::Connection;

#[derive(Default)]
pub struct HomeboxConnector;

impl HomeboxConnector {
    pub async fn schema(
        &self,
        _conn: &Connection,
        _egress: &Egress,
    ) -> Result<ConnectorSchema, ConnectorError> {
        Err(ConnectorError::Upstream("unimplemented".into()))
    }
    pub async fn browse(
        &self,
        _conn: &Connection,
        _egress: &Egress,
        _key: &SigningKey,
        _req: BrowseRequest,
    ) -> Result<BrowsePage, ConnectorError> {
        Err(ConnectorError::Upstream("unimplemented".into()))
    }
    pub async fn materialize(
        &self,
        _conn: &Connection,
        _egress: &Egress,
        _req: MaterializeRequest,
    ) -> Result<Vec<LabelRow>, ConnectorError> {
        Err(ConnectorError::Upstream("unimplemented".into()))
    }
}
