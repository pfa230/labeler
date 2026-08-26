//! API routes and handler functions.
//!
//! Handlers in this module import `Json` and `Path` from `crate::extract` rather than
//! `axum::extract` so that request extraction failures automatically produce the standard
//! JSON error envelope with `AppError` rather than axum's plain text rejections (ADR-0075).

use arc_swap::ArcSwap;
use axum::{
    extract::{DefaultBodyLimit, FromRequestParts, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use axum_extra::extract::cookie::CookieJar;
use sha2::Digest;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    connector::{BrowsePage, BrowseRequest, ConnectorSchema, LabelRow, MaterializeRequest},
    errors::AppError,
    extract::{Json, Path},
    fs_safe::{self, PublishResult},
    models::{
        BatchRequest, BatchRowError, BatchSummary, ErrorResponse, HealthResponse, PrintRequest,
        ReloadResponse, RenderLabelRequest, TemplateDetail, TemplateGroupUpdate, TemplateList,
        VariableValue,
    },
    openapi::ApiDoc,
    parse::parse_template,
    reason::Reason,
    render::{render_single_label_image, render_single_label_pdf, ColorMode, ImageRenderOptions},
    store::{Printer, Store},
    templates::{
        validate_group_name, validate_template_id_stem, TemplateContent, TemplateDefinition,
        TemplateRegistry, TemplateRegistryError,
    },
};
use rustix::fd::AsFd;
use rustix::fs::{AtFlags, Mode, OFlags};

const MAX_BATCH_LABELS: usize = 500;
const MAX_PRINT_COPIES: u32 = 100;

#[derive(serde::Deserialize)]
pub struct RenderQuery {
    pub format: Option<String>,
    pub color_mode: Option<String>,
    pub resolution: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct ImportCsvQuery {
    pub template: String,
    pub mode: Option<String>,
    pub printer: Option<String>,
    pub format: Option<String>,
}

pub struct AppState {
    templates: ArcSwap<TemplateRegistry>,
    templates_dir: PathBuf,
    write_lock: Mutex<()>,
    store: Store,
    ui_dir: PathBuf,
    trust_proxy: bool,
    no_auth: bool,
    egress: crate::egress::Egress,
    connectors: crate::connector::ConnectorRegistry,
    cursor_key: crate::connector::cursor::SigningKey,
    /// Fires between a write and its reload, so a test can stage the mid-request directory change
    /// the post-write confirmation exists to catch. Compiled out of the shipped binary: the service
    /// cannot cause that interleaving itself, and without a seam the endpoints' collision handling
    /// has no regression coverage at all.
    #[cfg(test)]
    mid_write_hook: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// Fires after the create guard and before the file is published, for the other interleaving a
    /// request cannot stage: a file arriving at the destination name once the guard has passed.
    #[cfg(test)]
    pre_publish_hook: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl AppState {
    pub fn new(registry: TemplateRegistry, templates_dir: PathBuf, store: Store) -> Self {
        Self {
            templates: ArcSwap::from_pointee(registry),
            templates_dir,
            write_lock: Mutex::new(()),
            store,
            ui_dir: std::env::var_os("LABELER_UI_DIR")
                .map(Into::into)
                .unwrap_or_else(|| PathBuf::from("ui/dist")),
            trust_proxy: std::env::var("LABELER_TRUST_PROXY")
                .map(|v| v == "true")
                .unwrap_or(false),
            #[cfg(test)]
            mid_write_hook: std::sync::Mutex::new(None),
            #[cfg(test)]
            pre_publish_hook: std::sync::Mutex::new(None),
            no_auth: std::env::var("LABELER_NO_AUTH")
                .map(|v| v == "true")
                .unwrap_or(false),
            egress: crate::egress::Egress::new(),
            connectors: crate::connector::ConnectorRegistry::default(),
            cursor_key: crate::connector::cursor::SigningKey::random(),
        }
    }

    pub fn with_ui_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.ui_dir = dir.into();
        self
    }

    pub fn with_no_auth(mut self, no_auth: bool) -> Self {
        self.no_auth = no_auth;
        self
    }

    pub fn ui_dir(&self) -> &std::path::Path {
        &self.ui_dir
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn trust_proxy(&self) -> bool {
        self.trust_proxy
    }

    pub fn no_auth(&self) -> bool {
        self.no_auth
    }

    pub fn egress(&self) -> &crate::egress::Egress {
        &self.egress
    }

    pub fn connectors(&self) -> &crate::connector::ConnectorRegistry {
        &self.connectors
    }

    pub fn cursor_key(&self) -> &crate::connector::cursor::SigningKey {
        &self.cursor_key
    }

    #[cfg(test)]
    pub fn with_loopback_egress(mut self) -> Self {
        self.egress = crate::egress::Egress::with_loopback();
        self
    }

    /// Install the between-write-and-reload hook. Test-only; see the field.
    #[cfg(test)]
    pub fn set_mid_write_hook(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.mid_write_hook.lock().expect("hook lock") = Some(Box::new(hook));
    }

    /// Install the guard-passed-but-not-yet-published hook. Test-only; see the field.
    #[cfg(test)]
    pub fn set_pre_publish_hook(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.pre_publish_hook.lock().expect("hook lock") = Some(Box::new(hook));
    }

    /// Called by each write endpoint after its write and before its reload.
    fn after_write(&self) {
        #[cfg(test)]
        {
            let hook = self.mid_write_hook.lock().expect("hook lock");
            if let Some(hook) = hook.as_ref() {
                hook();
            }
        }
    }

    /// Called by the create endpoint after its guard and before it publishes.
    fn before_publish(&self) {
        #[cfg(test)]
        {
            let hook = self.pre_publish_hook.lock().expect("hook lock");
            if let Some(hook) = hook.as_ref() {
                hook();
            }
        }
    }

    /// Read the templates directory without publishing the result.
    ///
    /// For a decision that may refuse the request: `reload` swaps the new reading in, so using it to
    /// decide would change what the service serves even when the request is then refused, which a
    /// refused delete must not do (#183).
    fn read_templates(&self) -> Result<TemplateRegistry, TemplateRegistryError> {
        TemplateRegistry::load_from_dir(&self.templates_dir)
    }

    /// Make `registry` the served set, logging whatever it refused.
    fn publish(&self, registry: TemplateRegistry) -> (usize, usize) {
        let count = registry.len();
        let broken_count = registry.broken().len();
        if broken_count > 0 {
            for b in registry.broken() {
                tracing::warn!(path = %b.path, error = %b.error, "template failed to load");
            }
        }
        self.templates.store(Arc::new(registry));
        (count, broken_count)
    }

    // Synchronous filesystem I/O. Acceptable for the single-user, local-templates-dir target and
    // consistent with the synchronous Typst render path; revisit with spawn_blocking if it ever
    // serves large dirs or remote storage.
    fn reload(&self) -> Result<(usize, usize), TemplateRegistryError> {
        let registry = self.read_templates()?;
        Ok(self.publish(registry))
    }
}

fn api_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/templates", get(list_templates))
        .route("/template-groups", get(list_groups))
        .route(
            "/template-groups/{*path}",
            axum::routing::delete(delete_group),
        )
        .route("/templates/reload", post(reload_templates))
        .route(
            "/templates/{id}",
            get(get_template).put(put_template).delete(delete_template),
        )
        .route("/templates/{id}/group", put(update_template_group))
        .route("/templates/{id}/source", get(template_source))
        .route("/templates/{id}/thumbnail", get(thumbnail))
        .route("/printers", get(list_printers).post(create_printer))
        .route("/printers/probe", post(probe_printer))
        .route(
            "/printers/{id}",
            get(get_printer).put(replace_printer).delete(delete_printer),
        )
        .route(
            "/printers/{id}/default",
            post(set_printer_default).delete(clear_printer_default),
        )
        .route(
            "/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/connections/{id}",
            get(get_connection_h)
                .put(update_connection_h)
                .delete(delete_connection_h),
        )
        .route("/connections/{id}/schema", get(connection_schema))
        .route("/connections/{id}/browse", post(connection_browse))
        .route(
            "/connections/{id}/materialize",
            post(connection_materialize),
        )
        .route("/variables", get(get_variables))
        .route("/variables/{key}", put(put_variable))
        .route("/settings", get(get_settings))
        .route("/settings/{key}", put(put_setting).delete(delete_setting))
        .route("/datetime-formats/preview", post(preview_datetime_format))
        .route("/render/label", post(render_label))
        .route("/batch", post(batch))
        .route(
            "/print",
            post(print_label).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/import/csv", post(import_csv))
        .route("/favorites", get(list_favorites))
        .route(
            "/favorites/{template_id}",
            put(add_favorite).delete(remove_favorite),
        )
        .route("/recent-templates", get(recent_templates))
        .route("/auth/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
        .route("/users", get(list_users).post(create_user_h))
        .route("/users/{id}", axum::routing::delete(delete_user_h))
        .route("/tokens", get(list_tokens).post(create_token_h))
        .route("/tokens/{id}", axum::routing::delete(delete_token_h))
        // Serve the OpenAPI doc from an explicit route so it resolves at /api/openapi.json under the
        // `/api` nest (SwaggerUi's own `.url()` serving route gets double-prefixed when nested).
        .route("/openapi.json", get(openapi_json))
        // SwaggerUi serves the UI at /api/docs/ (trailing slash).
        .merge(SwaggerUi::new("/docs").url("/api/openapi.json", ApiDoc::openapi()))
}

async fn openapi_json() -> Response {
    Json(ApiDoc::openapi()).into_response()
}

pub fn app(state: Arc<AppState>) -> Router {
    let assets = tower_http::services::ServeDir::new(state.ui_dir().join("assets"));
    let api = api_router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::require_auth,
    ));
    Router::new()
        .nest("/api", api)
        .nest_service("/assets", assets)
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn fallback(State(state): State<Arc<AppState>>, uri: axum::http::Uri) -> Response {
    if uri.path() == "/api" || uri.path().starts_with("/api/") {
        return AppError::not_found(uri.path()).into_response();
    }
    // SPA: serve index.html for any non-API, non-asset route (client-side routing).
    match tokio::fs::read(state.ui_dir().join("index.html")).await {
        Ok(bytes) => (
            axum::http::StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            "UI not built; run `npm --prefix ui run build`",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Debug, Default, serde::Deserialize, utoipa::IntoParams)]
pub struct TemplateListQuery {
    pub group: Option<String>,
    #[serde(default)]
    pub nested: bool,
}

#[utoipa::path(
    get,
    path = "/templates",
    params(
        ("group" = Option<String>, Query, description = "Filter templates by group. Omit for all templates; pass empty (?group=) for ungrouped templates."),
        ("nested" = Option<bool>, Query, description = "Include templates in descendant subgroups. Defaults to false.")
    ),
    responses(
        (status = 200, description = "List templates", body = TemplateList)
    )
)]
pub async fn list_templates(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TemplateListQuery>,
) -> impl IntoResponse {
    let registry = state.templates.load_full();
    let mut templates = registry.summaries();
    if let Some(ref group) = query.group {
        let stripped = group.trim();
        if stripped.is_empty() {
            templates.retain(|t| t.group.is_none());
        } else if query.nested {
            let prefix = format!("{stripped}/");
            templates.retain(|t| {
                t.group
                    .as_deref()
                    .is_some_and(|g| g == stripped || g.starts_with(&prefix))
            });
        } else {
            templates.retain(|t| t.group.as_deref() == Some(stripped));
        }
    }
    let broken = registry
        .broken()
        .iter()
        .map(|b| crate::models::BrokenTemplateSummary {
            path: b.path.clone(),
            error: b.error.clone(),
        })
        .collect();
    Json(TemplateList { templates, broken })
}

#[utoipa::path(
    post,
    path = "/templates/reload",
    responses(
        (status = 200, description = "Templates reloaded from disk", body = ReloadResponse),
        (status = 500, description = "Failed to read the templates directory", body = ErrorResponse)
    )
)]
pub async fn reload_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadResponse>, AppError> {
    // Under the same lock the write endpoints hold. Swapping the registry mid-write would let this
    // reload land between a handler's confirmation and the detail it answers with, which is exactly
    // the substitution the confirmation exists to prevent (#184).
    let _guard = state.write_lock.lock().await;
    let (count, broken_count) = state.reload()?;
    Ok(Json(ReloadResponse {
        count,
        broken_count,
    }))
}

