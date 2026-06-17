# 17. App authentication

**Status:** Accepted

## Context

The service binds `0.0.0.0` and, until now, ran with no authentication (LAN-trust). The M7 Homebox
integration stores a third-party API token in app state, so the service can no longer be left open to
anyone on the network. Issue #33 asked for user accounts with roles and granular permissions; the
approved scope for this milestone is flat authentication (real accounts, no role tiers) as the
prerequisite for M7. Two caller classes need to be supported: browsers (the React SPA) and automation
(scripts, the CSV importer, future integrations).

## Decision

- **Flat auth, no roles.** Every authenticated user is equal. Any authenticated user can manage users
  and API tokens. There are no admin-only operations.
- **Two credential types behind one middleware.** A single `require_auth` middleware gates every `/api`
  route except a small exemption list. It resolves the caller in order: an `Authorization: Bearer`
  token (machines), else a session cookie (browsers), else `401`.
- **Session cookies for browsers.** The cookie value is an opaque 256-bit random string, stored
  server-side only as a SHA-256 hash, set `HttpOnly`, `SameSite=Lax`, `Path=/`, and `Secure` when the
  effective scheme is https. Sessions have a 30-day sliding expiry with throttled writes. Login rotates
  the session id; logout deletes the row.
- **Origin check for cookie-authenticated state changes.** Every cookie-authenticated POST/PUT/DELETE/
  PATCH (including the unauthenticated `login`/`setup` and the `logout` endpoint) must present an
  `Origin` (or `Referer`) whose authority matches the request `Host`, else `403`. Token requests are
  exempt from this check. CSRF exemption is tracked separately from auth exemption so an auth-exempt
  endpoint is never accidentally CSRF-exempt.
- **API tokens for machines.** A token is a random 256-bit secret (display prefix `lbl_`) shown once at
  creation and stored only as a SHA-256 hash. Tokens are created and revoked through the API/UI.
- **First-run setup plus optional env bootstrap.** While zero users exist, `POST /api/auth/setup`
  creates the first account; afterwards it returns `409`. For headless deploys, `LABELER_INIT_USER` and
  `LABELER_INIT_PASSWORD` seed the first user at startup when no users exist.
- **Passwords with argon2id.** Hashed with the argon2 crate (default params), never logged. Login
  returns a generic `401` and verifies against a dummy hash on unknown user to flatten timing.

This resolves issue #33 minus granular permissions and admin tiers.

## Consequences

- All existing `/api` endpoints now require authentication. The exempt set is `GET /api/health`,
  `POST /api/auth/login`, `POST /api/auth/setup`, `GET /api/auth/me`, `GET /api/openapi.json`, and
  `/api/docs`. The Docker healthcheck stays on the exempt `/api/health`.
- `scripts/*.sh` and any automation must send `Authorization: Bearer $LABELER_API_TOKEN`; the CSV
  importer is a token caller.
- Behind a TLS-terminating proxy, `LABELER_TRUST_PROXY=true` makes the service honor
  `X-Forwarded-Proto` when deciding the `Secure` cookie flag; on a plain-http LAN the cookie is
  non-Secure (acceptable under LAN-trust, documented).
- Request logging must never emit `Cookie` or `Authorization` headers.
- **Deferred:** roles and admin tiers, OIDC/SSO, rate-limiting and account lockout, database-at-rest
  encryption, refresh tokens, and email password reset. These are out of scope and can supersede this
  ADR when revisited.
