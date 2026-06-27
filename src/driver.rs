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

/// Selects the artifact format for the print path based on the driver's color mode and template shape.
/// BiLevel + single -> PNG (fits in one IPP job); everything else -> PDF.
pub fn print_artifact_format(
    color_mode: crate::render::ColorMode,
    is_single: bool,
) -> ArtifactFormat {
    if matches!(color_mode, crate::render::ColorMode::BiLevel) && is_single {
        ArtifactFormat::Png
    } else {
        ArtifactFormat::Pdf
    }
}

fn ipp_document_format(f: ArtifactFormat) -> &'static str {
    match f {
        ArtifactFormat::Pdf => "application/pdf",
        ArtifactFormat::Png => "image/png",
        ArtifactFormat::Zpl => "application/vnd.zebra-zpl",
        ArtifactFormat::Raster => "image/pwg-raster",
    }
}

#[derive(Debug, Clone)]
pub struct PrintOptions {
    pub copies: u32,
    pub artifact_format: ArtifactFormat,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            copies: 1,
            artifact_format: ArtifactFormat::Pdf,
        }
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
    /// The render options this driver prefers; the dispatcher uses them to select the render path.
    fn render_options(&self) -> crate::render::ImageRenderOptions {
        crate::render::ImageRenderOptions::default()
    }
    async fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError>;
}

/// Strip write-only secrets from a printer config before returning it to a client. cups: drop `password`.
pub fn redact_config(kind: &str, config: &JsonValue) -> JsonValue {
    let mut config = config.clone();
    if kind == "cups" {
        if let Some(obj) = config.as_object_mut() {
            obj.remove("password");
        }
    }
    config
}

