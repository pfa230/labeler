use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;

use crate::api::AppState;
use crate::errors::AppError;

pub const SESSION_COOKIE: &str = "labeler_session";

/// Who is making the request (after auth). Inserted into request extensions for handlers that need it.
#[derive(Clone)]
pub enum Principal {
    User { id: String, username: String },
    Token { id: String },
    Local,
}

impl Principal {
    /// Stable actor identity for per-user data and job attribution.
    pub fn actor_id(&self) -> String {
        match self {
            Principal::User { id, .. } => id.clone(),
            Principal::Token { id } => format!("token:{id}"),
            Principal::Local => "local".to_string(),
        }
    }
}

/// Paths that never require auth. IMPORTANT: this middleware is layered on the router that is then
/// `nest("/api", ...)`-ed, so axum strips the `/api` prefix before the inner router runs. The path seen
/// here is therefore the STRIPPED path (`/health`, not `/api/health`). `/auth/me` is exempt and does its
/// own optional auth in the handler.
fn is_auth_exempt(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/auth/login" | "/auth/setup" | "/auth/me" | "/openapi.json"
    ) || path.starts_with("/docs")
}

/// True for state-changing methods (the ones that need an origin check when cookie-authenticated).
fn is_state_changing(method: &axum::http::Method) -> bool {
    matches!(
        method,
        &axum::http::Method::POST
            | &axum::http::Method::PUT
            | &axum::http::Method::DELETE
            | &axum::http::Method::PATCH
    )
}

/// Effective scheme from request parts: trust X-Forwarded-Proto only when configured, else use the
/// connection scheme (behind a TLS-terminating proxy set LABELER_TRUST_PROXY=true). Takes `HeaderMap` +
/// `Uri` so it works from a `FromRequestParts` extractor (which has `&mut Parts`, not `Request<Body>`).
pub fn effective_https(
    headers: &axum::http::HeaderMap,
    uri: &axum::http::Uri,
    trust_proxy: bool,
) -> bool {
    if trust_proxy {
        if let Some(p) = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
        {
            return p.eq_ignore_ascii_case("https");
        }
    }
    uri.scheme_str() == Some("https")
}