#[utoipa::path(
    get,
    path = "/template-groups",
    responses(
        (status = 200, description = "List template group paths", body = Vec<String>),
        (status = 500, description = "Failed to read the templates directory", body = ErrorResponse)
    )
)]
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, AppError> {
    let groups = crate::templates::list_template_groups(&state.templates_dir)
        .map_err(|err| AppError::render_failed(Reason::TemplateRegistryIo, err.to_string()))?;
    Ok(Json(groups))
}

fn check_percent_encoding(raw: &str) -> Result<(), AppError> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return Err(AppError::invalid_request(
                    Reason::PathParamInvalid,
                    format!("malformed percent-encoding in path: '{raw}'"),
                ));
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    Ok(())
}

#[utoipa::path(
    delete,
    path = "/template-groups/{path}",
    params(
        ("path" = String, Path, description = "Group path")
    ),
    responses(
        (status = 204, description = "Group directory deleted"),
        (status = 400, description = "Malformed percent sequence, invalid group name, or symlink on path", body = ErrorResponse),
        (status = 404, description = "Group not found or case mismatch", body = ErrorResponse),
        (status = 409, description = "Group is not empty", body = ErrorResponse),
        (status = 500, description = "Failed to delete group directory", body = ErrorResponse)
    )
)]
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    uri: axum::http::Uri,
) -> Result<Response, AppError> {
    let raw_uri_path = uri.path();
    let raw_group = if let Some(p) = raw_uri_path.strip_prefix("/template-groups/") {
        p
    } else if let Some(p) = raw_uri_path.strip_prefix("/api/template-groups/") {
        p
    } else {
        return Err(AppError::invalid_request(
            Reason::PathParamInvalid,
            "missing group path",
        ));
    };

    check_percent_encoding(raw_group)?;

    let decoded = urlencoding::decode(raw_group).map_err(|_| {
        AppError::invalid_request(Reason::PathParamInvalid, "group path is not valid UTF-8")
    })?;

    let validated = validate_group_name(&decoded)
        .map_err(|err| AppError::invalid_request(Reason::TemplateGroupInvalid, err))?;

    let _guard = state.write_lock.lock().await;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    let (parent_fd, segment) = fs_safe::resolve_group_for_delete(root_fd.as_fd(), &validated)?;

    match rustix::fs::unlinkat(parent_fd.as_fd(), &segment, AtFlags::REMOVEDIR) {
        Ok(()) => {
            state.reload()?;
            Ok((axum::http::StatusCode::NO_CONTENT, ()).into_response())
        }
        Err(rustix::io::Errno::NOTEMPTY) | Err(rustix::io::Errno::EXIST) => Err(
            AppError::conflict(format!("group '{validated}' is not empty")),
        ),
        Err(rustix::io::Errno::NOENT) => Err(AppError::not_found(&validated)),
        Err(err) => Err(AppError::internal(format!(
            "failed to delete group '{validated}': {err}"
        ))),
    }
}

fn parse_and_validate(body: &str) -> Result<TemplateContent, AppError> {
    let content = parse_template(body)
        .map_err(|err| AppError::template_invalid(Reason::TemplateParseFailed, err.to_string()))?;
    content
        .validate()
        .map_err(|err| AppError::template_invalid(Reason::TemplateValidationFailed, err))?;
    Ok(content)
}

fn file_label(templates_dir: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(templates_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn confirm_written_template(
    registry: &TemplateRegistry,
    id: &str,
    path: &std::path::Path,
    body: &str,
) -> Result<(), AppError> {
    use sha2::Sha256;

    let want = hex::encode(Sha256::digest(body.as_bytes()));
    let served = registry.path(id);
    if served == Some(path) && registry.content_hash(id) == Some(want.as_str()) {
        return Ok(());
    }

    let missing = || {
        AppError::render_failed(
            Reason::TemplateMissingAfterWrite,
            format!(
                "template '{id}' is missing after the write to {}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            ),
        )
    };

    let Some(winner) = served.filter(|served| *served != path) else {
        return Err(missing());
    };

    let refused = registry.duplicates(id);
    if !refused
        .iter()
        .any(|refused_rel| path.ends_with(refused_rel))
    {
        return Err(missing());
    }

    match std::fs::read_to_string(path) {
        Ok(on_disk) if on_disk == body => {
            let winner_rel = registry
                .rel_path(id)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| winner.file_name().unwrap().to_string_lossy().into_owned());
            let mut files = vec![winner_rel];
            files.extend(
                refused
                    .iter()
                    .map(|p| p.to_string_lossy().replace('\\', "/")),
            );
            let this_file_display = path.file_name().unwrap().to_string_lossy();
            let winner_display = winner.file_name().unwrap().to_string_lossy();
            Err(AppError::template_id_collision(
                id,
                files,
                format!(
                    "template id '{id}' is declared by both {winner_display} and {this_file_display}; {winner_display} is served and the file just written is refused"
                ),
            ))
        }
        _ => Err(missing()),
    }
}

#[derive(Debug, Default, serde::Deserialize, utoipa::IntoParams)]
pub struct PutTemplateQuery {
    pub group: Option<String>,
}

#[utoipa::path(
    put,
    path = "/templates/{id}",
    params(
        ("id" = String, Path, description = "Template ID"),
        ("group" = Option<String>, Query, description = "Group path for create (optional)")
    ),
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 200, description = "Template replaced", body = TemplateDetail),
        (status = 201, description = "Template created", body = TemplateDetail),
        (status = 400, description = "Invalid id, template group mismatch, or unsupported precondition", body = ErrorResponse),
        (status = 409, description = "After the write, the id is served from a different file", body = ErrorResponse),
        (status = 412, description = "Precondition failed (If-None-Match: * and template exists)", body = ErrorResponse),
        (status = 422, description = "Invalid template or group", body = ErrorResponse),
        (status = 500, description = "The write failed, the directory could not be re-read, or the written template is missing afterwards", body = ErrorResponse)
    )
)]
pub async fn put_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<PutTemplateQuery>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Response, AppError> {
    let create_only = if let Some(if_none_match) = headers.get("if-none-match") {
        let val = if_none_match.to_str().map_err(|_| {
            AppError::unsupported_precondition("unsupported If-None-Match header value")
        })?;
        if val.trim() == "*" {
            true
        } else {
            return Err(AppError::unsupported_precondition(
                "unsupported If-None-Match header value; only '*' is supported",
            ));
        }
    } else {
        false
    };

    if !validate_template_id_stem(&id) {
        return Err(AppError::invalid_request(
            Reason::TemplateIdInvalid,
            format!("template id '{id}' must be non-empty and match ^[a-zA-Z0-9_-]+$"),
        ));
    }

    let _content = parse_and_validate(&body)?;

    let _guard = state.write_lock.lock().await;
    state.reload()?;
    let registry = state.templates.load_full();
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;

    if let Some(existing) = registry.get(&id) {
        if create_only {
            return Err(AppError::precondition_failed(format!(
                "template with id '{id}' already exists"
            )));
        }

        if let Some(ref req_grp) = query.group {
            let stripped = req_grp.trim();
            let req_group_opt = if stripped.is_empty() {
                None
            } else {
                Some(stripped)
            };
            if req_group_opt != existing.group.as_deref() {
                return Err(AppError::template_group_mismatch(format!(
                    "template '{id}' already exists in group '{}'; use PUT /api/templates/{id}/group to move it",
                    existing.group.as_deref().unwrap_or("ungrouped")
                )));
            }
        }

        let resolved =
            fs_safe::resolve_or_create_group(root_fd.as_fd(), existing.group.as_deref(), false)?;
        let target_filename = format!("{id}.yaml");

        match rustix::fs::openat(
            resolved.target_fd.as_fd(),
            &target_filename,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(_) => {}
            Err(rustix::io::Errno::LOOP) => {
                return Err(AppError::render_failed(
                    Reason::TemplateGroupUnsafePath,
                    "destination template file is a symbolic link",
                ));
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(err) => {
                return Err(AppError::render_failed(
                    Reason::TemplateWriteFailed,
                    format!("failed to check destination: {err}"),
                ));
            }
        }

        state.before_publish();
        fs_safe::stage_and_replace(resolved.target_fd.as_fd(), &target_filename, &body)?;
        state.after_write();
        state.reload()?;
        let new_registry = state.templates.load_full();
        let dest_path = state
            .templates_dir
            .join(&resolved.target_path)
            .join(&target_filename);
        confirm_written_template(&new_registry, &id, &dest_path, &body)?;
        let detail = new_registry.detail(&id).ok_or_else(|| {
            AppError::render_failed(
                Reason::TemplateMissingAfterWrite,
                "template missing after write",
            )
        })?;
        Ok((axum::http::StatusCode::OK, Json(detail)).into_response())
    } else {
        let group_req = query
            .group
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let resolved = fs_safe::resolve_or_create_group(root_fd.as_fd(), group_req, true)?;
        let target_filename = format!("{id}.yaml");

        state.before_publish();
        let pub_res =
            fs_safe::stage_and_publish_new(resolved.target_fd.as_fd(), &target_filename, &body);

        match pub_res {
            Ok(PublishResult::Published) => {
                state.after_write();
                state.reload()?;
                let new_registry = state.templates.load_full();
                let dest_path = state
                    .templates_dir
                    .join(&resolved.target_path)
                    .join(&target_filename);
                confirm_written_template(&new_registry, &id, &dest_path, &body)?;
                let detail = new_registry.detail(&id).ok_or_else(|| {
                    AppError::render_failed(
                        Reason::TemplateMissingAfterWrite,
                        "template missing after write",
                    )
                })?;
                Ok((axum::http::StatusCode::CREATED, Json(detail)).into_response())
            }
            Ok(PublishResult::AlreadyExists) => {
                if create_only {
                    fs_safe::cleanup_created_dirs(resolved.created_dirs);
                    return Err(AppError::precondition_failed(format!(
                        "a file for template '{id}' already exists"
                    )));
                }

                match rustix::fs::openat(
                    resolved.target_fd.as_fd(),
                    &target_filename,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                ) {
                    Ok(_) => {}
                    Err(rustix::io::Errno::LOOP) => {
                        fs_safe::cleanup_created_dirs(resolved.created_dirs);
                        return Err(AppError::render_failed(
                            Reason::TemplateGroupUnsafePath,
                            "destination template file is a symbolic link",
                        ));
                    }
                    Err(err) => {
                        fs_safe::cleanup_created_dirs(resolved.created_dirs);
                        return Err(AppError::render_failed(
                            Reason::TemplateWriteFailed,
                            format!("failed to check destination: {err}"),
                        ));
                    }
                }

                if let Err(err) =
                    fs_safe::stage_and_replace(resolved.target_fd.as_fd(), &target_filename, &body)
                {
                    fs_safe::cleanup_created_dirs(resolved.created_dirs);
                    return Err(err);
                }

                state.after_write();
                state.reload()?;
                let new_registry = state.templates.load_full();
                let dest_path = state
                    .templates_dir
                    .join(&resolved.target_path)
                    .join(&target_filename);
                confirm_written_template(&new_registry, &id, &dest_path, &body)?;
                let detail = new_registry.detail(&id).ok_or_else(|| {
                    AppError::render_failed(
                        Reason::TemplateMissingAfterWrite,
                        "template missing after write",
                    )
                })?;
                Ok((axum::http::StatusCode::OK, Json(detail)).into_response())
            }
            Err(err) => {
                fs_safe::cleanup_created_dirs(resolved.created_dirs);
                Err(err)
            }
        }
    }
}

#[utoipa::path(
    put,
    path = "/templates/{id}/group",
    params(("id" = String, Path, description = "Template ID")),
    request_body(content = TemplateGroupUpdate, description = "Group assignment"),
    responses(
        (status = 200, description = "Template moved to group", body = TemplateDetail),
        (status = 400, description = "Invalid id or request body", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 409, description = "Destination already exists or id collision", body = ErrorResponse),
        (status = 422, description = "Invalid group name or case clash", body = ErrorResponse),
        (status = 500, description = "File move failed or template missing afterwards", body = ErrorResponse)
    )
)]
pub async fn update_template_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<TemplateGroupUpdate>,
) -> Result<Response, AppError> {
    // Request syntax first, before the id and before any filesystem work: a body that does not carry
    // the key is a bad request whatever the directory holds, and deciding it here keeps an unknown
    // id or an unreadable directory from answering 404 or 500 in its place.
    let group = update.group().ok_or_else(|| {
        AppError::invalid_request(
            Reason::RequestBodyInvalid,
            "body must carry a 'group' key; use null to clear the group",
        )
    })?;

    if !validate_template_id_stem(&id) {
        return Err(AppError::invalid_request(
            Reason::TemplateIdInvalid,
            format!("template id '{id}' must be non-empty and match ^[a-zA-Z0-9_-]+$"),
        ));
    }

    let _guard = state.write_lock.lock().await;
    state.reload()?;
    let registry = state.templates.load_full();
    let existing = registry
        .get(&id)
        .ok_or_else(|| AppError::template_not_found(id.clone()))?;
    let src_path = registry
        .path(&id)
        .ok_or_else(|| AppError::template_not_found(id.clone()))?;
    let src_filename = src_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let target_group: Option<&str> = match group {
        None => None,
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(AppError::template_group_invalid(
                    "group path cannot be empty; use null to clear the group",
                ));
            }
            crate::templates::validate_group_name(trimmed)
                .map_err(|e| AppError::template_group_invalid(e.to_string()))?;
            Some(trimmed)
        }
    };
    if existing.group.as_deref() == target_group {
        let detail = registry.detail(&id).unwrap();
        return Ok((axum::http::StatusCode::OK, Json(detail)).into_response());
    }

    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    let src_resolved =
        fs_safe::resolve_or_create_group(root_fd.as_fd(), existing.group.as_deref(), false)?;
    let dest_resolved = fs_safe::resolve_or_create_group(root_fd.as_fd(), target_group, true)?;
    let dest_filename = src_filename.clone();

    let dest_exists = rustix::fs::openat(
        dest_resolved.target_fd.as_fd(),
        &dest_filename,
        OFlags::RDONLY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .is_ok();

    if dest_exists {
        fs_safe::cleanup_created_dirs(dest_resolved.created_dirs);
        let rel_dest = dest_resolved
            .target_path
            .join(&dest_filename)
            .to_string_lossy()
            .replace('\\', "/");
        return Err(AppError::template_id_collision(
            &id,
            vec![rel_dest.clone()],
            format!("destination '{rel_dest}' already exists"),
        ));
    }

    state.before_publish();
    if let Err(err) = fs_safe::move_template_file(
        src_resolved.target_fd.as_fd(),
        &src_filename,
        dest_resolved.target_fd.as_fd(),
        &dest_filename,
    ) {
        fs_safe::cleanup_created_dirs(dest_resolved.created_dirs);
        return Err(err);
    }

    state.after_write();
    state.reload()?;
    let new_registry = state.templates.load_full();
    let dest_full_path = state
        .templates_dir
        .join(&dest_resolved.target_path)
        .join(&dest_filename);
    let content_str = std::fs::read_to_string(&dest_full_path)
        .map_err(|e| AppError::render_failed(Reason::TemplateRegistryIo, e.to_string()))?;
    confirm_written_template(&new_registry, &id, &dest_full_path, &content_str)?;
    let detail = new_registry.detail(&id).ok_or_else(|| {
        AppError::render_failed(
            Reason::TemplateMissingAfterWrite,
            "template missing after move",
        )
    })?;
    Ok((axum::http::StatusCode::OK, Json(detail)).into_response())
}

