use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

use super::ConnectorError;

type HmacSha256 = Hmac<Sha256>;

/// Process-lifetime signing key for browse cursors (regenerated each start).
#[derive(Clone)]
pub struct SigningKey([u8; 32]);
impl SigningKey {
    pub fn random() -> Self {
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        Self(k)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CursorClaims {
    pub connector: String,
    pub connection: String,
    pub resource: String,
    pub filter_hash: String,
    pub page: u32,
    pub page_size: u32,
}

pub struct CursorBinding<'a> {
    pub connector: &'a str,
    pub connection: &'a str,
    pub resource: &'a str,
    pub filter_hash: &'a str,
}

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn sign(key: &SigningKey, claims: &CursorClaims) -> String {
    let body = serde_json::to_vec(claims).expect("serialize cursor");
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("hmac key");
    mac.update(&body);
    let tag = mac.finalize().into_bytes();
    format!("{}.{}", B64.encode(&body), B64.encode(tag))
}

pub fn verify(
    key: &SigningKey,
    token: &str,
    bind: &CursorBinding,
) -> Result<CursorClaims, ConnectorError> {
    let (b, t) = token
        .split_once('.')
        .ok_or_else(|| ConnectorError::InvalidFilter("bad cursor".into()))?;
    let body = B64
        .decode(b)
        .map_err(|_| ConnectorError::InvalidFilter("bad cursor".into()))?;
    let tag = B64
        .decode(t)
        .map_err(|_| ConnectorError::InvalidFilter("bad cursor".into()))?;
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("hmac key");
    mac.update(&body);
    mac.verify_slice(&tag)
        .map_err(|_| ConnectorError::InvalidFilter("cursor signature".into()))?;
    let claims: CursorClaims = serde_json::from_slice(&body)
        .map_err(|_| ConnectorError::InvalidFilter("bad cursor".into()))?;
    if claims.connector != bind.connector
        || claims.connection != bind.connection
        || claims.resource != bind.resource
        || claims.filter_hash != bind.filter_hash
    {
        return Err(ConnectorError::InvalidFilter(
            "cursor does not match request".into(),
        ));
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_round_trips_and_rejects_mismatch() {
        let key = SigningKey::random();
        let claims = CursorClaims {
            connector: "homebox".into(),
            connection: "c1".into(),
            resource: "entities".into(),
            filter_hash: "h".into(),
            page: 2,
            page_size: 50,
        };
        let token = sign(&key, &claims);
        let bind = CursorBinding {
            connector: "homebox",
            connection: "c1",
            resource: "entities",
            filter_hash: "h",
        };
        let back = verify(&key, &token, &bind).unwrap();
        assert_eq!(back.page, 2);
        // wrong connection -> rejected
        let bad = CursorBinding {
            connector: "homebox",
            connection: "OTHER",
            resource: "entities",
            filter_hash: "h",
        };
        assert!(verify(&key, &token, &bad).is_err());
        // tampered token -> rejected
        assert!(verify(&key, &(token + "x"), &bind).is_err());
    }
}
