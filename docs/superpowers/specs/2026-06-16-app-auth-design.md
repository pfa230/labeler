# App Authentication — design

Context: a Rust/axum service that serves a React SPA at `/` and a JSON API under `/api` (all API routes nested under `/api`, ADR-0008). State in SQLite via rusqlite + rusqlite_migration. Currently binds 0.0.0.0 with NO authentication (LAN-trust). This adds flat app-level authentication (real user accounts + login, NO roles/admin tiers) as the prerequisite for the M7 Homebox integration, which stores a third-party API token. Rescopes issue #33 to flat auth (drops granular permissions/admin-only). Reviewed with one adversarial codex pass (no MAJOR issues; the MINOR hardening below is incorporated).

User-approved decisions:
1. Browser auth = server-side session cookie (not bearer-token-in-JS).
2. First user = a first-run setup screen, with optional env-var bootstrap.
3. Automation/non-browser = API tokens (table + create/revoke UI); middleware accepts session OR token.
4. Flat: every authenticated user is equal; any authenticated user can manage users and tokens.

## 1. Mechanism
One axum auth middleware gates EVERY `/api` route except the exemptions. It resolves the caller in order:
- If `Authorization: Bearer <token>` is present: hash it and look up `api_tokens WHERE token_hash = ?` (direct equality, no raw compare; `token_hash` is `UNIQUE`). If valid, authenticate as a machine principal.
- Else if the session cookie is present: hash the cookie value, look up `sessions` joined to `users`, check `expires_at` and that the user still exists; if valid, authenticate as that user.
- Else: 401.

A SEPARATE CSRF/origin check (distinct from the auth-exemption list) applies to every state-changing request (POST/PUT/DELETE) that authenticates via the **session cookie**, including the unauthenticated `login`/`setup` and the `logout` endpoints: the `Origin` (or, if absent, `Referer`) header must match the request host, else 403. API-token requests (no cookie, machine-driven) are exempt from the origin check. This blocks login-CSRF and cross-site state changes; `SameSite=Lax` is the cookie-level backstop.

Session cookie: httpOnly, `SameSite=Lax`, `Path=/`, name `labeler_session`, value = an opaque 256-bit random string (base64url). Stored server-side only as a SHA-256 hash (a DB read does not expose live sessions). 30-day sliding expiry, but the expiry/`last_used_at` writes are throttled (only updated when the stored value is older than ~1 hour) so a request burst does not write on every call. Logout deletes the row and clears the cookie. The `Secure` attribute is set when the effective scheme is https; the effective scheme trusts `X-Forwarded-Proto` ONLY when `LABELER_TRUST_PROXY=true` (so LAN clients cannot spoof it), otherwise it uses the connection scheme.

Exempt from AUTH (always reachable unauthenticated): `GET /api/health` (response is strictly `{ "status": "ok" }`, no diagnostics), `POST /api/auth/login`, `POST /api/auth/setup` (only while zero users exist), and the static SPA assets + `index.html` fallback. Everything else under `/api` requires session-or-token. (These auth-exempt endpoints still get the origin check where they are state-changing.)

## 2. Data model (new migrations)
- `users { id TEXT PK, username TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL, created_at }`
- `sessions { id TEXT PK (= SHA-256 of the cookie value), user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, expires_at, created_at }`
- `api_tokens { id TEXT PK, name TEXT, token_hash TEXT UNIQUE NOT NULL, created_at, last_used_at }`
Passwords hashed with argon2id (argon2 crate, default params). API token = a random 256-bit string shown ONCE at creation (display prefix `lbl_…`), stored only as a SHA-256 hash. Session id likewise stored only as a hash.