#[utoipa::path(
    delete,
    path = "/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 204, description = "Template deleted"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 409, description = "More than one file on disk declares this id", body = ErrorResponse),
        (status = 500, description = "File removal, the favorites prune, or the directory re-read failed", body = ErrorResponse)
    )
)]
pub async fn delete_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    if !validate_template_id_stem(&id) {
        return Err(AppError::invalid_request(
            Reason::TemplateIdInvalid,
            format!("template id '{id}' must be non-empty and match ^[a-zA-Z0-9_-]+$"),
        ));
    }
    let _guard = state.write_lock.lock().await;
    let registry = state.read_templates()?;
    let existing = match registry.get(&id) {
        Some(t) => t,
        None => {
            state.publish(registry);
            return Err(AppError::template_not_found(id));
        }
    };
    let path = registry.path(&id).unwrap().to_path_buf();
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
        .to_string();

    let refused = registry.duplicates(&id);
    if !refused.is_empty() {
        let mut files = vec![file_label(&state.templates_dir, &path)];
        files.extend(refused.iter().map(|p| file_label(&state.templates_dir, p)));
        let named = files.join(", ");
        return Err(AppError::template_id_collision(
            &id,
            files,
            format!(
                "template id '{id}' is declared by more than one file ({named}); remove or re-id the extra file before deleting"
            ),
        ));
    }

    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    let parent_resolved =
        fs_safe::resolve_or_create_group(root_fd.as_fd(), existing.group.as_deref(), false)?;
    drop(registry);

    fs_safe::unlink_file(parent_resolved.target_fd.as_fd(), &filename)?;

    state.store().remove_favorites_for_template(&id).await?;
    state.after_write();
    state.reload()?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(
    get,
    path = "/templates/{id}",
    params(
        ("id" = String, Path, description = "Template ID")
    ),
    responses(
        (status = 200, description = "Template details", body = TemplateDetail),
        (status = 404, description = "Template not found", body = ErrorResponse)
    )
)]
pub async fn get_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TemplateDetail>, AppError> {
    state
        .templates
        .load_full()
        .detail(&id)
        .map(Json)
        .ok_or_else(|| AppError::template_not_found(id))
}

#[utoipa::path(
    get,
    path = "/templates/{id}/source",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 200, description = "Raw template YAML", content_type = "text/yaml"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse)
    )
)]
pub async fn template_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    if !validate_template_id_stem(&id) {
        return Err(AppError::invalid_request(
            Reason::TemplateIdInvalid,
            format!("template id '{id}' must be non-empty and match ^[a-zA-Z0-9_-]+$"),
        ));
    }
    let registry = state.templates.load_full();
    let path = registry
        .path(&id)
        .ok_or_else(|| AppError::template_not_found(id.clone()))?;
    let yaml = std::fs::read_to_string(path).map_err(|_| AppError::template_not_found(id))?;
    Ok((
        axum::http::StatusCode::OK,
        [("content-type", "text/yaml; charset=utf-8")],
        yaml,
    )
        .into_response())
}

#[utoipa::path(
    get,
    path = "/templates/{id}/thumbnail",
    params(("id" = String, Path, description = "Template id")),
    responses(
        (status = 200, description = "Rendered PNG thumbnail", content_type = "image/png", body = Vec<u8>),
        (status = 304, description = "Not modified (ETag match)"),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 422, description = "Render/interpolation error", body = ErrorResponse),
    )
)]
pub async fn thumbnail(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&id)
        .ok_or_else(|| AppError::template_not_found(id.clone()))?;
    let data = crate::render::placeholder_data(template);
    let option = crate::render::default_option_selection(template);
    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let png =
        crate::render::render_thumbnail_png(template, &data, option.as_ref(), &variables, &dt)?;

    // #129: key the ETag on the rendered bytes, not the template YAML. The image depends on the
    // template AND the renderer AND the variables it interpolates AND the datetime formats, so a key
    // built from a list of inputs goes stale as soon as that list is incomplete — which is the bug
    // this replaces (the YAML hash covered one input of four). Hashing the payload cannot be
    // incomplete. The cost is that revalidation no longer skips the render, so a `304` saves the
    // transfer but not the work; at catalog scale that is well under a second per grid refresh.
    let etag = format!("\"{}\"", hex::encode(sha2::Sha256::digest(&png)));

    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH) {
        if inm.to_str().map(|v| v == "*" || v == etag).unwrap_or(false) {
            return Ok((
                axum::http::StatusCode::NOT_MODIFIED,
                [(axum::http::header::ETAG, etag.as_str())],
            )
                .into_response());
        }
    }

    Ok((
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (axum::http::header::ETAG, etag.as_str()),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        png,
    )
        .into_response())
}

fn validate_printer(printer: &Printer) -> Result<(), AppError> {
    if printer.id.is_empty()
        || !printer
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid_request(
            Reason::PrinterIdInvalid,
            format!(
                "printer id '{}' must be non-empty and contain only letters, digits, '-' or '_'",
                printer.id
            ),
        ));
    }
    if printer.name.trim().is_empty() {
        return Err(AppError::printer_invalid("printer name must not be empty"));
    }
    crate::driver::validate_config(&printer.kind, &printer.config)
        .map_err(|err| AppError::printer_invalid(err.to_string()))?;
    Ok(())
}

#[utoipa::path(
    get,
    path = "/printers",
    responses((status = 200, description = "List printers", body = [Printer]))
)]
pub async fn list_printers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Printer>>, AppError> {
    let mut printers = state.store().list_printers().await?;
    for p in &mut printers {
        p.config = crate::driver::redact_config(&p.kind, &p.config);
    }
    Ok(Json(printers))
}

#[utoipa::path(
    post,
    path = "/printers",
    request_body = Printer,
    responses(
        (status = 201, description = "Printer created", body = Printer),
        (status = 409, description = "Printer id already exists", body = ErrorResponse),
        (status = 422, description = "Invalid printer", body = ErrorResponse)
    )
)]
pub async fn create_printer(
    State(state): State<Arc<AppState>>,
    Json(mut printer): Json<Printer>,
) -> Result<Response, AppError> {
    validate_printer(&printer)?;
    let _guard = state.write_lock.lock().await;
    if state.store().get_printer(&printer.id).await?.is_some() {
        return Err(AppError::printer_exists(&printer.id));
    }
    crate::driver::merge_secrets(&printer.kind, &mut printer.config, None);
    state.store().upsert_printer(&printer).await?;
    printer.config = crate::driver::redact_config(&printer.kind, &printer.config);
    printer.is_default = false; // a new row is never default; upsert_printer ignores is_default
    Ok((axum::http::StatusCode::CREATED, Json(printer)).into_response())
}

#[utoipa::path(
    get,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 200, description = "Printer", body = Printer),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn get_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Printer>, AppError> {
    let mut printer = state
        .store()
        .get_printer(&id)
        .await?
        .ok_or_else(|| AppError::printer_not_found(id))?;
    printer.config = crate::driver::redact_config(&printer.kind, &printer.config);
    Ok(Json(printer))
}