/// Merge the write-only `password` from the existing stored config into `incoming` (cups). By the
/// presence of the `password` key in `incoming`: absent -> keep existing; string -> set; null -> clear.
pub fn merge_secrets(kind: &str, incoming: &mut JsonValue, existing: Option<&JsonValue>) {
    if kind != "cups" {
        return;
    }
    let Some(obj) = incoming.as_object_mut() else {
        return;
    };
    match obj.get("password") {
        None => {
            if let Some(prev) = existing
                .and_then(|e| e.get("password"))
                .filter(|v| v.is_string())
            {
                obj.insert("password".to_string(), prev.clone());
            }
        }
        Some(JsonValue::Null) => {
            obj.remove("password");
        }
        Some(_) => {}
    }
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
struct RenderProfileConfig {
    #[serde(default)]
    color_mode: Option<String>,
    #[serde(default)]
    resolution: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CupsConfig {
    uri: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    ca_cert: Option<String>,
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    render: Option<RenderProfileConfig>,
}

impl CupsConfig {
    fn from_value(config: &JsonValue) -> Result<Self, DriverError> {
        let cfg: CupsConfig = serde_json::from_value(config.clone())
            .map_err(|err| DriverError::Config(err.to_string()))?;
        if !(cfg.uri.starts_with("ipp://") || cfg.uri.starts_with("ipps://")) {
            return Err(DriverError::Config(format!(
                "cups uri must start with ipp:// or ipps:// (got '{}')",
                cfg.uri
            )));
        }
        if let Some(pem) = &cfg.ca_cert {
            if !pem.contains("-----BEGIN CERTIFICATE-----") {
                return Err(DriverError::Config(
                    "ca_cert must be a PEM certificate (expected -----BEGIN CERTIFICATE-----)"
                        .to_string(),
                ));
            }
        }
        if let Some(r) = &cfg.render {
            if let Some(cm) = &r.color_mode {
                if cm != "color" && cm != "bilevel" {
                    return Err(DriverError::Config(format!(
                        "render.color_mode must be color or bilevel (got '{cm}')"
                    )));
                }
            }
            if let Some(res) = r.resolution {
                if res == 0 || res > crate::render::MAX_RENDER_DPI {
                    return Err(DriverError::Config(format!(
                        "render.resolution must be between 1 and {}",
                        crate::render::MAX_RENDER_DPI
                    )));
                }
            }
        }
        Ok(cfg)
    }
}

/// Sends a rendered PDF to a CUPS queue or an IPP-Everywhere printer via IPP `Print-Job`.
pub struct CupsDriver {
    uri: String,
    username: Option<String>,
    password: Option<String>,
    ca_cert: Option<String>,
    insecure: bool,
    render: crate::render::ImageRenderOptions,
}

impl CupsDriver {
    fn from_value(config: &JsonValue) -> Result<Self, DriverError> {
        let cfg = CupsConfig::from_value(config)?;
        let render = cfg
            .render
            .as_ref()
            .map(|r| {
                let color_mode = match r.color_mode.as_deref() {
                    Some("bilevel") => crate::render::ColorMode::BiLevel,
                    _ => crate::render::ColorMode::Color,
                };
                crate::render::ImageRenderOptions {
                    color_mode,
                    resolution_dpi: r.resolution,
                }
            })
            .unwrap_or_default();
        Ok(Self {
            uri: cfg.uri,
            username: cfg.username,
            password: cfg.password,
            ca_cert: cfg.ca_cert,
            insecure: cfg.insecure,
            render,
        })
    }
}

#[async_trait]
impl PrinterDriver for CupsDriver {
    fn render_options(&self) -> crate::render::ImageRenderOptions {
        self.render
    }

    async fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError> {
        use ipp::prelude::*;

        let uri: Uri = self.uri.parse().map_err(|err| {
            PrintError::Transport(format!("invalid printer uri '{}': {err}", self.uri))
        })?;
        let payload = ipp::payload::IppPayload::new(std::io::Cursor::new(artifact.to_vec()));
        let operation = IppOperationBuilder::print_job(uri.clone(), payload)
            .document_format(ipp_document_format(opts.artifact_format))
            .job_title("labeler")
            .build()
            .map_err(|err| PrintError::Transport(err.to_string()))?;
        let mut builder = AsyncIppClient::builder(uri);
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            builder = builder.basic_auth(user, pass);
        }
        if self.insecure {
            builder = builder.ignore_tls_errors(true);
        } else if let Some(pem) = &self.ca_cert {
            builder = builder.ca_cert(pem.as_bytes());
        }
        let response = builder
            .build()
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
    render: crate::render::ImageRenderOptions,
}

#[cfg(test)]
impl FakeDriver {
    fn from_value(config: &JsonValue) -> Self {
        let fail = config
            .get("fail")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let render = config
            .get("render")
            .map(|r| {
                let color_mode = match r.get("color_mode").and_then(|v| v.as_str()) {
                    Some("bilevel") => crate::render::ColorMode::BiLevel,
                    _ => crate::render::ColorMode::Color,
                };
                let resolution_dpi = r
                    .get("resolution")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                crate::render::ImageRenderOptions {
                    color_mode,
                    resolution_dpi,
                }
            })
            .unwrap_or_default();
        Self { fail, render }
    }
}

#[cfg(test)]
#[async_trait]
impl PrinterDriver for FakeDriver {
    fn render_options(&self) -> crate::render::ImageRenderOptions {
        self.render
    }

    async fn send(&self, artifact: &[u8], _opts: &PrintOptions) -> Result<(), PrintError> {
        if self.fail {
            return Err(PrintError::Transport("fake failure".to_string()));
        }
        // A bilevel-configured fake printer must receive PNG bytes (proves the profile drove the render).
        if matches!(self.render.color_mode, crate::render::ColorMode::BiLevel)
            && !artifact.starts_with(b"\x89PNG")
        {
            return Err(PrintError::Transport(
                "bilevel printer expected a PNG artifact".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cups_config_parses_all_fields() {
        let cfg = CupsConfig::from_value(&json!({
            "uri": "ipps://host/printers/q",
            "username": "u",
            "password": "p",
            "ca_cert": "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----",
            "insecure": true
        }))
        .unwrap();
        assert_eq!(cfg.uri, "ipps://host/printers/q");
        assert_eq!(cfg.username.as_deref(), Some("u"));
        assert_eq!(cfg.password.as_deref(), Some("p"));
        assert!(cfg.ca_cert.is_some());
        assert!(cfg.insecure);
    }

    #[test]
    fn cups_config_minimal_defaults() {
        let cfg = CupsConfig::from_value(&json!({ "uri": "ipp://h/q" })).unwrap();
        assert!(
            cfg.username.is_none()
                && cfg.password.is_none()
                && cfg.ca_cert.is_none()
                && !cfg.insecure
        );
    }

    #[test]
    fn cups_config_rejects_non_pem_ca_cert() {
        assert!(
            CupsConfig::from_value(&json!({ "uri": "ipps://h/q", "ca_cert": "not a cert" }))
                .is_err()
        );
    }

    #[test]
    fn build_driver_accepts_full_cups_config() {
        assert!(build_driver(
            "cups",
            &json!({
                "uri": "ipp://h/q", "username": "u", "password": "p", "insecure": false
            })
        )
        .is_ok());
    }

    #[test]
    fn validate_and_build_cups() {
        assert!(validate_config("cups", &json!({ "uri": "ipp://h/p" })).is_ok());
        assert!(validate_config("cups", &json!({})).is_err()); // missing uri
        assert!(validate_config("zebra", &json!({})).is_err()); // unknown kind

        let driver = build_driver("cups", &json!({ "uri": "ipp://h/p" })).unwrap();
        assert!(matches!(
            driver.render_options().color_mode,
            crate::render::ColorMode::Color
        ));
        assert!(build_driver("zebra", &json!({})).is_err());
    }

    #[test]
    fn redact_config_omits_cups_password() {
        let c = redact_config(
            "cups",
            &json!({ "uri": "ipp://h/q", "username": "u", "password": "p" }),
        );
        assert!(c.get("password").is_none());
        assert_eq!(c["username"], "u");
    }

    #[test]
    fn merge_secrets_keep_set_clear() {
        let existing = json!({ "uri": "ipp://h/q", "password": "old" });
        // absent -> keep
        let mut a = json!({ "uri": "ipp://h/q" });
        merge_secrets("cups", &mut a, Some(&existing));
        assert_eq!(a["password"], "old");
        // present string -> set
        let mut b = json!({ "uri": "ipp://h/q", "password": "new" });
        merge_secrets("cups", &mut b, Some(&existing));
        assert_eq!(b["password"], "new");
        // null -> clear
        let mut c = json!({ "uri": "ipp://h/q", "password": null });
        merge_secrets("cups", &mut c, Some(&existing));
        assert!(c.get("password").is_none());
        // create (no existing), absent -> no password
        let mut d = json!({ "uri": "ipp://h/q" });
        merge_secrets("cups", &mut d, None);
        assert!(d.get("password").is_none());
    }

    #[test]
    fn cups_config_parses_render_profile() {
        let cfg = CupsConfig::from_value(&json!({
            "uri": "ipp://h/q",
            "render": { "color_mode": "bilevel", "resolution": 203 }
        }))
        .unwrap();
        let r = cfg.render.as_ref().unwrap();
        assert_eq!(r.color_mode.as_deref(), Some("bilevel"));
        assert_eq!(r.resolution, Some(203));
    }

    #[test]
    fn cups_config_rejects_bad_render_profile() {
        assert!(CupsConfig::from_value(
            &json!({ "uri": "ipp://h/q", "render": { "color_mode": "nope" } })
        )
        .is_err());
        assert!(CupsConfig::from_value(
            &json!({ "uri": "ipp://h/q", "render": { "resolution": 99999 } })
        )
        .is_err());
    }

    #[test]
    fn cups_driver_render_options_reflects_profile() {
        use crate::render::ColorMode;
        let d = CupsDriver::from_value(&json!({ "uri": "ipp://h/q", "render": { "color_mode": "bilevel", "resolution": 203 } })).unwrap();
        let opts = d.render_options();
        assert!(matches!(opts.color_mode, ColorMode::BiLevel));
        assert_eq!(opts.resolution_dpi, Some(203));
        // default when absent
        let d2 = CupsDriver::from_value(&json!({ "uri": "ipp://h/q" })).unwrap();
        assert!(matches!(d2.render_options().color_mode, ColorMode::Color));
    }

    #[test]
    fn print_artifact_format_rules() {
        use crate::render::ColorMode;
        assert_eq!(
            print_artifact_format(ColorMode::BiLevel, true),
            ArtifactFormat::Png
        );
        assert_eq!(
            print_artifact_format(ColorMode::BiLevel, false),
            ArtifactFormat::Pdf
        );
        assert_eq!(
            print_artifact_format(ColorMode::Color, true),
            ArtifactFormat::Pdf
        );
    }

    #[test]
    fn ipp_document_format_mapping() {
        assert_eq!(ipp_document_format(ArtifactFormat::Pdf), "application/pdf");
        assert_eq!(ipp_document_format(ArtifactFormat::Png), "image/png");
        assert_eq!(
            ipp_document_format(ArtifactFormat::Raster),
            "image/pwg-raster"
        );
        assert_eq!(
            ipp_document_format(ArtifactFormat::Zpl),
            "application/vnd.zebra-zpl"
        );
    }

    #[test]
    fn fake_driver_render_options_from_config() {
        use crate::render::ColorMode;
        let d = FakeDriver::from_value(
            &json!({ "fail": false, "render": { "color_mode": "bilevel" } }),
        );
        assert!(matches!(d.render_options().color_mode, ColorMode::BiLevel));
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
