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
    /// The render options explicitly configured for this driver, if any. None means unset.
    fn configured_render_options(&self) -> Option<crate::render::ImageRenderOptions> {
        None
    }
    /// Query the printer's live capabilities via IPP Get-Printer-Attributes. Returns None on any
    /// error (network, timeout, non-success status) so callers can fall back gracefully.
    async fn capabilities(&self) -> Option<PrinterCapabilities> {
        None
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
    render: Option<crate::render::ImageRenderOptions>,
}

impl CupsDriver {
    fn from_value(config: &JsonValue) -> Result<Self, DriverError> {
        let cfg = CupsConfig::from_value(config)?;
        let render = cfg.render.as_ref().map(|r| {
            let color_mode = match r.color_mode.as_deref() {
                Some("bilevel") => crate::render::ColorMode::BiLevel,
                _ => crate::render::ColorMode::Color,
            };
            crate::render::ImageRenderOptions {
                color_mode,
                resolution_dpi: r.resolution,
            }
        });
        Ok(Self {
            uri: cfg.uri,
            username: cfg.username,
            password: cfg.password,
            ca_cert: cfg.ca_cert,
            insecure: cfg.insecure,
            render,
        })
    }

    fn build_client(
        &self,
        uri: ipp::prelude::Uri,
        timeout: Option<std::time::Duration>,
    ) -> ipp::prelude::AsyncIppClient {
        use ipp::prelude::*;
        let mut builder = AsyncIppClient::builder(uri);
        if let Some(t) = timeout {
            builder = builder.request_timeout(t);
        }
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            builder = builder.basic_auth(user, pass);
        }
        if self.insecure {
            builder = builder.ignore_tls_errors(true);
        } else if let Some(pem) = &self.ca_cert {
            builder = builder.ca_cert(pem.as_bytes());
        }
        builder.build()
    }
}

#[async_trait]
impl PrinterDriver for CupsDriver {
    fn configured_render_options(&self) -> Option<crate::render::ImageRenderOptions> {
        self.render
    }