#[utoipa::path(
    get,
    path = "/variables",
    responses((status = 200, description = "All variables", body = std::collections::BTreeMap<String, String>))
)]
pub async fn get_variables(
    State(state): State<Arc<AppState>>,
) -> Result<Json<std::collections::BTreeMap<String, String>>, AppError> {
    Ok(Json(state.store().all_variables().await?))
}

#[utoipa::path(
    put,
    path = "/variables/{key}",
    params(("key" = String, Path, description = "Variable key")),
    request_body = VariableValue,
    responses(
        (status = 200, description = "Variable stored", body = VariableValue),
        (status = 400, description = "Invalid key", body = ErrorResponse)
    )
)]
pub async fn put_variable(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(body): Json<VariableValue>,
) -> Result<Json<VariableValue>, AppError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(AppError::invalid_request(
            Reason::VariableKeyInvalid,
            format!("variable key '{key}' must be non-empty and contain only letters, digits, '_', '-' or '.'"),
        ));
    }
    let _guard = state.write_lock.lock().await;
    state.store().set_variable(&key, &body.value).await?;
    Ok(Json(body))
}

/// A resolved application setting: its effective value and whether that is the in-code default.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ResolvedSetting {
    pub value: serde_json::Value,
    pub is_default: bool,
}

/// Request body for `PUT /settings/{key}`: the new value, validated per setting.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct SettingValue {
    pub value: serde_json::Value,
}

#[utoipa::path(
    get,
    path = "/settings",
    tag = "settings",
    responses((status = 200, description = "Resolved application settings", body = std::collections::BTreeMap<String, ResolvedSetting>))
)]
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    use std::collections::BTreeMap;
    let stored = state
        .store()
        .get_setting(crate::settings::JOB_LOG_RETENTION_DAYS)
        .await?;
    let is_default = stored.is_none();
    let days = crate::settings::resolve_retention_days_from(stored)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let mut out: BTreeMap<String, ResolvedSetting> = BTreeMap::new();
    out.insert(
        crate::settings::JOB_LOG_RETENTION_DAYS.to_string(),
        ResolvedSetting {
            value: serde_json::json!(days),
            is_default,
        },
    );
    let dt_stored = state
        .store()
        .get_setting(crate::settings::DATETIME_FORMATS)
        .await?;
    let dt_is_default = dt_stored.is_none();
    let dt_formats = crate::settings::resolve_datetime_formats_from(dt_stored)
        .map_err(|e| AppError::internal(e.to_string()))?;
    out.insert(
        crate::settings::DATETIME_FORMATS.to_string(),
        ResolvedSetting {
            value: serde_json::json!(dt_formats),
            is_default: dt_is_default,
        },
    );
    let max_dim_stored = state
        .store()
        .get_setting(crate::settings::MAX_LABEL_DIMENSION_MM)
        .await?;
    let max_dim_is_default = max_dim_stored.is_none();
    let max_dim = crate::settings::resolve_max_label_dimension_mm_from(max_dim_stored)
        .map_err(|e| AppError::internal(e.to_string()))?;
    out.insert(
        crate::settings::MAX_LABEL_DIMENSION_MM.to_string(),
        ResolvedSetting {
            value: serde_json::json!(max_dim),
            is_default: max_dim_is_default,
        },
    );
    let def_conn_stored = state
        .store()
        .get_setting(crate::settings::DEFAULT_CONNECTION_ID)
        .await?;
    let def_conn_is_default = def_conn_stored.is_none();
    let def_conn_id = crate::settings::resolve_default_connection_id_from(def_conn_stored)
        .map_err(|e| AppError::internal(e.to_string()))?;
    out.insert(
        crate::settings::DEFAULT_CONNECTION_ID.to_string(),
        ResolvedSetting {
            value: serde_json::json!(def_conn_id),
            is_default: def_conn_is_default,
        },
    );
    Ok(Json(out).into_response())
}

#[utoipa::path(
    put,
    path = "/settings/{key}",
    tag = "settings",
    params(("key" = String, Path, description = "Setting key")),
    request_body = SettingValue,
    responses(
        (status = 200, description = "Override stored", body = ResolvedSetting),
        (status = 400, description = "Invalid value", body = ErrorResponse),
        (status = 404, description = "Unknown setting", body = ErrorResponse)
    )
)]
pub async fn put_setting(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(body): Json<SettingValue>,
) -> Result<Response, AppError> {
    if !crate::settings::is_known(&key) {
        return Err(AppError::setting_not_found(&key));
    }
    let canonical = crate::settings::validate(&key, &body.value)
        .map_err(|err| AppError::invalid_request(Reason::SettingValueInvalid, err))?;
    let _guard = state.write_lock.lock().await;
    if key == crate::settings::DEFAULT_CONNECTION_ID {
        let exists = state.store().get_connection(&canonical).await?.is_some();
        if !exists {
            return Err(AppError::invalid_request(
                Reason::SettingValueInvalid,
                format!("connection '{canonical}' does not exist"),
            ));
        }
    }
    state.store().set_setting(&key, &canonical).await?;
    let value: serde_json::Value = if key == crate::settings::DEFAULT_CONNECTION_ID {
        serde_json::Value::String(canonical)
    } else {
        // canonical is the validated integer text; reflect it back as a JSON number
        canonical
            .parse::<u32>()
            .map(serde_json::Value::from)
            .unwrap_or(body.value)
    };
    Ok(Json(ResolvedSetting {
        value,
        is_default: false,
    })
    .into_response())
}

#[utoipa::path(
    delete,
    path = "/settings/{key}",
    tag = "settings",
    params(("key" = String, Path, description = "Setting key")),
    responses(
        (status = 204, description = "Reset to default"),
        (status = 404, description = "Unknown setting", body = ErrorResponse)
    )
)]
pub async fn delete_setting(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<Response, AppError> {
    if !crate::settings::is_known(&key) {
        return Err(AppError::setting_not_found(&key));
    }
    let _guard = state.write_lock.lock().await;
    // idempotent: a known setting that was never overridden is already at its default
    state.store().delete_setting(&key).await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Request body for `POST /datetime-formats/preview`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct DatetimePreviewRequest {
    pub pattern: String,
}

/// Response for `POST /datetime-formats/preview`: the pattern applied to the current local time.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct DatetimePreviewResponse {
    pub sample: String,
}

#[utoipa::path(
    post,
    path = "/datetime-formats/preview",
    tag = "settings",
    request_body = DatetimePreviewRequest,
    responses(
        (status = 200, description = "Rendered sample for the pattern", body = DatetimePreviewResponse),
        (status = 400, description = "Invalid strftime pattern", body = ErrorResponse),
    )
)]
pub async fn preview_datetime_format(
    Json(req): Json<DatetimePreviewRequest>,
) -> Result<Response, AppError> {
    crate::datetime_fmt::validate_pattern(&req.pattern)
        .map_err(|err| AppError::invalid_request(Reason::DatetimePatternInvalid, err))?;
    let sample = crate::datetime_fmt::format_now(&req.pattern, chrono::Local::now());
    Ok(Json(DatetimePreviewResponse { sample }).into_response())
}

#[utoipa::path(
    put,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    request_body = Printer,
    responses(
        (status = 200, description = "Printer replaced", body = Printer),
        (status = 400, description = "Body id does not match path id", body = ErrorResponse),
        (status = 404, description = "Printer not found", body = ErrorResponse),
        (status = 422, description = "Invalid printer", body = ErrorResponse)
    )
)]
pub async fn replace_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut printer): Json<Printer>,
) -> Result<Response, AppError> {
    if printer.id != id {
        return Err(AppError::invalid_request(
            Reason::PrinterIdMismatch,
            format!(
                "printer id in body ('{}') must match path id ('{id}')",
                printer.id
            ),
        ));
    }
    validate_printer(&printer)?;
    let _guard = state.write_lock.lock().await;
    let existing = state.store().get_printer(&id).await?;
    let Some(existing) = existing else {
        return Err(AppError::printer_not_found(id));
    };
    crate::driver::merge_secrets(&printer.kind, &mut printer.config, Some(&existing.config));
    state.store().upsert_printer(&printer).await?;
    printer.config = crate::driver::redact_config(&printer.kind, &printer.config);
    printer.is_default = existing.is_default; // replace preserves stored default; upsert ignores it
    Ok((axum::http::StatusCode::OK, Json(printer)).into_response())
}

#[utoipa::path(
    delete,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 204, description = "Printer deleted"),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn delete_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().delete_printer(&id).await? {
        Ok(axum::http::StatusCode::NO_CONTENT.into_response())
    } else {
        Err(AppError::printer_not_found(id))
    }
}

#[utoipa::path(
    post,
    path = "/printers/{id}/default",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 204, description = "Printer set as the global default"),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn set_printer_default(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().set_default_printer(&id).await? {
        Ok(axum::http::StatusCode::NO_CONTENT.into_response())
    } else {
        Err(AppError::printer_not_found(id))
    }
}

#[utoipa::path(
    delete,
    path = "/printers/{id}/default",
    params(("id" = String, Path, description = "Printer ID")),
    responses((status = 204, description = "Default flag cleared on the printer (idempotent)"))
)]
pub async fn clear_printer_default(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    state.store().clear_default_printer(&id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Request body for `POST /printers/probe`: an unsaved driver config to test-connect. Auth is out of
/// scope (#118), so no `id` and no stored-secret merge.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ProbeRequest {
    #[serde(default)]
    pub kind: Option<String>,
    pub config: serde_json::Value,
}

/// The printer's self-reported capabilities, shaped for UI feedback.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProbeCapabilities {
    pub model: Option<String>,
    pub media_width_mm: Option<f32>,
    pub resolution_dpi: Option<u32>,
    /// `"color"`, `"bilevel"`, or `"unknown"` (printer advertised no color/raster attribute).
    pub color: String,
    pub accepts_png: bool,
}

/// Result of a probe. Always returned inside a `200`; reachability is data, not the HTTP status.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeResponse {
    Ok { capabilities: ProbeCapabilities },
    Unreachable { detail: String },
}

impl ProbeResponse {
    fn ok(caps: crate::driver::PrinterCapabilities) -> Self {
        let color = if caps.bilevel {
            "bilevel"
        } else if caps.color_known {
            "color"
        } else {
            "unknown"
        };
        ProbeResponse::Ok {
            capabilities: ProbeCapabilities {
                model: caps.model,
                media_width_mm: caps.loaded_media_width_mm,
                resolution_dpi: caps.resolution_dpi,
                color: color.to_string(),
                accepts_png: caps.accepts_png,
            },
        }
    }
}

#[utoipa::path(
    post,
    path = "/printers/probe",
    request_body = ProbeRequest,
    responses(
        (status = 200, description = "Probe result (ok or unreachable)", body = ProbeResponse),
        (status = 422, description = "Invalid printer config", body = ErrorResponse)
    )
)]
pub async fn probe_printer(Json(req): Json<ProbeRequest>) -> Result<Json<ProbeResponse>, AppError> {
    let kind = req.kind.as_deref().unwrap_or("cups").to_string();
    let config = req.config;
    crate::driver::validate_config(&kind, &config)
        .map_err(|e| AppError::printer_invalid(e.to_string()))?;
    let driver = crate::driver::build_driver(&kind, &config)
        .map_err(|e| AppError::printer_invalid(e.to_string()))?;
    Ok(Json(match driver.probe().await {
        crate::driver::ProbeOutcome::Ok(c) => ProbeResponse::ok(c),
        crate::driver::ProbeOutcome::Unreachable(d) => ProbeResponse::Unreachable { detail: d },
    }))
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug, PartialEq)]
pub struct ConnectionView {
    pub id: String,
    pub connector: String,
    pub name: String,
    pub base_url: String,
    pub public_url: Option<String>,
    pub enabled: bool,
    pub has_credential: bool,
    pub transforms: Vec<crate::connector::FieldTransform>,
}

