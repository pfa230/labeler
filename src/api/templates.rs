//! Template routes: list, CRUD, source, thumbnail and reload, plus per-user favorites and recents.

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use rustix::fd::AsFd;
use sha2::Digest;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    errors::{AppError, NotFoundKind},
    extract::{Json, Path},
    fs_safe::{self, PublishResult},
    models::{ErrorResponse, ReloadResponse, TemplateDetail, TemplateList},
    parse::parse_template,
    reason::Reason,
    templates::{validate_template_id_stem, TemplateContent, TemplateDefinition},
};

use super::AppState;

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/templates", get(list_templates))
        .route("/template-registry/reload", post(reload_templates))
        .route(
            "/templates/{id}",
            get(get_template)
                .post(create_template)
                .put(replace_template)
                .delete(delete_template),
        )
        .route("/templates/{id}/source", get(template_source))
        .route("/templates/{id}/thumbnail", get(thumbnail))
        .route("/favorites", get(list_favorites))
        .route(
            "/favorites/{template_id}",
            put(add_favorite).delete(remove_favorite),
        )
        .route("/recent-templates", get(recent_templates))
}

#[utoipa::path(
    get,
    path = "/templates",
    tag = "api",
    responses(
        (status = 200, description = "List templates", body = TemplateList)
    )
)]
pub async fn list_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TemplateList>, AppError> {
    let context = detail_context(&state)?;
    let dt_resolver = crate::datetime_fmt::DateTimeResolver {
        formats: &context.dt_formats,
        now: chrono::Local::now(),
    };
    let registry = state.templates.load_full();
    let templates = registry.summaries(&context.variables, &dt_resolver);
    let broken = registry
        .broken()
        .iter()
        .map(|b| crate::models::BrokenTemplateSummary {
            path: b.path.clone(),
            error: b.error.clone(),
        })
        .collect();
    Ok(Json(TemplateList { templates, broken }))
}

#[utoipa::path(
    post,
    path = "/template-registry/reload",
    tag = "api",
    responses(
        (status = 200, description = "Templates reloaded from disk", body = ReloadResponse),
        (status = 500, description = "Failed to read the templates directory", body = ErrorResponse)
    )
)]
pub async fn reload_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadResponse>, AppError> {
    // Under the same lock the write endpoints hold, so it cannot interleave with a write and the
    // reload that follows it.
    let _guard = state.write_lock.lock().await;
    let (count, broken_count) = state.reload()?;
    Ok(Json(ReloadResponse {
        count,
        broken_count,
    }))
}

fn check_template_id(id: &str) -> Result<(), AppError> {
    if validate_template_id_stem(id) {
        Ok(())
    } else {
        Err(AppError::invalid_request(
            Reason::TemplateIdInvalid,
            format!("template id '{id}' must be non-empty and match ^[a-zA-Z0-9_-]+$"),
        ))
    }
}

fn parse_and_validate(body: &str) -> Result<TemplateContent, AppError> {
    let content = parse_template(body).map_err(|err| {
        AppError::template_invalid(Reason::TemplateValidationFailed, err.to_string())
    })?;
    content
        .validate()
        .map_err(|err| AppError::template_invalid(Reason::TemplateValidationFailed, err))?;
    Ok(content)
}

/// The variables and datetime formats a response's published `params` are resolved against. A
/// write reads them before writing, so a store failure answers `500` with nothing written.
struct DetailContext {
    variables: BTreeMap<String, String>,
    dt_formats: BTreeMap<String, String>,
}

fn detail_context(state: &AppState) -> Result<DetailContext, AppError> {
    Ok(DetailContext {
        variables: state.store().all_variables()?,
        dt_formats: crate::settings::resolve_datetime_formats(state.store())
            .map_err(|e| AppError::internal(e.to_string()))?,
    })
}

/// Finish a template write: reload the registry and answer with the detail of exactly what was
/// written, built from the validated body rather than looked up in the reloaded registry.
fn reload_and_describe(
    state: &AppState,
    id: String,
    content: TemplateContent,
    context: &DetailContext,
    status: axum::http::StatusCode,
) -> Result<Response, AppError> {
    state.reload()?;
    let dt_resolver = crate::datetime_fmt::DateTimeResolver {
        formats: &context.dt_formats,
        now: chrono::Local::now(),
    };
    let detail = TemplateDefinition { id, content }.build_detail(&context.variables, &dt_resolver);
    Ok((status, Json(detail)).into_response())
}

