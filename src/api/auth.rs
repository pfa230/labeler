//! Authentication, user and API-token routes.

use axum::{
    extract::{FromRequestParts, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::CookieJar;
use std::sync::Arc;

use crate::{
    errors::{AppError, NotFoundKind},
    extract::{Json, Path},
    models::ErrorResponse,
    reason::Reason,
};

use super::AppState;

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
        .route("/users", get(list_users).post(create_user_h))
        .route("/users/{id}", axum::routing::delete(delete_user_h))
        .route("/tokens", get(list_tokens).post(create_token_h))
        .route("/tokens/{id}", axum::routing::delete(delete_token_h))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
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
    if let Some(crate::middleware::Principal::User { id, username, .. }) =
        crate::middleware::resolve_optional(&state, &headers).await
    {
        return Ok(no_store(
            Json(serde_json::json!({
                "authed": true,
                "needsSetup": false,
                "me": {"id": id, "username": username}
            }))
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
        return Err(AppError::not_found(NotFoundKind::User, &id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
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
    let crate::middleware::Principal::User {
        id,
        by_token: false,
        ..
    } = p
    else {
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
#[serde(deny_unknown_fields)]
pub struct TokenCreate {
    pub name: String,
}

/// The calling user's id for the token endpoints. No-auth mode refuses `/tokens*` in the middleware,
/// so `Local` cannot reach a handler; it is refused rather than trusted.
fn token_owner(principal: &crate::middleware::Principal) -> Result<&str, AppError> {
    match principal {
        crate::middleware::Principal::User { id, .. } => Ok(id),
        crate::middleware::Principal::Local => {
            Err(AppError::forbidden("authentication is disabled"))
        }
    }
}

#[utoipa::path(
    get,
    path = "/tokens",
    tag = "auth",
    responses(
        (status = 200, description = "The caller's own API tokens (never the secret)", body = [TokenSummary]),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn list_tokens(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
) -> Result<Response, AppError> {
    let t = state
        .store()
        .list_tokens(token_owner(&p)?)
        .await
        .map_err(AppError::from)?;
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
        (status = 201, description = "Token created for the caller; secret returned once", body = TokenCreated),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn create_token_h(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Json(body): Json<TokenCreate>,
) -> Result<Response, AppError> {
    let owner = token_owner(&p)?;
    let _guard = state.write_lock.lock().await;
    let secret = format!("lbl_{}", crate::auth::random_secret());
    let t = state
        .store()
        .create_token(owner, &body.name, &crate::auth::sha256_hex(&secret))
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
        (status = 404, description = "No such token of the caller's", body = ErrorResponse)
    )
)]
pub async fn delete_token_h(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let owner = token_owner(&p)?;
    let _guard = state.write_lock.lock().await;
    if !state
        .store()
        .delete_token(owner, &id)
        .await
        .map_err(AppError::from)?
    {
        return Err(AppError::not_found(NotFoundKind::Token, &id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

// Effective-https extractor for the cookie Secure flag (proxy-aware), used by setup/login.
pub(crate) struct HttpsHint(bool);
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