impl From<&crate::store::Connection> for ConnectionView {
    fn from(c: &crate::store::Connection) -> Self {
        Self {
            id: c.id.clone(),
            connector: c.connector.clone(),
            name: c.name.clone(),
            base_url: c.base_url.clone(),
            public_url: c.public_url.clone(),
            enabled: c.enabled,
            has_credential: !c.credential.is_empty(),
            transforms: c.transforms.clone(),
        }
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ConnectionInput {
    pub connector: String,
    pub name: String,
    pub base_url: String,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    #[schema(value_type = Option<String>)]
    pub public_url: Option<Option<String>>,
    pub credential: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    #[schema(value_type = Option<Vec<crate::connector::FieldTransform>>)]
    pub transforms: Option<Option<Vec<crate::connector::FieldTransform>>>,
}

fn deserialize_optional_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UrlField {
    Base,
    Public,
}

impl UrlField {
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Base => "base_url",
            Self::Public => "public_url",
        }
    }

    pub(crate) fn reason(self) -> Reason {
        match self {
            Self::Base => Reason::BaseUrlInvalid,
            Self::Public => Reason::PublicUrlInvalid,
        }
    }
}

pub(crate) fn validate_and_normalize_url(raw: &str, field: UrlField) -> Result<String, AppError> {
    let trimmed = raw.trim();
    let parsed = url::Url::parse(trimmed).map_err(|_| {
        AppError::invalid_request(field.reason(), format!("invalid {}", field.wire_name()))
    })?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::invalid_request(
            field.reason(),
            format!("{} must use http or https scheme", field.wire_name()),
        ));
    }
    if parsed.host().is_none() {
        return Err(AppError::invalid_request(
            field.reason(),
            format!("{} must include a host", field.wire_name()),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::invalid_request(
            field.reason(),
            format!("{} must not contain userinfo", field.wire_name()),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AppError::invalid_request(
            field.reason(),
            format!(
                "{} must not contain query parameters or fragments",
                field.wire_name()
            ),
        ));
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

#[utoipa::path(
    get,
    path = "/connections",
    responses(
        (status = 200, description = "List connections (credential redacted; only has_credential exposed)", body = [ConnectionView])
    )
)]
pub async fn list_connections(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let cs = state.store().list_connections().await?;
    Ok(Json(cs.iter().map(ConnectionView::from).collect::<Vec<_>>()).into_response())
}

#[utoipa::path(
    post,
    path = "/connections",
    request_body = ConnectionInput,
    responses(
        (status = 201, description = "Connection created (credential redacted in response)", body = ConnectionView),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn create_connection(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConnectionInput>,
) -> Result<Response, AppError> {
    let connector = state
        .connectors()
        .get(&body.connector)
        .ok_or_else(|| AppError::invalid_request(Reason::ConnectorUnknown, "unknown connector"))?;
    let transforms = match &body.transforms {
        Some(Some(t)) => t.as_slice(),
        _ => &[],
    };
    if let Err((idx, msg)) = connector.validate_transforms(transforms) {
        return Err(AppError::invalid_request(
            Reason::ConnectionTransformInvalid,
            format!("rule {idx}: {msg}"),
        ));
    }
    let cred = body.credential.unwrap_or_default();
    if cred.is_empty() {
        return Err(AppError::invalid_request(
            Reason::CredentialRequired,
            "credential required",
        ));
    }
    let base_url = validate_and_normalize_url(&body.base_url, UrlField::Base)?;
    let pub_url = match &body.public_url {
        Some(Some(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(validate_and_normalize_url(trimmed, UrlField::Public)?)
            }
        }
        _ => None,
    };
    let _g = state.write_lock.lock().await;
    let c = state
        .store()
        .create_connection(crate::store::NewConnection {
            connector: &body.connector,
            name: &body.name,
            base_url: &base_url,
            public_url: pub_url.as_deref(),
            credential: &cred,
            enabled: body.enabled,
            transforms,
        })
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ConnectionView::from(&c)),
    )
        .into_response())
}

#[utoipa::path(
    get,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Connection (credential redacted)", body = ConnectionView),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn get_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let c = state
        .store()
        .get_connection(&id)
        .await?
        .ok_or_else(|| AppError::not_found(&id))?;
    Ok(Json(ConnectionView::from(&c)).into_response())
}

#[utoipa::path(
    put,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = ConnectionInput,
    responses(
        (status = 200, description = "Connection updated (credential redacted)", body = ConnectionView),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn update_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ConnectionInput>,
) -> Result<Response, AppError> {
    let existing = state
        .store()
        .get_connection(&id)
        .await?
        .ok_or_else(|| AppError::not_found(&id))?;
    let connector = state.connectors().get(&existing.connector).ok_or_else(|| {
        AppError::invalid_request(Reason::ConnectionConnectorMissing, "unknown connector")
    })?;
    let base_url = validate_and_normalize_url(&body.base_url, UrlField::Base)?;
    let public_url = match &body.public_url {
        None => crate::store::UpdateField::Keep,
        Some(None) => crate::store::UpdateField::Clear,
        Some(Some(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                crate::store::UpdateField::Clear
            } else {
                crate::store::UpdateField::Set(validate_and_normalize_url(
                    trimmed,
                    UrlField::Public,
                )?)
            }
        }
    };
    let transforms = match &body.transforms {
        None => crate::store::UpdateField::Keep,
        Some(None) => crate::store::UpdateField::Clear,
        Some(Some(rules)) => {
            if let Err((idx, msg)) = connector.validate_transforms(rules) {
                return Err(AppError::invalid_request(
                    Reason::ConnectionTransformInvalid,
                    format!("rule {idx}: {msg}"),
                ));
            }
            if rules.is_empty() {
                crate::store::UpdateField::Clear
            } else {
                crate::store::UpdateField::Set(rules.clone())
            }
        }
    };
    let _g = state.write_lock.lock().await;
    let cred = body.credential.filter(|c| !c.is_empty());
    let ok = state
        .store()
        .update_connection(
            &id,
            crate::store::UpdateConnection {
                name: &body.name,
                base_url: &base_url,
                public_url,
                credential: cred.as_deref(),
                enabled: body.enabled,
                transforms,
            },
        )
        .await?;
    if !ok {
        return Err(AppError::not_found(&id));
    }
    let c = state.store().get_connection(&id).await?.unwrap();
    Ok(Json(ConnectionView::from(&c)).into_response())
}

#[utoipa::path(
    delete,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 204, description = "Connection deleted"),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn delete_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _g = state.write_lock.lock().await;
    if !state.store().delete_connection_and_default(&id).await? {
        return Err(AppError::not_found(&id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

async fn load_conn_and_connector<'a>(
    state: &'a AppState,
    id: &str,
) -> Result<(crate::store::Connection, &'a crate::connector::Connectors), AppError> {
    let conn = state
        .store()
        .get_connection(id)
        .await?
        .ok_or_else(|| AppError::not_found(id))?;
    let c = state.connectors().get(&conn.connector).ok_or_else(|| {
        AppError::invalid_request(Reason::ConnectionConnectorMissing, "unknown connector")
    })?;
    Ok((conn, c))
}

#[utoipa::path(
    get,
    path = "/connections/{id}/schema",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Connector schema (resources, fields, filters, relationships)", body = ConnectorSchema),
        (status = 404, description = "Connection not found", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse)
    )
)]
pub async fn connection_schema(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let schema = c
        .schema(&conn, state.egress())
        .await
        .map_err(AppError::from)?;
    Ok(Json(schema).into_response())
}

#[utoipa::path(
    post,
    path = "/connections/{id}/browse",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = BrowseRequest,
    responses(
        (status = 200, description = "A page of browse rows with an opaque cursor", body = BrowsePage),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Connection not found", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse)
    )
)]
pub async fn connection_browse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<crate::connector::BrowseRequest>,
) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let page = c
        .browse(&conn, state.egress(), state.cursor_key(), req)
        .await
        .map_err(AppError::from)?;
    Ok(Json(page).into_response())
}

#[utoipa::path(
    post,
    path = "/connections/{id}/materialize",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = MaterializeRequest,
    responses(
        (status = 200, description = "Materialized label rows", body = [LabelRow]),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Connection not found", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse)
    )
)]
pub async fn connection_materialize(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<crate::connector::MaterializeRequest>,
) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let rows = c
        .materialize(&conn, state.egress(), req)
        .await
        .map_err(AppError::from)?;
    Ok(Json(rows).into_response())
}

struct ParsedCsvRow {
    data: std::collections::HashMap<String, serde_json::Value>,
    option: std::collections::BTreeMap<String, String>,
}

fn parse_csv_rows(body: &str) -> Result<Vec<ParsedCsvRow>, AppError> {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(body.as_bytes());
    let headers = reader
        .headers()
        .map_err(|err| {
            AppError::invalid_request(
                Reason::CsvHeaderInvalid,
                format!("invalid CSV header: {err}"),
            )
        })?
        .clone();
    let mut seen = std::collections::HashSet::new();
    for header in headers.iter() {
        let header = header.trim();
        if header.is_empty() || !seen.insert(header) {
            return Err(AppError::invalid_request(
                Reason::CsvHeaderInvalid,
                "CSV header has empty or duplicate column names",
            ));
        }
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|err| {
            AppError::invalid_request(Reason::CsvRowInvalid, format!("invalid CSV row: {err}"))
        })?;
        let mut data = std::collections::HashMap::new();
        let mut option = std::collections::BTreeMap::new();
        for (key, val) in headers.iter().zip(record.iter()) {
            if let Some(name) = key.strip_prefix("option.") {
                option.insert(name.to_string(), val.to_string());
            } else {
                data.insert(key.to_string(), serde_json::Value::String(val.to_string()));
            }
        }
        rows.push(ParsedCsvRow { data, option });
    }
    if rows.is_empty() {
        return Err(AppError::invalid_request(
            Reason::CsvEmpty,
            "CSV has no data rows",
        ));
    }
    Ok(rows)
}

