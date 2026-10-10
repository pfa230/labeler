//! Printer driver abstraction. Every printer is reached over IPP (`CupsDriver`); the trait keeps
//! the `/print` dispatch independent of the transport and lets tests substitute a fake.

use crate::models::{PrinterConnection, RenderProfile};
use async_trait::async_trait;

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

/// IPP `media-col` `media-size`, in hundredths of a millimetre (printing, "Media size").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaSize {
    pub x_dimension: i32,
    pub y_dimension: i32,
}

#[derive(Debug, Clone)]
pub struct PrintOptions {
    pub artifact_format: ArtifactFormat,
    /// Present for a template declaring `media_width`.
    pub media_size: Option<MediaSize>,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            artifact_format: ArtifactFormat::Pdf,
            media_size: None,
        }
    }
}

/// A length in millimetres as hundredths of a millimetre, rounded to the nearest integer.
pub fn hundredths_mm(length_mm: f64) -> i32 {
    (length_mm * 100.0).round() as i32
}

/// A template's `media_width`, given in its `unit`, in millimetres.
pub fn media_width_mm(media_width: f32, unit: &str) -> f64 {
    if unit == "in" {
        f64::from(media_width) * 25.4
    } else {
        f64::from(media_width)
    }
}

/// The IPP `Print-Job` request for one job: titled `labeler`, typed by the artifact, and carrying
/// `media-col` when the template declares a media width.
pub fn print_job_request(
    uri: ipp::prelude::Uri,
    artifact: &[u8],
    opts: &PrintOptions,
) -> Result<ipp::prelude::IppRequestResponse, PrintError> {
    use ipp::prelude::*;
    use std::collections::BTreeMap;
    let payload = IppPayload::new(std::io::Cursor::new(artifact.to_vec()));
    let mut builder = IppOperationBuilder::print_job(uri, payload)
        .document_format(ipp_document_format(opts.artifact_format))
        .job_title("labeler");
    if let Some(m) = opts.media_size {
        let name = |s: &str| -> ipp::value::IppName { s.try_into().expect("static IPP name") };
        let size = IppValue::Collection(BTreeMap::from([
            (name("x-dimension"), IppValue::Integer(m.x_dimension)),
            (name("y-dimension"), IppValue::Integer(m.y_dimension)),
        ]));
        let col = IppValue::Collection(BTreeMap::from([(name("media-size"), size)]));
        builder = builder.attribute(IppAttribute::new(name("media-col"), col));
    }
    builder
        .build()
        .map(Into::into)
        .map_err(|err| PrintError::Transport(err.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("invalid printer: {0}")]
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

/// Reserved host (RFC 2606, never resolves) that routes a printer to `FakeDriver` in the test build,
/// so printer tests run the real validation and dispatch without a network.
#[cfg(test)]
const FAKE_HOST: &str = "fake.test";

/// Validate a printer's connection fields and build its driver. Used by printer CRUD (validation
/// only), probe and `/print` dispatch.
pub fn driver_for(connection: &PrinterConnection) -> Result<Box<dyn PrinterDriver>, DriverError> {
    let driver = CupsDriver::new(connection)?;
    #[cfg(test)]
    if let Some(fake) = FakeDriver::for_fake_host(connection) {
        return Ok(Box::new(fake));
    }
    Ok(Box::new(driver))
}

/// Per-field overrides from a printer's `render`; a missing field stays `None` (negotiate it).
fn render_override(render: Option<&RenderProfile>) -> RenderOverride {
    render
        .map(|r| RenderOverride {
            color_mode: match r.color_mode.as_deref() {
                Some("bilevel") => Some(crate::render::ColorMode::BiLevel),
                Some("color") => Some(crate::render::ColorMode::Color),
                _ => None,
            },
            resolution_dpi: r.resolution,
        })
        .unwrap_or_default()
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
    /// Check each connection field against its rule (printing spec, "Printer fields").
    fn new(cfg: &PrinterConnection) -> Result<Self, DriverError> {
        if !(cfg.uri.starts_with("ipp://") || cfg.uri.starts_with("ipps://")) {
            return Err(DriverError::Config(format!(
                "cups uri must start with ipp:// or ipps:// (got '{}')",
                cfg.uri
            )));
        }
        // Must be a parseable URI with a host, so a prefixed-but-malformed value (e.g. "ipp://") is
        // rejected at validation (422) rather than surfacing later as a 200 "unreachable" probe.
        match url::Url::parse(&cfg.uri) {
            Ok(u) if u.host_str().is_some_and(|h| !h.is_empty()) => {}
            Ok(_) => {
                return Err(DriverError::Config(format!(
                    "cups uri has no host: '{}'",
                    cfg.uri
                )))
            }
            Err(e) => {
                return Err(DriverError::Config(format!(
                    "invalid cups uri '{}': {e}",
                    cfg.uri
                )))
            }
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
        Ok(Self {
            uri: cfg.uri.clone(),
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            ca_cert: cfg.ca_cert.clone(),
            insecure: cfg.insecure,
            render: render_override(cfg.render.as_ref()),
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

        let uri: Uri = self.uri.parse().map_err(|err| {
            PrintError::Transport(format!("invalid printer uri '{}': {err}", self.uri))
        })?;
        let request = print_job_request(uri.clone(), artifact, opts)?;
        let response = self
            .build_client(uri, None)
            .send(request)
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

/// Test-only driver for `ipp://fake.test/?<knobs>`: records nothing and succeeds or fails, for
/// exercising the `/print` dispatch without a real printer. Knobs: `fail=1`, `probe=unreachable`,
/// `password=<s>`, and capabilities `bilevel`, `png` (0/1), `dpi`, `media` (loaded width, mm),
/// `model`; the printer reports capabilities iff at least one capability knob is present.
#[cfg(test)]
struct FakeDriver {
    fail: bool,
    render: RenderOverride,
    caps: Option<PrinterCapabilities>,
    probe_unreachable: bool,
    /// `send` fails unless this equals the connection's password (absent on both sides is equal),
    /// so a test can observe the stored secret reaching dispatch.
    expected_password: Option<String>,
    password: Option<String>,
    /// `send` fails unless the job's `media-size` matches each knob present (`media_x`, `media_y`),
    /// like a printer refusing media it does not have.
    expected_media_x: Option<i32>,
    expected_media_y: Option<i32>,
}

#[cfg(test)]
impl FakeDriver {
    fn for_fake_host(connection: &PrinterConnection) -> Option<Self> {
        let url = url::Url::parse(&connection.uri).ok()?;
        if url.host_str() != Some(FAKE_HOST) {
            return None;
        }
        let knobs: std::collections::HashMap<String, String> =
            url.query_pairs().into_owned().collect();
        let knob = |k: &str| knobs.get(k).map(String::as_str);
        let number = |k: &str| knob(k).map(|v| v.parse::<u32>().expect("numeric fake knob"));
        let has_caps = ["bilevel", "png", "dpi", "media", "model"]
            .iter()
            .any(|k| knobs.contains_key(*k));
        let bilevel = knob("bilevel") == Some("1");
        let caps = has_caps.then(|| PrinterCapabilities {
            bilevel,
            color_known: bilevel,
            accepts_png: knob("png") == Some("1"),
            resolution_dpi: number("dpi"),
            loaded_media_width_mm: number("media").map(|mm| mm as f32),
            model: knob("model").map(str::to_string),
        });
        Some(Self {
            fail: knob("fail") == Some("1"),
            render: render_override(connection.render.as_ref()),
            caps,
            probe_unreachable: knob("probe") == Some("unreachable"),
            expected_password: knob("password").map(str::to_string),
            password: connection.password.clone(),
            expected_media_x: knob("media_x").map(|v| v.parse().expect("numeric fake knob")),
            expected_media_y: knob("media_y").map(|v| v.parse().expect("numeric fake knob")),
        })
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

    async fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError> {
        if self.fail {
            return Err(PrintError::Transport("fake failure".to_string()));
        }
        if self.password != self.expected_password {
            return Err(PrintError::Transport("fake: wrong password".to_string()));
        }
        let sent = opts.media_size;
        if self
            .expected_media_x
            .is_some_and(|x| sent.map(|m| m.x_dimension) != Some(x))
            || self
                .expected_media_y
                .is_some_and(|y| sent.map(|m| m.y_dimension) != Some(y))
        {
            return Err(PrintError::Transport(format!(
                "fake: media-size {sent:?}, expected x {:?} y {:?}",
                self.expected_media_x, self.expected_media_y
            )));
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
    use serde_json::{json, Value as JsonValue};

    fn connection(v: JsonValue) -> PrinterConnection {
        serde_json::from_value(v).expect("connection fields")
    }

    fn cups(v: JsonValue) -> Result<CupsDriver, DriverError> {
        CupsDriver::new(&connection(v))
    }

    fn fake(uri: &str) -> FakeDriver {
        FakeDriver::for_fake_host(&connection(json!({ "uri": uri }))).expect("fake.test host")
    }

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
        let cfg = cups(json!({
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
        let cfg = cups(json!({ "uri": "ipp://h/q" })).unwrap();
        assert!(
            cfg.username.is_none()
                && cfg.password.is_none()
                && cfg.ca_cert.is_none()
                && !cfg.insecure
        );
    }

    #[test]
    fn cups_config_rejects_non_pem_ca_cert() {
        assert!(cups(json!({ "uri": "ipps://h/q", "ca_cert": "not a cert" })).is_err());
    }

    #[test]
    fn cups_config_parses_render_profile() {
        let cfg = cups(json!({
            "uri": "ipp://h/q",
            "render": { "color_mode": "bilevel", "resolution": 203 }
        }))
        .unwrap();
        assert!(matches!(
            cfg.render.color_mode,
            Some(crate::render::ColorMode::BiLevel)
        ));
        assert_eq!(cfg.render.resolution_dpi, Some(203));
    }

    #[test]
    fn cups_config_rejects_bad_render_profile() {
        assert!(cups(json!({ "uri": "ipp://h/q", "render": { "color_mode": "nope" } })).is_err());
        assert!(cups(json!({ "uri": "ipp://h/q", "render": { "resolution": 99999 } })).is_err());
    }

    #[test]
    fn cups_driver_render_options_reflects_profile() {
        use crate::render::ColorMode;
        let d = cups(
            json!({ "uri": "ipp://h/q", "render": { "color_mode": "bilevel", "resolution": 203 } }),
        )
        .unwrap();
        let ovr = d.configured_render_override();
        assert!(matches!(ovr.color_mode, Some(ColorMode::BiLevel)));
        assert_eq!(ovr.resolution_dpi, Some(203));
        // only resolution set -> color_mode stays None (negotiate it)
        let d_res = cups(json!({ "uri": "ipp://h/q", "render": { "resolution": 203 } })).unwrap();
        let ovr_res = d_res.configured_render_override();
        assert!(ovr_res.color_mode.is_none());
        assert_eq!(ovr_res.resolution_dpi, Some(203));
        // absent render config -> both None
        let d2 = cups(json!({ "uri": "ipp://h/q" })).unwrap();
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
        let d = FakeDriver::for_fake_host(&connection(json!({
            "uri": "ipp://fake.test/", "render": { "color_mode": "bilevel" }
        })))
        .expect("fake.test host");
        let ovr = d.configured_render_override();
        assert!(matches!(ovr.color_mode, Some(ColorMode::BiLevel)));
        // no render -> both None
        let ovr2 = fake("ipp://fake.test/").configured_render_override();
        assert!(ovr2.color_mode.is_none() && ovr2.resolution_dpi.is_none());
        // capabilities parsed correctly
        let caps = fake("ipp://fake.test/?bilevel=1&png=1&dpi=203")
            .capabilities_sync()
            .expect("caps present");
        assert!(caps.bilevel && caps.accepts_png);
        assert_eq!(caps.resolution_dpi, Some(203));
        // another host is not the fake
        assert!(FakeDriver::for_fake_host(&connection(json!({ "uri": "ipp://h/q" }))).is_none());
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

    #[test]
    fn cups_config_rejects_malformed_uri() {
        // prefixed but no host -> rejected (feeds the probe endpoint's 422 contract)
        assert!(cups(json!({ "uri": "ipp://" })).is_err());
        assert!(cups(json!({ "uri": "ipps://" })).is_err());
        assert!(cups(json!({ "uri": "http://host/ipp/print" })).is_err());
        // a well-formed one still parses
        assert!(cups(json!({ "uri": "ipp://host:631/ipp/print" })).is_ok());
    }

    #[tokio::test]
    async fn fake_probe_outcomes_flow_through() {
        let d = fake("ipp://fake.test/?probe=unreachable");
        assert!(matches!(d.probe().await, ProbeOutcome::Unreachable(_)));
        let d2 = fake("ipp://fake.test/?bilevel=1&png=1");
        assert!(matches!(d2.probe().await, ProbeOutcome::Ok(_)));
        // capabilities() provided default mirrors probe(): Ok -> Some, Unreachable -> None.
        assert!(d.capabilities().await.is_none());
        assert!(d2.capabilities().await.is_some());
    }

    #[test]
    fn lengths_become_hundredths_of_a_millimetre() {
        assert_eq!(hundredths_mm(media_width_mm(24.0, "mm")), 2400);
        assert_eq!(hundredths_mm(media_width_mm(0.47, "in")), 1194); // 1193.8: rounded, not truncated
        assert_eq!(hundredths_mm(62.499_99), 6250);
    }

    fn media_col(opts: &PrintOptions) -> Option<ipp::prelude::IppValue> {
        use ipp::prelude::*;
        let uri: Uri = "ipp://printer.test/ipp/print".parse().unwrap();
        let req = print_job_request(uri, b"%PDF", opts).expect("request");
        let value = req
            .attributes()
            .groups_of(DelimiterTag::JobAttributes)
            .find_map(|g| g.attributes().get("media-col").map(|a| a.value().clone()));
        value
    }

    #[test]
    fn a_job_carries_media_col_only_when_the_template_has_a_media_width() {
        use ipp::prelude::IppValue;
        let with = PrintOptions {
            artifact_format: ArtifactFormat::Pdf,
            media_size: Some(MediaSize {
                x_dimension: 2400,
                y_dimension: 6250,
            }),
        };
        let Some(IppValue::Collection(col)) = media_col(&with) else {
            panic!("media-col missing or not a collection");
        };
        let Some(IppValue::Collection(size)) = col.get("media-size") else {
            panic!("media-size missing");
        };
        assert_eq!(size.get("x-dimension"), Some(&IppValue::Integer(2400)));
        assert_eq!(size.get("y-dimension"), Some(&IppValue::Integer(6250)));
        assert_eq!(
            size.len(),
            2,
            "media-size carries exactly the two dimensions"
        );

        let without = PrintOptions {
            artifact_format: ArtifactFormat::Pdf,
            media_size: None,
        };
        assert!(media_col(&without).is_none());
    }

    #[tokio::test]
    #[ignore = "requires a real IPP/CUPS endpoint in LABELER_TEST_IPP_URI"]
    async fn cups_send_live() {
        let uri = std::env::var("LABELER_TEST_IPP_URI").expect("LABELER_TEST_IPP_URI");
        let driver = driver_for(&connection(json!({ "uri": uri }))).unwrap();
        driver
            .send(b"%PDF-1.4\n%%EOF\n", &PrintOptions::default())
            .await
            .unwrap();
    }
}