/// Reject a cookie-authenticated state-changing request whose Origin/Referer authority does not match
/// the effective host. Parses the Origin/Referer as a URI and compares its authority (host:port)
/// case-insensitively. Behind a trusted reverse proxy (which may rewrite `Host` to an internal value),
/// the original external host is taken from `X-Forwarded-Host` so the browser's `Origin` still matches.
fn origin_ok(req: &Request<Body>, trust_proxy: bool) -> bool {
    let fwd_host = if trust_proxy {
        req.headers()
            .get("x-forwarded-host")
            .and_then(|v| v.to_str().ok())
    } else {
        None
    };
    let host = match fwd_host.or_else(|| {
        req.headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
    }) {
        Some(h) => h,
        None => return false,
    };
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .or_else(|| req.headers().get(header::REFERER))
        .and_then(|v| v.to_str().ok());
    // No Origin/Referer at all: reject (browsers send one for cookie state-changing requests).
    let origin = match origin {
        Some(o) => o,
        None => return false,
    };
    match origin.parse::<axum::http::Uri>() {
        Ok(uri) => uri
            .authority()
            .map(|a| a.as_str().eq_ignore_ascii_case(host))
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Credential-management paths (stripped of the `/api` prefix). In no-auth mode every method on these is
/// rejected, so no durable user or token can be created, changed, listed, or deleted while auth is off.
/// `/auth/me` is intentionally excluded (it must answer in no-auth mode).
fn is_auth_managed(path: &str) -> bool {
    matches!(
        path,
        "/auth/setup" | "/auth/login" | "/auth/logout" | "/auth/password" | "/users" | "/tokens"
    ) || path.starts_with("/users/")
        || path.starts_with("/tokens/")
}

/// Relaxed origin check for no-auth mode: reject only when an Origin/Referer IS present and its authority
/// does not match the host; allow when absent (non-browser callers like curl or scripts). This preserves
/// drive-by CSRF protection without a session. Mirrors `origin_ok`'s host/authority extraction but treats
/// a missing Origin as allowed rather than rejected.
fn origin_present_and_mismatched(req: &Request<Body>, trust_proxy: bool) -> bool {
    let fwd_host = if trust_proxy {
        req.headers()
            .get("x-forwarded-host")
            .and_then(|v| v.to_str().ok())
    } else {
        None
    };
    let host = match fwd_host.or_else(|| {
        req.headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
    }) {
        Some(h) => h,
        None => return false,
    };
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .or_else(|| req.headers().get(header::REFERER))
        .and_then(|v| v.to_str().ok());
    let origin = match origin {
        Some(o) => o,
        None => return false, // no Origin: non-browser caller, allow
    };
    match origin.parse::<axum::http::Uri>() {
        Ok(uri) => !uri
            .authority()
            .map(|a| a.as_str().eq_ignore_ascii_case(host))
            .unwrap_or(false),
        Err(_) => true, // unparseable Origin: reject
    }
}

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if state.no_auth() {
        if is_auth_managed(&path) {
            return AppError::forbidden("authentication is disabled").into_response();
        }
        if is_state_changing(req.method())
            && origin_present_and_mismatched(&req, state.trust_proxy())
        {
            return AppError::forbidden("cross-origin request rejected").into_response();
        }
        return run_with(req, next, Principal::Local).await;
    }
    if is_auth_exempt(&path) {
        // Even exempt endpoints get the origin check when they are cookie state-changing (login/setup CSRF).
        if is_state_changing(req.method())
            && req.headers().get(header::AUTHORIZATION).is_none()
            && !origin_ok(&req, state.trust_proxy())
        {
            return AppError::forbidden("cross-origin request rejected").into_response();
        }
        return next.run(req).await;
    }

    // 1) Bearer token (machine). Checked first; exempt from the origin check.
    if let Some(auth) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(secret) = auth.strip_prefix("Bearer ") {
            match state
                .store()
                .lookup_token(&crate::auth::sha256_hex(secret))
                .await
            {
                Ok(Some(id)) => return run_with(req, next, Principal::Token { id }).await,
                Ok(None) => return AppError::unauthorized().into_response(),
                Err(_) => return AppError::internal("token lookup failed").into_response(),
            }
        }
    }

    // 2) Session cookie (browser). Origin check on state-changing methods.
    let jar = CookieJar::from_headers(req.headers());
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        if is_state_changing(req.method()) && !origin_ok(&req, state.trust_proxy()) {
            return AppError::forbidden("cross-origin request rejected").into_response();
        }
        match state
            .store()
            .lookup_session(&crate::auth::sha256_hex(cookie.value()))
            .await
        {
            Ok(Some(user)) => {
                return run_with(
                    req,
                    next,
                    Principal::User {
                        id: user.id,
                        username: user.username,
                    },
                )
                .await
            }
            Ok(None) => return AppError::unauthorized().into_response(),
            Err(_) => return AppError::internal("session lookup failed").into_response(),
        }
    }

    AppError::unauthorized().into_response()
}

async fn run_with(mut req: Request<Body>, next: Next, principal: Principal) -> Response {
    req.extensions_mut().insert(principal);
    next.run(req).await
}

// Cookie helpers shared by the auth handlers.
pub fn session_cookie(value: String, https: bool) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};
    let mut c = Cookie::new(SESSION_COOKIE, value);
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_secure(https);
    c.set_path("/");
    c
}
pub fn clear_cookie() -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::Cookie;
    let mut c = Cookie::new(SESSION_COOKIE, "");
    c.set_path("/");
    c.make_removal(); // expire immediately (avoids the non-public axum_extra::...::time path)
    c
}

/// Optional auth resolver for `/auth/me` (which is middleware-exempt and must not reject when missing):
/// try bearer token then session cookie; return the principal if valid, else None.
pub async fn resolve_optional(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Option<Principal> {
    if let Some(secret) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|a| a.strip_prefix("Bearer "))
    {
        if let Ok(Some(id)) = state
            .store()
            .lookup_token(&crate::auth::sha256_hex(secret))
            .await
        {
            return Some(Principal::Token { id });
        }
        return None;
    }
    let jar = CookieJar::from_headers(headers);
    let cookie = jar.get(SESSION_COOKIE)?;
    match state
        .store()
        .lookup_session(&crate::auth::sha256_hex(cookie.value()))
        .await
    {
        Ok(Some(user)) => Some(Principal::User {
            id: user.id,
            username: user.username,
        }),
        _ => None,
    }
}