fn download_response(bytes: Vec<u8>, content_type: &'static str, filename: &str) -> Response {
    (
        axum::http::StatusCode::OK,
        [
            ("content-type", content_type.to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response()
}

fn parse_batch_mode(mode: &str) -> Result<crate::batch::BatchMode, AppError> {
    match mode {
        "download" => Ok(crate::batch::BatchMode::Download),
        "print" => Ok(crate::batch::BatchMode::Print),
        other => Err(AppError::invalid_request(
            Reason::ModeUnknown,
            format!("unknown mode '{other}'; use download or print"),
        )),
    }
}

/// Print/download dispatch options for `run_batch`, resolved from the request and the caller's identity.
struct BatchDispatch<'a> {
    printer: Option<&'a str>,
    format: Option<&'a str>,
    start_slot: u32,
    actor: &'a str,
}

/// Shared batch dispatch for `/batch` and `/import/csv`: validates constraints, then either renders a
/// download blob or runs the print send loop and returns a `BatchSummary`.
async fn run_batch(
    state: &Arc<AppState>,
    template: &TemplateDefinition,
    labels: &[crate::models::LabelInput],
    mode: crate::batch::BatchMode,
    dispatch: BatchDispatch<'_>,
) -> Result<Response, AppError> {
    let BatchDispatch {
        printer,
        format,
        start_slot,
        actor,
    } = dispatch;
    let is_single = matches!(
        template.format,
        crate::models::TemplateFormat::Single { .. }
    );
    if start_slot > 0 && is_single {
        return Err(AppError::invalid_request(
            Reason::StartSlotNotApplicable,
            "start_slot applies only to sheet templates",
        ));
    }
    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    match mode {
        crate::batch::BatchMode::Download => {
            let env = crate::batch::BatchEnv {
                settings: &variables,
                datetime: &dt,
                render_opts: crate::render::ImageRenderOptions::default(),
            };
            let rendered = crate::batch::render_batch(
                template,
                labels,
                mode,
                format,
                start_slot,
                &env,
                MAX_BATCH_LABELS,
            )?;
            let crate::batch::RenderedBatch::Download {
                bytes,
                content_type,
                filename,
            } = rendered
            else {
                return Err(AppError::internal(
                    "batch returned non-download for download mode",
                ));
            };
            Ok(download_response(bytes, content_type, &filename))
        }
        crate::batch::BatchMode::Print => {
            if format.is_some() {
                return Err(AppError::invalid_request(
                    Reason::FormatNotApplicable,
                    "format applies only to download; omit it when printing",
                ));
            }
            let printer_id = printer.ok_or_else(|| {
                AppError::invalid_request(Reason::PrinterRequired, "mode=print requires a printer")
            })?;
            let printer = state
                .store()
                .get_printer(printer_id)
                .await?
                .ok_or_else(|| AppError::printer_not_found(printer_id.to_string()))?;
            let driver = crate::driver::build_driver(&printer.kind, &printer.config)
                .map_err(|err| AppError::printer_invalid(err.to_string()))?;
            let ovr = driver.configured_render_override();
            let template_media_width = match &template.format {
                crate::models::TemplateFormat::Single { media_width, .. } => *media_width,
                _ => None,
            };
            // Fetch caps when any override field is unset (needs negotiation) or a media check is pending.
            let need_caps = ovr.color_mode.is_none()
                || ovr.resolution_dpi.is_none()
                || template_media_width.is_some();
            let caps = if need_caps {
                driver.capabilities().await
            } else {
                None
            };
            // media preflight gate (fail-open): reject ONLY on a confident mismatch.
            if let (Some(mw), Some(got)) = (
                template_media_width,
                caps.as_ref().and_then(|c| c.loaded_media_width_mm),
            ) {
                let want_mm = if template.unit == "in" { mw * 25.4 } else { mw };
                if (want_mm - got).abs() > 1.0 {
                    return Err(AppError::media_mismatch(want_mm, got));
                }
            }
            let render_opts = crate::driver::effective_render(&ovr, caps.as_ref());
            let artifact_format =
                crate::driver::print_artifact_format(render_opts.color_mode, is_single);
            let render_format = match artifact_format {
                crate::driver::ArtifactFormat::Png => "png",
                _ => "pdf",
            };
            let env = crate::batch::BatchEnv {
                settings: &variables,
                datetime: &dt,
                render_opts,
            };
            // Validate-then-execute: render everything first; bad data => 422 before any send.
            let rendered = crate::batch::render_batch(
                template,
                labels,
                mode,
                Some(render_format),
                start_slot,
                &env,
                MAX_BATCH_LABELS,
            )?;
            let crate::batch::RenderedBatch::Print { units } = rendered else {
                return Err(AppError::internal(
                    "batch returned non-print for print mode",
                ));
            };
            let total = labels.len();
            let jobs = units.len();
            let mut failed = Vec::new();
            for unit in &units {
                match driver
                    .send(
                        &unit.bytes,
                        &crate::driver::PrintOptions {
                            copies: 1,
                            artifact_format,
                        },
                    )
                    .await
                {
                    Ok(()) => {
                        let _ = state
                            .store()
                            .record_job(&template.id, Some(printer_id), "ok", None, actor)
                            .await;
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        let _ = state
                            .store()
                            .record_job(&template.id, Some(printer_id), "failed", Some(&msg), actor)
                            .await;
                        for &i in &unit.indices {
                            failed.push(BatchRowError {
                                index: i,
                                error: msg.clone(),
                            });
                        }
                    }
                }
            }
            let summary = BatchSummary {
                total,
                succeeded: total - failed.len(),
                failed,
                jobs,
            };
            Ok((axum::http::StatusCode::OK, Json(summary)).into_response())
        }
    }
}

#[utoipa::path(
    post,
    path = "/batch",
    request_body = BatchRequest,
    responses(
        (status = 200, description = "Download blob (zip/pdf) or print summary"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 409, description = "Media mismatch", body = ErrorResponse),
        (status = 413, description = "Batch too large", body = ErrorResponse),
        (status = 422, description = "One or more labels invalid", body = ErrorResponse),
        (status = 502, description = "Printer transport failure", body = ErrorResponse)
    )
)]
pub async fn batch(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Json(req): Json<BatchRequest>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;
    let mode = parse_batch_mode(&req.mode)?;
    run_batch(
        &state,
        template,
        &req.labels,
        mode,
        BatchDispatch {
            printer: req.printer.as_deref(),
            format: req.format.as_deref(),
            start_slot: req.start_slot,
            actor: &principal.actor_id(),
        },
    )
    .await
}

#[utoipa::path(
    post,
    path = "/print",
    request_body = PrintRequest,
    responses(
        (status = 200, description = "Print summary", body = BatchSummary),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 409, description = "Media mismatch", body = ErrorResponse),
        (status = 413, description = "Request body too large", body = ErrorResponse),
        (status = 502, description = "Printer transport failure", body = ErrorResponse)
    )
)]
pub async fn print_label(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Json(req): Json<PrintRequest>,
) -> Result<Response, AppError> {
    if !(1..=MAX_PRINT_COPIES).contains(&req.copies) {
        return Err(AppError::invalid_request(
            Reason::CopiesInvalid,
            format!("copies must be between 1 and {MAX_PRINT_COPIES}"),
        ));
    }
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;
    let label_data = req.data.or(req.fields).unwrap_or_default();
    let label = crate::models::LabelInput { data: label_data };
    let labels = vec![label; req.copies as usize];
    run_batch(
        &state,
        template,
        &labels,
        crate::batch::BatchMode::Print,
        BatchDispatch {
            printer: Some(&req.printer),
            format: None,
            start_slot: 0,
            actor: &principal.actor_id(),
        },
    )
    .await
}

#[utoipa::path(
    post,
    path = "/render/label",
    params(
        ("format" = Option<String>, Query, description = "Output format: png (default) or pdf"),
        ("color_mode" = Option<String>, Query, description = "Color mode for PNG: color (default) or bilevel"),
        ("resolution" = Option<String>, Query, description = "PNG raster DPI override (1-1200); defaults to template dpi")
    ),
    request_body = RenderLabelRequest,
    responses(
        (status = 200, description = "Rendered PNG bytes", content_type = "image/png", body = Vec<u8>),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 415, description = "Unsupported media type", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse),
        (status = 501, description = "Not implemented", body = ErrorResponse)
    )
)]
pub async fn render_label(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RenderQuery>,
    Json(req): Json<RenderLabelRequest>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;

    tracing::debug!(
        template = %template.id,
        dpi = template.dpi,
        data_keys = req.label.data.len(),
        "render label request"
    );

    let color_mode = match query.color_mode.as_deref() {
        None | Some("") | Some("color") => ColorMode::Color,
        Some("bilevel") => ColorMode::BiLevel,
        Some(other) => {
            return Err(AppError::invalid_request(
                Reason::ColorModeUnknown,
                format!("unknown color_mode '{other}'; use color or bilevel"),
            ))
        }
    };
    let resolution_dpi = match query.resolution.as_deref() {
        None | Some("") => None,
        Some(s) => {
            let dpi: u32 = s.parse().map_err(|_| {
                AppError::invalid_request(
                    Reason::ResolutionInvalid,
                    format!("resolution must be a positive integer, got '{s}'"),
                )
            })?;
            if dpi == 0 || dpi > crate::render::MAX_RENDER_DPI {
                return Err(AppError::invalid_request(
                    Reason::ResolutionInvalid,
                    format!(
                        "resolution must be between 1 and {}",
                        crate::render::MAX_RENDER_DPI
                    ),
                ));
            }
            Some(dpi)
        }
    };
    let img_opts = ImageRenderOptions {
        color_mode,
        resolution_dpi,
    };

    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let (bytes, content_type) = match query.format.as_deref() {
        None | Some("") | Some("png") => (
            render_single_label_image(template, &req.label.data, None, &variables, &dt, img_opts)?,
            "image/png",
        ),
        Some("pdf") => {
            if color_mode == ColorMode::BiLevel {
                return Err(AppError::invalid_request(
                    Reason::BilevelRequiresPng,
                    "bilevel is only supported for png output",
                ));
            }
            (
                render_single_label_pdf(template, &req.label.data, None, &variables, &dt)?,
                "application/pdf",
            )
        }
        Some(other) => {
            return Err(AppError::invalid_request(
                Reason::FormatUnknown,
                format!("unknown format '{other}'; use png or pdf"),
            ))
        }
    };

    Ok((
        axum::http::StatusCode::OK,
        [("content-type", content_type)],
        bytes,
    )
        .into_response())
}

#[utoipa::path(
    post,
    path = "/import/csv",
    params(
        ("template" = String, Query, description = "Template id"),
        ("mode" = Option<String>, Query, description = "download (default) or print"),
        ("printer" = Option<String>, Query, description = "Printer id (required when mode=print)"),
        ("format" = Option<String>, Query, description = "Download format: png (default) or pdf")
    ),
    request_body(content = String, description = "CSV (header row + one row per label)", content_type = "text/csv"),
    responses(
        (status = 200, description = "Download blob (zip/pdf) or print summary (BatchSummary)"),
        (status = 400, description = "Invalid CSV or request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 413, description = "Batch too large", body = ErrorResponse),
        (status = 422, description = "One or more rows invalid (batch is atomic)", body = ErrorResponse),
        (status = 502, description = "Printer/transport failure", body = ErrorResponse)
    )
)]
pub async fn import_csv(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Query(params): Query<ImportCsvQuery>,
    body: String,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&params.template)
        .ok_or_else(|| AppError::template_not_found(params.template.clone()))?;
    let mode = parse_batch_mode(params.mode.as_deref().unwrap_or("download"))?;
    let parsed_rows = parse_csv_rows(&body)?;
    // Per SPEC section E, an unknown option.<name> column is an error, not silently ignored.
    for row in &parsed_rows {
        for name in row.option.keys() {
            if !template.params.contains_key(name) {
                return Err(AppError::invalid_request(
                    Reason::CsvOptionColumnUnknown,
                    format!(
                        "CSV column 'option.{name}' is not a declared option of template '{}'",
                        template.id
                    ),
                ));
            }
        }
    }
    let labels: Vec<crate::models::LabelInput> = parsed_rows
        .into_iter()
        .map(|mut row| {
            for (name, v) in row.option {
                if !v.is_empty() {
                    row.data.insert(name, serde_json::Value::String(v));
                }
            }
            crate::models::LabelInput { data: row.data }
        })
        .collect();
    run_batch(
        &state,
        template,
        &labels,
        mode,
        BatchDispatch {
            printer: params.printer.as_deref(),
            format: params.format.as_deref(),
            start_slot: 0,
            actor: &principal.actor_id(),
        },
    )
    .await
}

#[utoipa::path(get, path = "/favorites", tag = "favorites",
    responses((status = 200, description = "Favorited template ids", body = Vec<String>)))]
pub async fn list_favorites(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
) -> Result<Json<Vec<String>>, AppError> {
    let ids = state.store().list_favorites(&principal.actor_id()).await?;
    let registry = state.templates.load_full();
    Ok(Json(
        ids.into_iter()
            .filter(|id| registry.get(id).is_some())
            .collect(),
    ))
}

#[utoipa::path(put, path = "/favorites/{template_id}", tag = "favorites",
    params(("template_id" = String, Path, description = "Template ID")),
    responses((status = 204, description = "Favorited"), (status = 404, description = "Unknown template", body = ErrorResponse)))]
