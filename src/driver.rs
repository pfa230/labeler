//! Printer driver abstraction (ADR-0007). A configured printer's `kind` selects a driver that declares
//! the artifact format it accepts and knows how to send it. Phase 1 ships one driver (`cups`, PDF over
//! IPP); later families register here without touching the `/print` dispatch.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    Pdf,
    Png,
    Zpl,
    Raster,
}

#[derive(Debug, Clone)]
pub struct PrintOptions {
    pub copies: u32,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self { copies: 1 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("unknown printer kind '{0}'")]
    UnknownKind(String),
    #[error("invalid printer config: {0}")]
    Config(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PrintError {
    #[error("print transport error: {0}")]
    Transport(String),
}

#[async_trait]
pub trait PrinterDriver: Send + Sync {
    /// The artifact format this driver consumes; the dispatcher renders to it before `send`.
    fn accepted_format(&self) -> ArtifactFormat;
    async fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError>;
}

/// Validate that `kind` is known and `config` parses for that driver (used by printer CRUD).
pub fn validate_config(kind: &str, config: &JsonValue) -> Result<(), DriverError> {
    match kind {
        "cups" => CupsConfig::from_value(config).map(|_| ()),
        #[cfg(test)]
        "fake" => Ok(()),
        other => Err(DriverError::UnknownKind(other.to_string())),
    }
}

/// Build a driver from a stored printer's `kind` + `config` (used by `/print` dispatch).
pub fn build_driver(kind: &str, config: &JsonValue) -> Result<Box<dyn PrinterDriver>, DriverError> {
    match kind {
        "cups" => Ok(Box::new(CupsDriver::from_value(config)?)),
        #[cfg(test)]
        "fake" => Ok(Box::new(FakeDriver::from_value(config))),
        other => Err(DriverError::UnknownKind(other.to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct CupsConfig {
    uri: String,
}

impl CupsConfig {
    fn from_value(config: &JsonValue) -> Result<Self, DriverError> {
        serde_json::from_value(config.clone()).map_err(|err| DriverError::Config(err.to_string()))
    }
}

/// Sends a rendered PDF to a CUPS queue or an IPP-Everywhere printer via IPP `Print-Job`.
pub struct CupsDriver {
    uri: String,
}

impl CupsDriver {
    fn from_value(config: &JsonValue) -> Result<Self, DriverError> {
        let cfg = CupsConfig::from_value(config)?;
        Ok(Self { uri: cfg.uri })
    }
}

#[async_trait]
impl PrinterDriver for CupsDriver {
    fn accepted_format(&self) -> ArtifactFormat {
        ArtifactFormat::Pdf
    }

    async fn send(&self, artifact: &[u8], _opts: &PrintOptions) -> Result<(), PrintError> {
        use ipp::prelude::*;

        let uri: Uri = self.uri.parse().map_err(|err| {
            PrintError::Transport(format!("invalid printer uri '{}': {err}", self.uri))
        })?;
        let payload = ipp::payload::IppPayload::new(std::io::Cursor::new(artifact.to_vec()));
        let operation = IppOperationBuilder::print_job(uri.clone(), payload)
            .document_format("application/pdf")
            .job_title("labeler")
            .build()
            .map_err(|err| PrintError::Transport(err.to_string()))?;
        let response = AsyncIppClient::new(uri)
            .send(operation)
            .await
            .map_err(|err| PrintError::Transport(err.to_string()))?;
        if response.header().status_code().is_success() {
            Ok(())
        } else {
            Err(PrintError::Transport(format!(
                "printer returned IPP status {:?}",
                response.header().status_code()
            )))
        }
    }
}

/// Test-only driver that records nothing and either succeeds or fails, for exercising the `/print`
/// dispatch without a real printer.
#[cfg(test)]
struct FakeDriver {
    fail: bool,
}

#[cfg(test)]
impl FakeDriver {
    fn from_value(config: &JsonValue) -> Self {
        let fail = config
            .get("fail")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        Self { fail }
    }
}

#[cfg(test)]
#[async_trait]
impl PrinterDriver for FakeDriver {
    fn accepted_format(&self) -> ArtifactFormat {
        ArtifactFormat::Pdf
    }

    async fn send(&self, _artifact: &[u8], _opts: &PrintOptions) -> Result<(), PrintError> {
        if self.fail {
            Err(PrintError::Transport("fake failure".to_string()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_and_build_cups() {
        assert!(validate_config("cups", &json!({ "uri": "ipp://h/p" })).is_ok());
        assert!(validate_config("cups", &json!({})).is_err()); // missing uri
        assert!(validate_config("zebra", &json!({})).is_err()); // unknown kind

        let driver = build_driver("cups", &json!({ "uri": "ipp://h/p" })).unwrap();
        assert_eq!(driver.accepted_format(), ArtifactFormat::Pdf);
        assert!(build_driver("zebra", &json!({})).is_err());
    }

    #[tokio::test]
    #[ignore = "requires a real IPP/CUPS endpoint in LABELER_TEST_IPP_URI"]
    async fn cups_send_live() {
        let uri = std::env::var("LABELER_TEST_IPP_URI").expect("LABELER_TEST_IPP_URI");
        let driver = build_driver("cups", &json!({ "uri": uri })).unwrap();
        driver
            .send(b"%PDF-1.4\n%%EOF\n", &PrintOptions::default())
            .await
            .unwrap();
    }
}