#[utoipa::path(
    post,
    path = "/templates/{id}",
    tag = "api",
    params(("id" = String, Path, description = "Template ID")),
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 201, description = "Template created", body = TemplateDetail),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 409, description = "The template's file already exists", body = ErrorResponse),
        (status = 422, description = "Invalid template", body = ErrorResponse),
        (status = 500, description = "The write failed or the directory could not be re-read", body = ErrorResponse)
    )
)]
pub async fn create_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let content = parse_and_validate(&body)?;

    let _guard = state.write_lock.lock().await;
    let context = detail_context(&state)?;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    state.before_publish();
    // The exclusive publish decides existence from the disk, atomically, so a file copied in out of
    // band or a broken one is never overwritten.
    match fs_safe::stage_and_publish_new(root_fd.as_fd(), &format!("{id}.yaml"), &body)? {
        PublishResult::Published => {}
        PublishResult::AlreadyExists => {
            return Err(AppError::conflict(format!(
                "template '{id}' already exists"
            )));
        }
    }
    reload_and_describe(
        &state,
        id,
        content,
        &context,
        axum::http::StatusCode::CREATED,
    )
}

#[utoipa::path(
    put,
    path = "/templates/{id}",
    tag = "api",
    params(("id" = String, Path, description = "Template ID")),
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 200, description = "Template replaced", body = TemplateDetail),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "The template's file does not exist", body = ErrorResponse),
        (status = 422, description = "Invalid template", body = ErrorResponse),
        (status = 500, description = "The write failed or the directory could not be re-read", body = ErrorResponse)
    )
)]
pub async fn replace_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let content = parse_and_validate(&body)?;

    let _guard = state.write_lock.lock().await;
    let filename = format!("{id}.yaml");
    // Decided from the disk, not the registry, so a broken file is replaceable.
    match std::fs::symlink_metadata(state.templates_dir.join(&filename)) {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::not_found(NotFoundKind::Template, id));
        }
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to check template file '{filename}': {err}"
            )));
        }
    }
    let context = detail_context(&state)?;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    fs_safe::stage_and_replace(root_fd.as_fd(), &filename, &body)?;
    reload_and_describe(&state, id, content, &context, axum::http::StatusCode::OK)
}

#[utoipa::path(
    delete,
    path = "/templates/{id}",
    tag = "api",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 204, description = "Template deleted"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "The template's file does not exist", body = ErrorResponse),
        (status = 500, description = "File removal, the favorites prune, or the directory re-read failed", body = ErrorResponse)
    )
)]
pub async fn delete_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let _guard = state.write_lock.lock().await;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    fs_safe::unlink_file(root_fd.as_fd(), &id)?;
    state.store().remove_favorites_for_template(&id)?;
    state.reload()?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(
    get,
    path = "/templates/{id}",
    tag = "api",
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
    let variables = state.store().all_variables()?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .map_err(|e| AppError::internal(e.to_string()))?;
    let now = chrono::Local::now();
    let dt_resolver = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now,
    };
    state
        .templates
        .load_full()
        .detail(&id, &variables, &dt_resolver)
        .map(Json)
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, id))
}

#[utoipa::path(
    get,
    path = "/templates/{id}/source",
    tag = "api",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 200, description = "The template file's bytes, served or broken", content_type = "text/yaml"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "The template's file does not exist or cannot be read", body = ErrorResponse)
    )
)]
pub async fn template_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let yaml = std::fs::read(state.templates_dir.join(format!("{id}.yaml")))
        .map_err(|_| AppError::not_found(NotFoundKind::Template, id))?;
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
    tag = "api",
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
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, id.clone()))?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .map_err(|e| AppError::internal(e.to_string()))?;
    let now = chrono::Local::now();
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now,
    };
    let variables = state.store().all_variables()?;
    let data = template.placeholder_data(now);
    // Typst compilation/rasterization run inline on this request's thread, assuming small
    // local operator workloads without latency isolation for concurrent requests.
    let png = crate::render::render_thumbnail_png(template, &data, &variables, &dt)?;

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

#[utoipa::path(get, path = "/favorites", tag = "favorites",
    responses((status = 200, description = "Favorited template ids", body = Vec<String>)))]
pub async fn list_favorites(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
) -> Result<Json<Vec<String>>, AppError> {
    let ids = state.store().list_favorites(&principal.actor_id())?;
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
        return Err(AppError::not_found(NotFoundKind::Template, template_id));
    }
    state
        .store()
        .add_favorite(&principal.actor_id(), &template_id)?;
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
        .remove_favorite(&principal.actor_id(), &template_id)?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(get, path = "/recent-templates", tag = "favorites",
    responses((status = 200, description = "The caller's 6 most recently printed template ids", body = Vec<String>)))]
pub async fn recent_templates(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
) -> Result<Json<Vec<String>>, AppError> {
    let ids = state.store().recent_templates(&principal.actor_id())?;
    let registry = state.templates.load_full();
    Ok(Json(
        ids.into_iter()
            .filter(|id| registry.get(id).is_some())
            .collect(),
    ))
}