pub async fn add_favorite(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Path(template_id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    // Under the lock, not before it: an in-flight delete would otherwise prune between this check
    // and the insert below, leaving exactly the stale row the prune exists to remove (#140).
    if state.templates.load_full().get(&template_id).is_none() {
        return Err(AppError::template_not_found(template_id));
    }
    state
        .store()
        .add_favorite(&principal.actor_id(), &template_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(delete, path = "/favorites/{template_id}", tag = "favorites",
    params(("template_id" = String, Path, description = "Template ID")),
    responses((status = 204, description = "Unfavorited (idempotent)")))]
pub async fn remove_favorite(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Path(template_id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    state
        .store()
        .remove_favorite(&principal.actor_id(), &template_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Deserialize)]
pub struct RecentQuery {
    pub limit: Option<u32>,
}

#[utoipa::path(get, path = "/recent-templates", tag = "favorites",
    params(("limit" = Option<u32>, Query, description = "Max results (default 6, cap 20)")),
    responses((status = 200, description = "Recently printed template ids", body = Vec<String>)))]
pub async fn recent_templates(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Query(q): Query<RecentQuery>,
) -> Result<Json<Vec<String>>, AppError> {
    let limit = q.limit.unwrap_or(6).clamp(1, 20);
    let ids = state
        .store()
        .recent_templates(&principal.actor_id(), limit)
        .await?;
    let registry = state.templates.load_full();
    Ok(Json(
        ids.into_iter()
            .filter(|id| registry.get(id).is_some())
            .collect(),
    ))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Validate a new account/password: non-empty username, non-empty password. Returns 400 otherwise
/// (an empty password is a footgun; run with LABELER_NO_AUTH instead of an empty-password account).
fn validate_new_account(username: &str, password: &str) -> Result<(), AppError> {
    if username.trim().is_empty() {
        return Err(AppError::invalid_request(
            Reason::UsernameEmpty,
            "username must not be empty",
        ));
    }
    validate_password(password)
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.is_empty() {
        return Err(AppError::invalid_request(
            Reason::PasswordEmpty,
            "password must not be empty",
        ));
    }
    Ok(())
}

/// Authentication state for the SPA, returned by `GET /auth/me`.
/// This type is the OpenAPI schema only; the `me` handler constructs the JSON response directly with `serde_json::json!`, so changes here must be mirrored in the handler.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct AuthStatus {
    pub authed: bool,
    #[serde(rename = "needsSetup")]
    pub needs_setup: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<UserSummary>,
    #[serde(rename = "noAuth", skip_serializing_if = "std::ops::Not::not")]
    pub no_auth: bool,
}

/// A user as exposed by the API (never includes the password hash).
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct UserSummary {
    pub id: String,
    pub username: String,
}

/// An API token's public metadata (the secret is only ever returned once, at creation).
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TokenSummary {
    pub id: String,
    pub name: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

/// The one-time response to `POST /tokens`, carrying the plaintext secret.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TokenCreated {
    pub id: String,
    pub name: String,
    pub secret: String,
}

/// A trivial `{ "ok": true }` acknowledgement.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct OkResponse {
    pub ok: bool,
}

#[utoipa::path(
    post,
    path = "/auth/setup",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 200, description = "First user created and logged in", body = OkResponse),
        (status = 409, description = "Setup already completed", body = ErrorResponse)
    )
)]
pub async fn setup(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    req_https: HttpsHint,
    Json(body): Json<Credentials>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().count_users().await.map_err(AppError::from)? > 0 {
        return Err(AppError::conflict("setup already completed"));
    }
    validate_new_account(&body.username, &body.password)?;
    let hash = crate::auth::hash_password(&body.password)
        .map_err(|_| AppError::internal("hash failed"))?;
    let user = state
        .store()
        .create_user(&body.username, &hash)
        .await
        .map_err(AppError::from)?;
    start_session(&state, jar, &user.id, req_https.0).await
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 200, description = "Logged in; sets a session cookie", body = OkResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse),
        (status = 403, description = "Cross-origin request rejected", body = ErrorResponse)
    )
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    req_https: HttpsHint,
    Json(body): Json<Credentials>,
) -> Result<Response, AppError> {
    match state
        .store()
        .get_user_by_username(&body.username)
        .await
        .map_err(AppError::from)?
    {
        Some(user) if crate::auth::verify_password(&body.password, &user.password_hash) => {
            start_session(&state, jar, &user.id, req_https.0).await
        }
        Some(_) => Err(AppError::unauthorized()),
        None => {
            crate::auth::dummy_verify(&body.password);
            Err(AppError::unauthorized())
        }
    }
}

async fn start_session(
    state: &AppState,
    jar: CookieJar,
    user_id: &str,
    https: bool,
) -> Result<Response, AppError> {
    // Rotate: invalidate any session the incoming cookie referenced (session-fixation defense).
    if let Some(old) = jar.get(crate::middleware::SESSION_COOKIE) {
        let _ = state
            .store()
            .delete_session(&crate::auth::sha256_hex(old.value()))
            .await;
    }
    let secret = crate::auth::random_secret();
    state
        .store()
        .create_session(&crate::auth::sha256_hex(&secret), user_id, "+30 days")
        .await
        .map_err(AppError::from)?;
    let jar = jar.add(crate::middleware::session_cookie(secret, https));
    Ok((jar, Json(serde_json::json!({"ok": true}))).into_response())
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    responses(
        (status = 200, description = "Session cleared", body = OkResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Cross-origin request rejected", body = ErrorResponse)
    )
)]
pub async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(crate::middleware::SESSION_COOKIE) {
        let _ = state
            .store()
            .delete_session(&crate::auth::sha256_hex(c.value()))
            .await;
    }
    (
        jar.add(crate::middleware::clear_cookie()),
        Json(serde_json::json!({"ok": true})),
    )
        .into_response()
}

/// Mark a response uncacheable so the browser and any reverse proxy never serve a stale auth state
/// (a cached `/auth/me` would strand the SPA on `/setup` after first-account setup; see #103).
fn no_store(mut resp: Response) -> Response {
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

// `/auth/me` is AUTH-EXEMPT (it must answer for logged-OUT callers too), so it resolves auth itself
// (optional) and always returns 200 with the auth state the SPA needs.
#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current auth state (authed flag, needsSetup, optional user)", body = AuthStatus)
    )
)]
pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    if state.no_auth() {
        return Ok(no_store(
            Json(serde_json::json!({
                "authed": true,
                "needsSetup": false,
                "me": {"id": "local", "username": "local"},
                "noAuth": true
            }))
            .into_response(),
        ));
    }
    if let Some(p) = crate::middleware::resolve_optional(&state, &headers).await {
        let me = match p {
            crate::middleware::Principal::User { id, username } => {
                serde_json::json!({"id": id, "username": username})
            }
            crate::middleware::Principal::Token { .. } => {
                serde_json::json!({"id": "token", "username": "api-token"})
            }
            // resolve_optional never returns Local, but the match must be exhaustive.
            crate::middleware::Principal::Local => {
                serde_json::json!({"id": "local", "username": "local"})
            }
        };
        return Ok(no_store(
            Json(serde_json::json!({"authed": true, "needsSetup": false, "me": me}))
                .into_response(),
        ));
    }
    let needs_setup = state.store().count_users().await.map_err(AppError::from)? == 0;
    Ok(no_store(
        Json(serde_json::json!({"authed": false, "needsSetup": needs_setup})).into_response(),
    ))
}

#[utoipa::path(
    get,
    path = "/users",
    tag = "auth",
    responses(
        (status = 200, description = "List users", body = [UserSummary]),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn list_users(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let users = state.store().list_users().await.map_err(AppError::from)?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| serde_json::json!({"id": u.id, "username": u.username}))
            .collect::<Vec<_>>(),
    )
    .into_response())
}

#[utoipa::path(
    post,
    path = "/users",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 201, description = "User created", body = UserSummary),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 409, description = "Username already exists", body = ErrorResponse)
    )
)]
pub async fn create_user_h(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Credentials>,
) -> Result<Response, AppError> {
    validate_new_account(&body.username, &body.password)?;
    let _guard = state.write_lock.lock().await;
    // The write-lock serializes writers, so a check-then-insert is race-free here and yields a clean 409
    // instead of a 500 from the UNIQUE constraint.
    if state
        .store()
        .get_user_by_username(&body.username)
        .await
        .map_err(AppError::from)?
        .is_some()
    {
        return Err(AppError::conflict("username already exists"));
    }
    let hash = crate::auth::hash_password(&body.password)
        .map_err(|_| AppError::internal("hash failed"))?;
    let u = state
        .store()
        .create_user(&body.username, &hash)
        .await
        .map_err(AppError::from)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"id": u.id, "username": u.username})),
    )
        .into_response())
}

#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "auth",
    params(("id" = String, Path, description = "User ID")),
    responses(
        (status = 204, description = "User deleted"),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
        (status = 409, description = "Cannot delete the last user or your own account", body = ErrorResponse)
    )
)]
pub async fn delete_user_h(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().count_users().await.map_err(AppError::from)? <= 1 {
        return Err(AppError::conflict("cannot delete the last user"));
    }
    // Deleting your own account cascades your session (FK ON DELETE CASCADE), silently logging you out;
    // block it so the action is refused with a clear message rather than bouncing the caller to login.
    if let crate::middleware::Principal::User { id: me, .. } = &p {
        if me == &id {
            return Err(AppError::conflict("cannot delete your own account"));
        }
    }
    if !state
        .store()
        .delete_user(&id)
        .await
        .map_err(AppError::from)?
    {
        return Err(AppError::not_found(&id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct PasswordChange {
    pub current_password: String,
    pub new_password: String,
}

#[utoipa::path(
    post,
    path = "/auth/password",
    tag = "auth",
    request_body = PasswordChange,
    responses(
        (status = 200, description = "Password changed; other sessions revoked", body = OkResponse),
        (status = 401, description = "Current password incorrect or not authenticated", body = ErrorResponse),
        (status = 403, description = "An API token cannot change a password", body = ErrorResponse)
    )
)]
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Json(body): Json<PasswordChange>,
) -> Result<Response, AppError> {
    let crate::middleware::Principal::User { id, .. } = p else {
        return Err(AppError::forbidden("token cannot change a password"));
    };
    let user = state
        .store()
        .get_user_by_id(&id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::unauthorized)?;
    if !crate::auth::verify_password(&body.current_password, &user.password_hash) {
        return Err(AppError::unauthorized());
    }
    validate_password(&body.new_password)?;
    let _guard = state.write_lock.lock().await;
    let hash = crate::auth::hash_password(&body.new_password)
        .map_err(|_| AppError::internal("hash failed"))?;
    state
        .store()
        .set_user_password(&id, &hash)
        .await
        .map_err(AppError::from)?;
    let keep = jar
        .get(crate::middleware::SESSION_COOKIE)
        .map(|c| crate::auth::sha256_hex(c.value()))
        .unwrap_or_default();
    state
        .store()
        .delete_user_sessions_except(&id, &keep)
        .await
        .map_err(AppError::from)?;
    Ok(Json(serde_json::json!({"ok": true})).into_response())
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct TokenCreate {
    pub name: String,
}

#[utoipa::path(
    get,
    path = "/tokens",
    tag = "auth",
    responses(
        (status = 200, description = "List API tokens (never the secret)", body = [TokenSummary]),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn list_tokens(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let t = state.store().list_tokens().await.map_err(AppError::from)?;
    Ok(Json(
        t.into_iter()
            .map(|t| {
                serde_json::json!({"id": t.id, "name": t.name, "last_used_at": t.last_used_at, "created_at": t.created_at})
            })
            .collect::<Vec<_>>(),
    )
    .into_response())
}

#[utoipa::path(
    post,
    path = "/tokens",
    tag = "auth",
    request_body = TokenCreate,
    responses(
        (status = 201, description = "Token created; secret returned once", body = TokenCreated),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn create_token_h(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TokenCreate>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    let secret = format!("lbl_{}", crate::auth::random_secret());
    let t = state
        .store()
        .create_token(&body.name, &crate::auth::sha256_hex(&secret))
        .await
        .map_err(AppError::from)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"id": t.id, "name": t.name, "secret": secret})),
    )
        .into_response())
}

