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

/// Outcome of probing a printer for its live capabilities. Auth handling is out of scope (#118): a
/// printer that rejects the unauthenticated query is reported as `Unreachable` with its status/error
/// text in the detail string, not as a distinct auth outcome.
#[derive(Debug)]
pub enum ProbeOutcome {
    Ok(PrinterCapabilities),
    Unreachable(String),
}

#[async_trait]
pub trait PrinterDriver: Send + Sync {
    /// The per-field render overrides explicitly configured for this driver. Each `None` field means
    /// "not overridden, negotiate it". See [`effective_render`].
    fn configured_render_override(&self) -> RenderOverride {
        RenderOverride::default()
    }
    /// Probe the printer for live capabilities via IPP Get-Printer-Attributes, distinguishing a
    /// reachable printer that answered (`Ok`) from one we could not usefully reach (`Unreachable`).
    async fn probe(&self) -> ProbeOutcome;
    /// Live capabilities, or None on any probe failure, so callers can fall back gracefully.
    async fn capabilities(&self) -> Option<PrinterCapabilities> {
        match self.probe().await {
            ProbeOutcome::Ok(c) => Some(c),
            ProbeOutcome::Unreachable(_) => None,
        }
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

/// Screen an IPP dial target through the same outbound IP policy the connector uses
/// (`crate::egress::ip_allowed`): reject loopback, link-local/metadata, unspecified, multicast. Applied
/// before any IPP request so a user-supplied printer URI cannot be turned into an SSRF probe of internal
/// services. TOCTOU caveat: the address resolved here and the one the client later dials can differ; this
/// matches the `egress.rs` posture and is acceptable for a self-hosted tool.
pub async fn screen_ipp_uri(uri: &str) -> Result<(), DriverError> {
    let parsed = url::Url::parse(uri)
        .map_err(|e| DriverError::Config(format!("invalid uri '{uri}': {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| DriverError::Config(format!("uri has no host: '{uri}'")))?;
    // IPP default port is 631; only used for DNS resolution below, not for the screen itself.
    let port = parsed.port().unwrap_or(631);
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if !crate::egress::ip_allowed(ip, false) {
            return Err(DriverError::Config(format!(
                "blocked printer address: {ip}"
            )));
        }
        return Ok(());
    }
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| DriverError::Config(format!("cannot resolve printer host '{host}': {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(DriverError::Config(format!(
            "printer host '{host}' resolved to no addresses"
        )));
    }
    for addr in addrs {
        if !crate::egress::ip_allowed(addr.ip(), false) {
            return Err(DriverError::Config(format!(
                "blocked printer address for '{host}': {}",
                addr.ip()
            )));
        }
    }
    Ok(())
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
    render: RenderOverride,
}

impl CupsDriver {
    fn from_value(config: &JsonValue) -> Result<Self, DriverError> {
        let cfg = CupsConfig::from_value(config)?;
        // Per-field: a missing key stays `None` (negotiate it), not a concrete default.
        let render = cfg
            .render
            .as_ref()
            .map(|r| RenderOverride {
                color_mode: match r.color_mode.as_deref() {
                    Some("bilevel") => Some(crate::render::ColorMode::BiLevel),
                    Some("color") => Some(crate::render::ColorMode::Color),
                    _ => None,
                },
                resolution_dpi: r.resolution,
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
    fn configured_render_override(&self) -> RenderOverride {
        self.render
    }

    async fn probe(&self) -> ProbeOutcome {
        use ipp::prelude::*;
        if let Err(e) = screen_ipp_uri(&self.uri).await {
            return ProbeOutcome::Unreachable(e.to_string());
        }
        let uri: Uri = match self.uri.parse() {
            Ok(u) => u,
            Err(e) => {
                return ProbeOutcome::Unreachable(format!(
                    "invalid printer uri '{}': {e}",
                    self.uri
                ))
            }
        };
        let op = match IppOperationBuilder::get_printer_attributes(uri.clone()).build() {
            Ok(o) => o,
            Err(e) => return ProbeOutcome::Unreachable(e.to_string()),
        };
        let resp = match self
            .build_client(uri, Some(std::time::Duration::from_secs(3)))
            .send(op)
            .await
        {
            Ok(r) => r,
            Err(e) => return ProbeOutcome::Unreachable(e.to_string()),
        };
        if !resp.header().status_code().is_success() {
            return ProbeOutcome::Unreachable(format!(
                "printer returned IPP status {:?}",
                resp.header().status_code()
            ));
        }
        ProbeOutcome::Ok(PrinterCapabilities::from_attributes(resp.attributes()))
    }

    async fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError> {
        use ipp::prelude::*;

        screen_ipp_uri(&self.uri)
            .await
            .map_err(|err| PrintError::Transport(err.to_string()))?;
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

/// Convert an IPP `media-size` `x-dimension` (hundredths of mm, Integer) to millimetres.
/// Returns None for missing or non-positive values (zero means unknown/unset in IPP).
pub fn loaded_media_width_mm(x_hundredths: Option<i32>) -> Option<f32> {
    x_hundredths.filter(|x| *x > 0).map(|x| x as f32 / 100.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrinterCapabilities {
    pub bilevel: bool,
    /// Whether the printer advertised any color-mode or raster-type attribute at all. Lets callers
    /// tell "known color-capable" (`color_known && !bilevel`) apart from "said nothing" (`!color_known`).
    pub color_known: bool,
    pub accepts_png: bool,
    pub resolution_dpi: Option<u32>,
    pub loaded_media_width_mm: Option<f32>,
    /// `printer-make-and-model`, when reported.
    pub model: Option<String>,
}

const COLOR_RASTER_TYPES: &[&str] = &["srgb_8", "sgray_8", "cmyk_8", "adobe-rgb_8", "srgb_16"];

/// Return the first `Collection` reachable from `v`. Handles both a bare `Collection` and an
/// `Array` of collections (IPP 1setOf), since iterating a `Collection` yields members, not itself.
fn first_collection(
    v: &ipp::value::IppValue,
) -> Option<&std::collections::BTreeMap<ipp::value::IppName, ipp::value::IppValue>> {
    match v {
        ipp::value::IppValue::Collection(c) => Some(c),
        ipp::value::IppValue::Array(items) => items.iter().find_map(|i| match i {
            ipp::value::IppValue::Collection(c) => Some(c),
            _ => None,
        }),
        _ => None,
    }
}

impl PrinterCapabilities {
    pub fn from_parts(
        color_modes: &[String],
        raster_types: &[String],
        formats: &[String],
        resolution: Option<(i32, i32, i8)>,
        model: Option<String>,
    ) -> Self {
        let color_mode_bilevel = color_modes.iter().any(|m| m == "bi-level");
        let raster_bilevel = raster_types.iter().any(|t| t == "black_1")
            && !raster_types
                .iter()
                .any(|t| COLOR_RASTER_TYPES.contains(&t.as_str()));
        let bilevel = color_mode_bilevel || raster_bilevel;
        // The printer told us something about color iff it advertised a color-mode or raster type.
        let color_known = !color_modes.is_empty() || !raster_types.is_empty();
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
            color_known,
            accepts_png,
            resolution_dpi,
            loaded_media_width_mm: None,
            model,
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
                            ipp::value::IppValue::TextWithoutLanguage(s) => {
                                Some(AsRef::<str>::as_ref(s).to_string())
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        // printer-resolution-default is a single `resolution` value per RFC 8011, not a 1setOf.
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
        // media-col-ready -> media-size -> x-dimension (hundredths-mm Integer).
        let x_hundredths = group
            .and_then(|g| g.attributes().get("media-col-ready"))
            .and_then(|attr| first_collection(attr.value()))
            .and_then(|c| c.get("media-size"))
            .and_then(first_collection)
            .and_then(|sz| sz.get("x-dimension"))
            .and_then(|v| match v {
                ipp::value::IppValue::Integer(x) => Some(*x),
                _ => None,
            });
        let mut caps = PrinterCapabilities::from_parts(
            &[
                strings("print-color-mode-supported"),
                strings("print-color-mode-default"),
            ]
            .concat(),
            &strings("pwg-raster-document-type-supported"),
            &strings("document-format-supported"),
            resolution,
            strings("printer-make-and-model").into_iter().next(),
        );
        caps.loaded_media_width_mm = loaded_media_width_mm(x_hundredths);
        caps
    }
}

/// Per-field render overrides. Each `None` field is negotiated from the printer's capabilities; a
/// `Some` field is an explicit user choice that wins over negotiation. See [`effective_render`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOverride {
    pub color_mode: Option<crate::render::ColorMode>,
    pub resolution_dpi: Option<u32>,
}

/// Resolve the effective render options from per-field overrides and (optional) printer capabilities.
/// Each field independently is: override, else negotiated from caps, else default. Negotiated color is
/// `BiLevel` only when the printer is bilevel AND accepts PNG (BiLevel + single -> PNG artifact via
/// [`print_artifact_format`], so a bilevel-but-non-PNG printer must not be auto-switched to PNG).
pub fn effective_render(
    ovr: &RenderOverride,
    caps: Option<&PrinterCapabilities>,
) -> crate::render::ImageRenderOptions {
    use crate::render::{ColorMode, ImageRenderOptions};
    let color_mode = match ovr.color_mode {
        Some(cm) => cm,
        None => match caps {
            Some(c) if c.bilevel && c.accepts_png => ColorMode::BiLevel,
            _ => ColorMode::Color,
        },
    };
    let resolution_dpi = ovr
        .resolution_dpi
        .or_else(|| caps.and_then(|c| c.resolution_dpi));
    ImageRenderOptions {
        color_mode,
        resolution_dpi,
    }
}

/// Test-only driver that records nothing and either succeeds or fails, for exercising the `/print`
/// dispatch without a real printer.
#[cfg(test)]
struct FakeDriver {
    fail: bool,
    render: RenderOverride,
    caps: Option<PrinterCapabilities>,
    probe_unreachable: bool,
}

#[cfg(test)]
impl FakeDriver {
    fn from_value(config: &JsonValue) -> Self {
        let fail = config
            .get("fail")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let probe_unreachable = config.get("probe").and_then(|v| v.as_str()) == Some("unreachable");
        let render = config
            .get("render")
            .map(|r| RenderOverride {
                color_mode: match r.get("color_mode").and_then(|v| v.as_str()) {
                    Some("bilevel") => Some(crate::render::ColorMode::BiLevel),
                    Some("color") => Some(crate::render::ColorMode::Color),
                    _ => None,
                },
                resolution_dpi: r
                    .get("resolution")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
            })
            .unwrap_or_default();
        let caps = config.get("capabilities").map(|c| PrinterCapabilities {
            bilevel: c.get("bilevel").and_then(|v| v.as_bool()).unwrap_or(false),
            color_known: c
                .get("color_known")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            accepts_png: c
                .get("accepts_png")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            resolution_dpi: c
                .get("resolution")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            loaded_media_width_mm: c
                .get("loaded_media_width")
                .and_then(|v| v.as_f64())
                .map(|n| n as f32),
            model: c
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        });
        Self {
            fail,
            render,
            caps,
            probe_unreachable,
        }
    }

    fn capabilities_sync(&self) -> Option<PrinterCapabilities> {
        self.caps.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl PrinterDriver for FakeDriver {
    fn configured_render_override(&self) -> RenderOverride {
        self.render
    }

    async fn probe(&self) -> ProbeOutcome {
        if self.probe_unreachable {
            return ProbeOutcome::Unreachable("fake unreachable".to_string());
        }
        match self.capabilities_sync() {
            Some(c) => ProbeOutcome::Ok(c),
            None => ProbeOutcome::Unreachable("fake: no capabilities".to_string()),
        }
    }

    async fn send(&self, artifact: &[u8], _opts: &PrintOptions) -> Result<(), PrintError> {
        if self.fail {
            return Err(PrintError::Transport("fake failure".to_string()));
        }
        // Mirror the print path's per-field precedence (override else negotiated else default).
        let effective = effective_render(
            &self.configured_render_override(),
            self.capabilities_sync().as_ref(),
        );
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
            None,
        );
        assert!(c.bilevel && c.accepts_png);
        assert_eq!(c.resolution_dpi, Some(203));
        // print-color-mode bi-level + png
        let c2 = PrinterCapabilities::from_parts(
            &["bi-level".into(), "monochrome".into()],
            &[],
            &["image/png".into()],
            None,
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
            None,
        );
        assert!(!c3.bilevel);
        // no png -> accepts_png false
        let c4 = PrinterCapabilities::from_parts(
            &["bi-level".into()],
            &[],
            &["application/pdf".into()],
            None,
            None,
        );
        assert!(c4.bilevel && !c4.accepts_png);
    }

    #[test]
    fn from_parts_carries_model() {
        let caps =
            PrinterCapabilities::from_parts(&[], &[], &[], None, Some("Brother PT-2730".into()));
        assert_eq!(caps.model.as_deref(), Some("Brother PT-2730"));
    }

    #[test]
    fn color_known_distinguishes_silence_from_color() {
        // Advertised a color-capable raster type -> color known, not bilevel.
        let color = PrinterCapabilities::from_parts(&[], &["srgb_8".into()], &[], None, None);
        assert!(color.color_known && !color.bilevel);
        // Advertised nothing -> unknown.
        let silent = PrinterCapabilities::from_parts(&[], &[], &[], None, None);
        assert!(!silent.color_known && !silent.bilevel);
        // Advertised bi-level -> bilevel (and known).
        let bw = PrinterCapabilities::from_parts(&["bi-level".into()], &[], &[], None, None);
        assert!(bw.bilevel && bw.color_known);
    }

    #[test]
    fn from_attributes_reads_make_and_model_text() {
        use ipp::attribute::{IppAttribute, IppAttributeGroup, IppAttributes};
        use ipp::model::DelimiterTag;
        let attr = IppAttribute::with_name(
            "printer-make-and-model",
            ipp::value::IppValue::TextWithoutLanguage(
                ipp::value::IppTextValue::new("Brother PT-2730").unwrap(),
            ),
        )
        .unwrap();
        let mut group = IppAttributeGroup::new(DelimiterTag::PrinterAttributes);
        group.attributes_mut().insert(attr.name().clone(), attr);
        let mut attrs = IppAttributes::new();
        attrs.groups_mut().push(group);
        assert_eq!(
            PrinterCapabilities::from_attributes(&attrs)
                .model
                .as_deref(),
            Some("Brother PT-2730")
        );
    }

    #[test]
    fn resolution_conversion_rules() {
        // square dpi in range
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((300, 300, 3)), None)
                .resolution_dpi,
            Some(300)
        );
        // asymmetric -> None
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((300, 600, 3)), None)
                .resolution_dpi,
            None
        );
        // dpcm (units 4) -> None
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((118, 118, 4)), None)
                .resolution_dpi,
            None
        );
        // out of bounds -> None
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((5000, 5000, 3)), None)
                .resolution_dpi,
            None
        );
        assert_eq!(
            PrinterCapabilities::from_parts(&[], &[], &[], Some((0, 0, 3)), None).resolution_dpi,
            None
        );
    }

    #[test]
    fn override_color_still_negotiates_resolution() {
        let ovr = RenderOverride {
            color_mode: Some(crate::render::ColorMode::Color),
            resolution_dpi: None,
        };
        let caps = PrinterCapabilities::from_parts(
            &["bi-level".into()],
            &[],
            &["image/png".into()],
            Some((300, 300, 3)),
            None,
        );
        let eff = effective_render(&ovr, Some(&caps));
        assert!(matches!(eff.color_mode, crate::render::ColorMode::Color)); // override wins
        assert_eq!(eff.resolution_dpi, Some(300)); // still negotiated
    }

    #[test]
    fn override_resolution_still_negotiates_bilevel() {
        let ovr = RenderOverride {
            color_mode: None,
            resolution_dpi: Some(203),
        };
        // bilevel AND accepts PNG -> negotiated color is BiLevel.
        let caps = PrinterCapabilities::from_parts(
            &["bi-level".into()],
            &[],
            &["image/png".into()],
            Some((300, 300, 3)),
            None,
        );
        let eff = effective_render(&ovr, Some(&caps));
        assert!(matches!(eff.color_mode, crate::render::ColorMode::BiLevel)); // still negotiated
        assert_eq!(eff.resolution_dpi, Some(203)); // override wins
    }

    #[test]
    fn negotiated_bilevel_requires_png_but_resolution_stands_alone() {
        // bilevel but NO png support -> must NOT auto-pick BiLevel (would force PNG); resolution still negotiates.
        let caps = PrinterCapabilities::from_parts(
            &["bi-level".into()],
            &[],
            &[],
            Some((300, 300, 3)),
            None,
        );
        let eff = effective_render(&RenderOverride::default(), Some(&caps));
        assert!(matches!(eff.color_mode, crate::render::ColorMode::Color));
        assert_eq!(eff.resolution_dpi, Some(300));
    }

    #[test]
    fn no_override_no_caps_is_default() {
        let eff = effective_render(&RenderOverride::default(), None);
        assert!(matches!(eff.color_mode, crate::render::ColorMode::Color));
        assert_eq!(eff.resolution_dpi, None);
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
        // no render config -> both fields None (auto-negotiated at print time)
        let ovr = driver.configured_render_override();
        assert!(ovr.color_mode.is_none() && ovr.resolution_dpi.is_none());
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
        let ovr = d.configured_render_override();
        assert!(matches!(ovr.color_mode, Some(ColorMode::BiLevel)));
        assert_eq!(ovr.resolution_dpi, Some(203));
        // only resolution set -> color_mode stays None (negotiate it)
        let d_res =
            CupsDriver::from_value(&json!({ "uri": "ipp://h/q", "render": { "resolution": 203 } }))
                .unwrap();
        let ovr_res = d_res.configured_render_override();
        assert!(ovr_res.color_mode.is_none());
        assert_eq!(ovr_res.resolution_dpi, Some(203));
        // absent render config -> both None
        let d2 = CupsDriver::from_value(&json!({ "uri": "ipp://h/q" })).unwrap();
        let ovr2 = d2.configured_render_override();
        assert!(ovr2.color_mode.is_none() && ovr2.resolution_dpi.is_none());
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
        let ovr = d.configured_render_override();
        assert!(matches!(ovr.color_mode, Some(ColorMode::BiLevel)));
        // no render -> both None
        let d2 = FakeDriver::from_value(&json!({ "fail": false }));
        let ovr2 = d2.configured_render_override();
        assert!(ovr2.color_mode.is_none() && ovr2.resolution_dpi.is_none());
        // capabilities parsed correctly
        let d3 = FakeDriver::from_value(
            &json!({ "fail": false, "capabilities": { "bilevel": true, "accepts_png": true, "resolution": 203 } }),
        );
        let caps = d3.capabilities_sync().expect("caps present");
        assert!(caps.bilevel && caps.accepts_png);
        assert_eq!(caps.resolution_dpi, Some(203));
    }

    #[test]
    fn loaded_media_width_parsing() {
        // media-col x-dimension in hundredths-mm -> mm; only structured source, no name guessing.
        assert_eq!(loaded_media_width_mm(Some(1200)), Some(12.0));
        assert_eq!(loaded_media_width_mm(Some(2400)), Some(24.0));
        assert_eq!(loaded_media_width_mm(Some(0)), None);
        assert_eq!(loaded_media_width_mm(Some(-5)), None);
        assert_eq!(loaded_media_width_mm(None), None);
    }

    #[tokio::test]
    async fn fake_probe_outcomes_flow_through() {
        let d = FakeDriver::from_value(&json!({ "probe": "unreachable" }));
        assert!(matches!(d.probe().await, ProbeOutcome::Unreachable(_)));
        let d2 = FakeDriver::from_value(
            &json!({ "capabilities": { "bilevel": true, "accepts_png": true } }),
        );
        assert!(matches!(d2.probe().await, ProbeOutcome::Ok(_)));
        // capabilities() provided default mirrors probe(): Ok -> Some, Unreachable -> None.
        assert!(d.capabilities().await.is_none());
        assert!(d2.capabilities().await.is_some());
    }

    #[tokio::test]
    async fn screen_rejects_loopback_and_metadata() {
        assert!(screen_ipp_uri("ipp://127.0.0.1:631/ipp/print")
            .await
            .is_err());
        assert!(screen_ipp_uri("ipp://169.254.169.254/ipp/print")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn screen_allows_private_host_literal() {
        assert!(screen_ipp_uri("ipp://10.10.1.34:8000/ipp/print")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn send_rejects_loopback_target() {
        // The print path must screen before dialing, same as probe.
        let d = CupsDriver::from_value(&json!({ "uri": "ipp://127.0.0.1:631/ipp/print" })).unwrap();
        let opts = PrintOptions {
            artifact_format: ArtifactFormat::Pdf,
            ..PrintOptions::default()
        };
        let err = d.send(b"%PDF-", &opts).await.unwrap_err();
        assert!(matches!(err, PrintError::Transport(_)));
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
