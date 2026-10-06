# Auth

## Purpose

Who may call the API and how: users, sessions, API tokens, the origin check, first-run setup, no-auth mode, and the screens that manage them.

## Requirements

### Requirement: Gated routes

Every `/api` route SHALL require authentication except `GET /api/health`, `POST /api/auth/login`, `POST /api/auth/setup`, `GET /api/auth/me`, `GET /api/openapi.json` and `/api/docs*`. The caller SHALL be resolved from an `Authorization: Bearer <token>` header first, then the `labeler_session` cookie; an unknown token, an unknown or expired session, or no credential SHALL be `401 Unauthorized`. All users are equal and MAY manage users and tokens.

#### Scenario: An unknown bearer token beats a valid cookie

- **WHEN** a request carries a valid session cookie and `Authorization: Bearer nope`
- **THEN** the response is `401`

### Requirement: Origin check

A `POST`, `PUT`, `DELETE` or `PATCH` that is cookie-authenticated, or that targets `login` or `setup` without an `Authorization` header, MUST carry an `Origin` (else `Referer`) whose authority equals `Host` case-insensitively, else `403 Forbidden`; a missing or unparseable header fails. Bearer requests SHALL skip the check. With `LABELER_TRUST_PROXY=true`, `X-Forwarded-Host` SHALL stand in for `Host`.

#### Scenario: Login without an Origin

- **WHEN** a client posts valid credentials to `/api/auth/login` with no `Origin`, `Referer` or `Authorization`
- **THEN** the response is `403` and no session is created

### Requirement: Sessions

Login and setup SHALL set cookie `labeler_session` to a random 256-bit value, stored only as its SHA-256 hash, with `HttpOnly`, `SameSite=Lax`, `Path=/`, and `Secure` exactly when the scheme is https (`X-Forwarded-Proto` decides when `LABELER_TRUST_PROXY=true`). A session SHALL expire 30 days after last use, refreshed at most hourly, and SHALL die with its user. Issuing a session SHALL delete the one the request's cookie named.

#### Scenario: Login rotates the session

- **WHEN** a client holding session A logs in again
- **THEN** a new cookie B is set and A no longer authenticates

### Requirement: Auth endpoints

| Method | Path | Body | Success | Refusals |
|---|---|---|---|---|
| POST | `/api/auth/setup` | `{username, password}` | `200 {ok: true}`, session set | `409 Conflict` once a user exists |
| POST | `/api/auth/login` | `{username, password}` | `200 {ok: true}`, session set | `401` unknown user or wrong password |
| POST | `/api/auth/logout` | | `200 {ok: true}`, session deleted, cookie cleared | |
| GET | `/api/auth/me` | | `200`, see "Auth state" | |
| POST | `/api/auth/password` | `{current_password, new_password}` | `200 {ok: true}`; the user's other sessions are deleted | `403` token caller; `401` wrong current password |
| GET | `/api/users` | | `[{id, username}]` by username | |
| POST | `/api/users` | `{username, password}` | `201 {id, username}` | `409 Conflict` taken username |
| DELETE | `/api/users/{id}` | | `204` | `409 Conflict` if one user remains (checked first) or the id is the caller's; `404 NotFound` |
| GET | `/api/tokens` | | `[{id, name, last_used_at, created_at}]` by creation | |
| POST | `/api/tokens` | `{name}` | `201 {id, name, secret}` | |
| DELETE | `/api/tokens/{id}` | | `204` | `404 NotFound` |

Passwords SHALL be argon2id hashes. A new username MUST be non-blank (else `400`, reason `username_empty`) and is stored as sent; a new password MUST be non-empty (else reason `password_empty`), with no minimum length.

#### Scenario: Password change keeps the current session

- **WHEN** a user with sessions A and B changes their password from A
- **THEN** A still authenticates and B gets `401`

### Requirement: API tokens

A token secret SHALL be `lbl_` plus 256 random bits in URL-safe base64, shown only in the create response and stored only as its SHA-256 hash. Using it SHALL update `last_used_at`, at most hourly.

#### Scenario: Using a token

- **WHEN** a client calls `GET /api/templates` with `Authorization: Bearer <secret>`
- **THEN** the response is `200` and the token's `last_used_at` is set

### Requirement: Auth state

`GET /api/auth/me` SHALL answer `200` with `Cache-Control: no-store`: `{authed: true, needsSetup: false, me: {id, username}}` for a session, `me: {id: "token", username: "api-token"}` for a token, and otherwise `{authed: false, needsSetup}` with `needsSetup` true exactly when no user exists.

#### Scenario: Fresh install

- **WHEN** no user exists and an anonymous client calls it
- **THEN** the body is `{"authed": false, "needsSetup": true}`

### Requirement: Startup bootstrap

When `LABELER_INIT_USER` and `LABELER_INIT_PASSWORD` are both non-empty and no user exists, startup SHALL create that user, and SHALL abort if it cannot.

#### Scenario: Headless first user

- **WHEN** the server starts on an empty store with both set
- **THEN** that user can log in and setup returns `409`

### Requirement: No-auth mode

With `LABELER_NO_AUTH=true` every `/api` route SHALL run as actor `local`, except that every method on `/api/auth/setup`, `login`, `logout`, `password`, `/api/users*` and `/api/tokens*` SHALL be `403 Forbidden` before the body is read, and a state-changing request whose `Origin` (else `Referer`) is present and mismatched or unparseable SHALL be `403`. `GET /api/auth/me` SHALL return `{authed: true, needsSetup: false, me: {id: "local", username: "local"}, noAuth: true}`. The startup bootstrap still runs.

#### Scenario: Scripted write without Origin

- **WHEN** curl posts to `/api/batch` with no `Origin` in no-auth mode
- **THEN** the request is processed

### Requirement: Auth UI

While not authed the SPA SHALL send every page to `/setup` when `needsSetup`, else `/login`, and `/login` and `/setup` SHALL redirect to `/` once authed; any API `401` SHALL navigate to `/login`. The sidebar SHALL show the username and Logout, which lands on `/login` whatever the outcome. Settings SHALL have a Users section (list, caller marked "(you)" with Delete disabled, Confirm before delete, Add user, Change my password) and an API tokens section (name, last used or "never", created; Confirm before revoke; a new secret shown once with Copy). In no-auth mode the sidebar user, Logout and both sections SHALL be hidden.

#### Scenario: Anonymous visit on a fresh install

- **WHEN** an anonymous browser opens `/` and no user exists
- **THEN** it lands on `/setup`