#[utoipa::path(
    delete,
    path = "/tokens/{id}",
    tag = "auth",
    params(("id" = String, Path, description = "Token ID")),
    responses(
        (status = 204, description = "Token revoked"),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "Token not found", body = ErrorResponse)
    )
)]
pub async fn delete_token_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if !state
        .store()
        .delete_token(&id)
        .await
        .map_err(AppError::from)?
    {
        return Err(AppError::not_found(&id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

// Effective-https extractor for the cookie Secure flag (proxy-aware), used by setup/login.
pub struct HttpsHint(pub bool);
impl FromRequestParts<Arc<AppState>> for HttpsHint {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(HttpsHint(crate::middleware::effective_https(
            &parts.headers,
            &parts.uri,
            state.trust_proxy(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn confirm_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("labeler_confirm_{label}_{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn confirm_yaml(name: &str) -> String {
        format!(
            "name: {name}\ndescription: d\nunit: mm\ndpi: 300\nformat:\n  type: single\n  width: 20.0\n  height: 10.0\nlayout:\n  - type: text\n    value: hi\n    at: [0.0, 0.0]\n    size: [20.0, 5.0]\n    font_size: 3.0\n"
        )
    }

    /// The confirmation is what stands between a write and a response describing somebody else's
    /// file. Its three outcomes are the contract: pass, collision, or lost write (#183, #184).
    ///
    /// These are unit tests because the arms need the directory to change *between* a handler's
    /// write and its reload, which no request can stage on its own: everything reachable from
    /// outside stops earlier, at the pre-write re-read. The classification is decided here; the
    /// handlers' wiring is held by the HTTP tests that drive a real post-write collision through the
    /// `cfg(test)` mid-write hook, so a handler that stops confirming fails a test.
    #[test]
    fn confirm_written_template_passes_when_the_id_is_served_from_our_file() {
        let dir = confirm_dir("pass");
        let body = confirm_yaml("mine");
        let path = dir.join("t.yaml");
        std::fs::write(&path, &body).unwrap();
        let registry = TemplateRegistry::load_from_dir(&dir).expect("load");

        assert!(confirm_written_template(&registry, "t", &path, &body).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    async fn error_response(err: AppError) -> (axum::http::StatusCode, serde_json::Value) {
        use axum::response::IntoResponse;
        use http_body_util::BodyExt;
        let response = err.into_response();
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        (status, serde_json::from_slice(&bytes).expect("json"))
    }

    #[tokio::test]
    async fn confirm_written_template_reports_a_collision_naming_every_claimant() {
        let dir = confirm_dir("collide");
        let body = confirm_yaml("mine");
        // Ours is written, but a file sorting earlier claims the id, so the load refuses ours. A
        // third claimant is present too: an operator told about only two of three would fix one file
        // and still not converge.
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("m")).unwrap();
        std::fs::create_dir_all(dir.join("z")).unwrap();
        let ours = dir.join("z").join("t.yaml");
        std::fs::write(&ours, &body).unwrap();
        std::fs::write(dir.join("a").join("t.yaml"), confirm_yaml("theirs")).unwrap();
        std::fs::write(dir.join("m").join("t.yaml"), confirm_yaml("third")).unwrap();
        let registry = TemplateRegistry::load_from_dir(&dir).expect("load");

        let err = confirm_written_template(&registry, "t", &ours, &body)
            .expect_err("the id is served from another file");
        let (status, value) = error_response(err).await;
        assert_eq!(status, axum::http::StatusCode::CONFLICT);
        assert_eq!(value["error"]["code"], "TemplateIdCollision");
        assert_eq!(value["error"]["details"]["template"], "t");
        let mut files: Vec<&str> = value["error"]["details"]["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        files.sort_unstable();
        assert_eq!(
            files,
            vec!["a/t.yaml", "m/t.yaml", "z/t.yaml"],
            "every file declaring the id, and nothing else"
        );
        assert!(
            !value["error"]["details"]
                .as_object()
                .expect("details object")
                .contains_key("reason"),
            "a 409 carries no details.reason key at all (ADR-0052)"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file the reading never refused is not a collider, however it looks by the time the error is
    /// built. Without the `duplicates` check the live read below would see our own bytes at our own
    /// path and report a `409` naming a file the snapshot never listed as claiming the id.
    #[tokio::test]
    async fn confirm_written_template_requires_the_snapshot_to_have_refused_our_file() {
        let dir = confirm_dir("not_refused");
        let body = confirm_yaml("mine");
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("z")).unwrap();
        // The reading happens while only the winner exists...
        std::fs::write(dir.join("a").join("t.yaml"), confirm_yaml("theirs")).unwrap();
        let registry = TemplateRegistry::load_from_dir(&dir).expect("load");
        // ...and our file appears afterwards, so it is on disk with our bytes but was never refused.
        let ours = dir.join("z").join("t.yaml");
        std::fs::write(&ours, &body).unwrap();

        let err = confirm_written_template(&registry, "t", &ours, &body)
            .expect_err("the reading did not refuse our file");
        assert_eq!(err.reason(), Some("template_missing_after_write"));
        let (status, _) = error_response(err).await;
        assert_eq!(
            status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "not a collision: the snapshot never listed our file as claiming the id"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An external rename leaves the id served by a path we did not write while our own path is
    /// gone. There is no colliding file to name and no intact copy of the write, so this is the lost
    /// write, not a collision (round-4 review).
    #[tokio::test]
    async fn confirm_written_template_reports_a_renamed_file_as_a_lost_write() {
        let dir = confirm_dir("renamed");
        let body = confirm_yaml("mine");
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("z")).unwrap();
        std::fs::write(dir.join("a").join("t.yaml"), &body).unwrap();
        let ours = dir.join("z").join("t.yaml"); // the name we wrote, since renamed away
        let registry = TemplateRegistry::load_from_dir(&dir).expect("load");

        let err =
            confirm_written_template(&registry, "t", &ours, &body).expect_err("our file is gone");
        assert_eq!(err.reason(), Some("template_missing_after_write"));
        let (status, value) = error_response(err).await;
        assert_eq!(
            status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "a vanished write is a 500, not a 409"
        );
        assert_eq!(value["error"]["code"], "RenderFailed");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Our filename survives but holds another writer's bytes: the path comparison alone would pass
    /// and the handler would present their content as the caller's (round-4 review).
    #[tokio::test]
    async fn confirm_written_template_reports_replaced_content_as_a_lost_write() {
        let dir = confirm_dir("replaced");
        let path = dir.join("t.yaml");
        std::fs::write(&path, confirm_yaml("theirs")).unwrap();
        let registry = TemplateRegistry::load_from_dir(&dir).expect("load");

        let err = confirm_written_template(&registry, "t", &path, &confirm_yaml("mine"))
            .expect_err("the file no longer holds what we wrote");
        assert_eq!(err.reason(), Some("template_missing_after_write"));
        let (status, _) = error_response(err).await;
        assert_eq!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_and_normalize_url_accepts_valid_urls() {
        assert_eq!(
            validate_and_normalize_url("http://example.com", UrlField::Base).unwrap(),
            "http://example.com"
        );
        assert_eq!(
            validate_and_normalize_url("https://example.com/", UrlField::Base).unwrap(),
            "https://example.com"
        );
        assert_eq!(
            validate_and_normalize_url("  https://example.com/sub/path///  ", UrlField::Public)
                .unwrap(),
            "https://example.com/sub/path"
        );
        assert_eq!(
            validate_and_normalize_url("http://hb.lan:7745", UrlField::Base).unwrap(),
            "http://hb.lan:7745"
        );
    }

    #[test]
    fn validate_and_normalize_url_rejects_invalid_urls() {
        // Bad scheme
        let err = validate_and_normalize_url("ftp://example.com", UrlField::Public).unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert!(err.message_text().contains("must use http or https scheme"));

        // Missing host
        let err = validate_and_normalize_url("http://", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));

        // Userinfo with password
        let err = validate_and_normalize_url("https://user:pass@example.com", UrlField::Public)
            .unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert!(err.message_text().contains("must not contain userinfo"));

        // Userinfo without password (username only)
        let err =
            validate_and_normalize_url("https://user@example.com", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));
        assert!(err.message_text().contains("must not contain userinfo"));

        // Query parameters
        let err =
            validate_and_normalize_url("http://example.com?query=1", UrlField::Public).unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert!(err
            .message_text()
            .contains("must not contain query parameters or fragments"));

        // Fragments
        let err =
            validate_and_normalize_url("http://example.com#section", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));
        assert!(err
            .message_text()
            .contains("must not contain query parameters or fragments"));

        // Parse failure
        let err = validate_and_normalize_url("not a url", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));
        assert_eq!(err.message_text(), "invalid base_url");

        let err = validate_and_normalize_url("not a url", UrlField::Public).unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert_eq!(err.message_text(), "invalid public_url");
    }

    #[test]
    fn connection_input_deserialization_public_url_tri_state() {
        // Omitted -> None
        let json = r#"{"connector":"homebox","name":"test","base_url":"http://hb.lan"}"#;
        let input: ConnectionInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.public_url, None);

        // Null -> Some(None)
        let json =
            r#"{"connector":"homebox","name":"test","base_url":"http://hb.lan","public_url":null}"#;
        let input: ConnectionInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.public_url, Some(None));

        // Empty string -> Some(Some(""))
        let json =
            r#"{"connector":"homebox","name":"test","base_url":"http://hb.lan","public_url":""}"#;
        let input: ConnectionInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.public_url, Some(Some(String::new())));

        // String value -> Some(Some("https://example.com"))
        let json = r#"{"connector":"homebox","name":"test","base_url":"http://hb.lan","public_url":"https://example.com"}"#;
        let input: ConnectionInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.public_url, Some(Some("https://example.com".into())));
    }
    #[test]
    fn connection_view_from_connection() {
        let conn = crate::store::Connection {
            id: "conn-1".into(),
            connector: "homebox".into(),
            name: "Homebox".into(),
            base_url: "http://hb.lan:7745".into(),
            public_url: Some("https://homebox.example.com".into()),
            credential: "secret".into(),
            enabled: true,
            transforms: vec![],
        };
        let view = ConnectionView::from(&conn);
        assert_eq!(
            view,
            ConnectionView {
                id: "conn-1".into(),
                connector: "homebox".into(),
                name: "Homebox".into(),
                base_url: "http://hb.lan:7745".into(),
                public_url: Some("https://homebox.example.com".into()),
                enabled: true,
                has_credential: true,
                transforms: vec![],
            }
        );

        let conn_no_cred = crate::store::Connection {
            id: "conn-2".into(),
            connector: "homebox".into(),
            name: "Homebox".into(),
            base_url: "http://hb.lan:7745".into(),
            public_url: None,
            credential: "".into(),
            enabled: false,
            transforms: vec![],
        };
        let view2 = ConnectionView::from(&conn_no_cred);
        assert!(!view2.has_credential);
        assert_eq!(view2.public_url, None);
        assert!(!view2.enabled);
        assert!(view2.transforms.is_empty());
    }
}