## 3. Endpoints
- `POST /api/auth/setup` `{username, password}` → creates the FIRST user; 409 if any user already exists. The zero-users check + insert run under the store write-lock to avoid a race creating two "first" users. Logs the user in (sets a fresh session). Origin-checked.
- `POST /api/auth/login` `{username, password}` → verifies argon2; on success generates a FRESH session id (rotation; clears any prior session for the presented cookie) and sets the cookie; 401 on failure with a generic message and a dummy-hash verify on unknown user (no user-enumeration/timing signal). Origin-checked.
- `POST /api/auth/logout` → deletes the current session, clears the cookie. Origin-checked.
- `GET /api/auth/me` → `{ id, username }` or 401. Drives the SPA's logged-in state. Also reports whether setup is needed (zero users) for the unauthenticated case, e.g. `401 { needs_setup: true|false }`.
- `GET /api/users` / `POST /api/users` `{username,password}` / `DELETE /api/users/{id}`. Flat. Cannot delete the last remaining user (409). Deleting a user cascades their sessions (immediate revocation).
- `POST /api/auth/password` `{current_password, new_password}` → change own password (verifies current); on success revokes the user's OTHER sessions (keeps the current one).
- `GET /api/tokens` (id, name, created_at, last_used_at; never the secret) / `POST /api/tokens` `{name}` → returns the secret ONCE / `DELETE /api/tokens/{id}` (revoke).
- Optional env bootstrap: on startup, if zero users and `LABELER_INIT_USER`/`LABELER_INIT_PASSWORD` are set, create that user (password read from env, never logged). Documented as a convenience for headless deploys.

## 4. Frontend
- A `useAuth` hook backed by `GET /api/auth/me` (React Query). A `<RequireAuth>` guard wraps the app shell: a 401 redirects to `/login`, or to `/setup` when `me` reports `needs_setup`. A global 401 interceptor in the API client redirects to `/login` and clears cached queries.
- Screens: `/setup` (only reachable when zero users; create the first account), `/login` (username/password), a Users section and an API-Tokens section (in Settings or their own screens), and a logout control in the shell. Token-create shows the secret once with a copy button and a "you will not see this again" note.
- All existing screens move behind the guard; nothing else changes inside them.

## 5. Security
- **Passwords:** argon2id; never logged. Login returns a generic 401 (no distinction between unknown-user and bad-password) and verifies against a dummy hash on unknown user to flatten timing.
- **CSRF / origin:** SameSite=Lax + the Origin/Referer check on all cookie-authenticated state-changing requests (including login/setup/logout). CSRF-exemption is tracked separately from auth-exemption so an auth-exempt endpoint is not accidentally CSRF-exempt.
- **Session hygiene:** id rotation on login; server-side deletion on logout; cascade-delete on user deletion; revoke-others on password change; session auth fails if the referenced user no longer exists. Secrets at rest are SHA-256 hashes (sessions, tokens) and argon2id (passwords).
- **Secure cookie behind a proxy:** `LABELER_TRUST_PROXY` gates whether `X-Forwarded-Proto` is honored; plain-http LAN use issues a non-Secure cookie (acceptable under LAN-trust, documented).
- **Logging:** TraceLayer (and any request logging) must not log `Cookie` or `Authorization` headers; passwords/secrets never logged.
- **Out of scope (consistent with the store):** DB-file-at-rest encryption.

## 6. Testing
- Unit: argon2 verify; session create/lookup/expire/slide(throttled)/rotate-on-login; token hash + lookup; last-user-delete guard; user-delete cascades sessions; password-change revokes others; setup-when-zero-users then 409.
- HTTP integration (src/lib.rs style): unauthenticated protected route → 401; login → cookie → access; bad password → 401; logout → 401 again; API token header → access; bad token → 401; origin-mismatch cookie POST → 403; token request with mismatched origin → allowed; setup creates first user then 409; `GET /api/health` open and returns `{status:"ok"}`.
- Frontend: guard redirects unauthenticated → /login; `/setup` shown when `needs_setup`; login flow sets state; token-create shows the secret once.

## 7. Scope
- In: flat user auth (session cookie), first-run setup + optional env bootstrap, API tokens (create/revoke), the auth middleware + exemptions, the origin/CSRF check, users/tokens CRUD + screens, redaction. Resolves #33 minus granular permissions/roles.
- Out / deferred: roles/admin tiers, OIDC/SSO, email password reset, rate-limiting/account lockout (only a small unknown-user delay), DB-at-rest encryption, refresh tokens. The later Homebox connection CRUD simply requires auth (no admin-only distinction, since flat).
- Impact: ALL existing `/api` endpoints become authenticated. `scripts/*.sh` must send `Authorization: Bearer $LABELER_API_TOKEN` (update the scripts + document); `/import/csv` callers use a token; the Docker healthcheck stays on the exempt `/api/health`.