    async fn capabilities(&self) -> Option<PrinterCapabilities> {
        use ipp::prelude::*;
        let uri: Uri = self.uri.parse().ok()?;
        let op = IppOperationBuilder::get_printer_attributes(uri.clone())
            .build()
            .ok()?;
        let resp = self
            .build_client(uri, Some(std::time::Duration::from_secs(3)))
            .send(op)
            .await
            .ok()?;
        if !resp.header().status_code().is_success() {
            return None;
        }
        Some(PrinterCapabilities::from_attributes(resp.attributes()))
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
        let response = self
            .build_client(uri, None)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterCapabilities {
    pub bilevel: bool,
    pub accepts_png: bool,
    pub resolution_dpi: Option<u32>,
}

const COLOR_RASTER_TYPES: &[&str] = &["srgb_8", "sgray_8", "cmyk_8", "adobe-rgb_8", "srgb_16"];

impl PrinterCapabilities {
    pub fn from_parts(
        color_modes: &[String],
        raster_types: &[String],
        formats: &[String],
        resolution: Option<(i32, i32, i8)>,
    ) -> Self {
        let color_mode_bilevel = color_modes.iter().any(|m| m == "bi-level");
        let raster_bilevel = raster_types.iter().any(|t| t == "black_1")
            && !raster_types
                .iter()
                .any(|t| COLOR_RASTER_TYPES.contains(&t.as_str()));
        let bilevel = color_mode_bilevel || raster_bilevel;
        let accepts_png = formats.iter().any(|f| f == "image/png");
        let resolution_dpi = resolution.and_then(|(cf, feed, units)| {
            if units == 3 && cf == feed && cf > 0 && (cf as u32) <= crate::render::MAX_RENDER_DPI {
                Some(cf as u32)
            } else {
                None
            }
        });
        Self {
            bilevel,
            accepts_png,
            resolution_dpi,
        }
    }

    fn from_attributes(attrs: &ipp::attribute::IppAttributes) -> Self {
        use ipp::model::DelimiterTag;
        let group = attrs.groups_of(DelimiterTag::PrinterAttributes).next();
        let strings = |name: &str| -> Vec<String> {
            group
                .and_then(|g| g.attributes().get(name))
                .map(|attr| {
                    attr.value()
                        .into_iter()
                        .filter_map(|v| match v {
                            ipp::value::IppValue::Keyword(s) => Some(s.as_str().to_string()),
                            ipp::value::IppValue::MimeMediaType(s) => Some(s.as_str().to_string()),
                            ipp::value::IppValue::NameWithoutLanguage(s) => {
                                Some(s.as_str().to_string())
                            }
                            other => other.as_keyword().map(|s| s.as_str().to_string()),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let resolution = group
            .and_then(|g| g.attributes().get("printer-resolution-default"))
            .and_then(|attr| match attr.value() {
                ipp::value::IppValue::Resolution {
                    cross_feed,
                    feed,
                    units,
                } => Some((*cross_feed, *feed, *units)),
                _ => None,
            });
        PrinterCapabilities::from_parts(
            &[
                strings("print-color-mode-supported"),
                strings("print-color-mode-default"),
            ]
            .concat(),
            &strings("pwg-raster-document-type-supported"),
            &strings("document-format-supported"),
            resolution,
        )
    }
}

pub fn negotiated_profile(caps: &PrinterCapabilities) -> crate::render::ImageRenderOptions {
    if caps.bilevel && caps.accepts_png {
        crate::render::ImageRenderOptions {
            color_mode: crate::render::ColorMode::BiLevel,
            resolution_dpi: caps.resolution_dpi,
        }
    } else {
        crate::render::ImageRenderOptions::default()
    }
}

/// Test-only driver that records nothing and either succeeds or fails, for exercising the `/print`
/// dispatch without a real printer.
#[cfg(test)]
struct FakeDriver {
    fail: bool,
    render: Option<crate::render::ImageRenderOptions>,
    caps: Option<PrinterCapabilities>,
}

#[cfg(test)]
impl FakeDriver {
    fn from_value(config: &JsonValue) -> Self {
        let fail = config
            .get("fail")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let render = config.get("render").map(|r| {
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
        });
        let caps = config.get("capabilities").map(|c| PrinterCapabilities {
            bilevel: c.get("bilevel").and_then(|v| v.as_bool()).unwrap_or(false),
            accepts_png: c
                .get("accepts_png")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            resolution_dpi: c
                .get("resolution")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
        });
        Self { fail, render, caps }
    }

    fn capabilities_sync(&self) -> Option<PrinterCapabilities> {
        self.caps.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl PrinterDriver for FakeDriver {
    fn configured_render_options(&self) -> Option<crate::render::ImageRenderOptions> {
        self.render
    }

    async fn capabilities(&self) -> Option<PrinterCapabilities> {
        self.capabilities_sync()
    }

    async fn send(&self, artifact: &[u8], _opts: &PrintOptions) -> Result<(), PrintError> {
        if self.fail {
            return Err(PrintError::Transport("fake failure".to_string()));
        }
        // Mirror run_batch's precedence (configured else negotiated else default); tests use Single.
        let effective = self
            .configured_render_options()
            .or_else(|| self.capabilities_sync().map(|c| negotiated_profile(&c)))
            .unwrap_or_default();
        let expected = print_artifact_format(effective.color_mode, true);
        let is_png = artifact.starts_with(b"\x89PNG");
        let want_png = matches!(expected, ArtifactFormat::Png);
        if want_png != is_png {
            return Err(PrintError::Transport(format!(
                "fake: expected {expected:?} artifact, png={is_png}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capabilities_from_parts_detects_bilevel() {
        // pwg black_1 only + png
        let c = PrinterCapabilities::from_parts(
            &[],
            &["black_1".into(), "black_8".into()],
            &["image/png".into(), "application/pdf".into()],
            Some((203, 203, 3)),
        );
        assert!(c.bilevel && c.accepts_png);
        assert_eq!(c.resolution_dpi, Some(203));
        // print-color-mode bi-level + png
        let c2 = PrinterCapabilities::from_parts(
            &["bi-level".into(), "monochrome".into()],
            &[],
            &["image/png".into()],
            None,
        );
        assert!(c2.bilevel);
        assert_eq!(c2.resolution_dpi, None);
        // black_1 alongside a COLOR raster type -> not bilevel
        let c3 = PrinterCapabilities::from_parts(
            &[],
            &["black_1".into(), "srgb_8".into()],
            &["image/png".into()],
            None,
        );
        assert!(!c3.bilevel);
        // no png -> accepts_png false
        let c4 = PrinterCapabilities::from_parts(
            &["bi-level".into()],
            &[],
            &["application/pdf".into()],
            None,
        );
        assert!(c4.bilevel && !c4.accepts_png);
    }

    #[test]
    fn resolution_conversion_rules() {
        // square dpi in range
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((300, 300, 3))).resolution_dpi,
            Some(300)
        );
        // asymmetric -> None
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((300, 600, 3))).resolution_dpi,
            None
        );
        // dpcm (units 4) -> None
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((118, 118, 4))).resolution_dpi,
            None
        );
        // out of bounds -> None
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((5000, 5000, 3))).resolution_dpi,
            None
        );
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((0, 0, 3))).resolution_dpi,
            None
        );
    }

    #[test]
    fn negotiated_profile_mapping() {
        use crate::render::ColorMode;
        let bilevel_png = PrinterCapabilities {
            bilevel: true,
            accepts_png: true,
            resolution_dpi: Some(203),
        };
        let p = negotiated_profile(&bilevel_png);
        assert!(matches!(p.color_mode, ColorMode::BiLevel));
        assert_eq!(p.resolution_dpi, Some(203));
        // bilevel but no png -> Color
        let no_png = PrinterCapabilities {
            bilevel: true,
            accepts_png: false,
            resolution_dpi: Some(203),
        };
        assert!(matches!(
            negotiated_profile(&no_png).color_mode,
            ColorMode::Color
        ));
        // color -> Color
        let color = PrinterCapabilities {
            bilevel: false,
            accepts_png: true,
            resolution_dpi: None,
        };
        assert!(matches!(
            negotiated_profile(&color).color_mode,
            ColorMode::Color
        ));
    }

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
        // no render config -> None (auto-negotiated at print time)
        assert!(driver.configured_render_options().is_none());
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
        let opts = d.configured_render_options().expect("render present");
        assert!(matches!(opts.color_mode, ColorMode::BiLevel));
        assert_eq!(opts.resolution_dpi, Some(203));
        // absent render config -> None
        let d2 = CupsDriver::from_value(&json!({ "uri": "ipp://h/q" })).unwrap();
        assert!(d2.configured_render_options().is_none());
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
        let opts = d.configured_render_options().expect("render present");
        assert!(matches!(opts.color_mode, ColorMode::BiLevel));
        // no render -> None
        let d2 = FakeDriver::from_value(&json!({ "fail": false }));
        assert!(d2.configured_render_options().is_none());
        // capabilities parsed correctly
        let d3 = FakeDriver::from_value(
            &json!({ "fail": false, "capabilities": { "bilevel": true, "accepts_png": true, "resolution": 203 } }),
        );
        let caps = d3.capabilities_sync().expect("caps present");
        assert!(caps.bilevel && caps.accepts_png);
        assert_eq!(caps.resolution_dpi, Some(203));
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
