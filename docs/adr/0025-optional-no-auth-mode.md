# 25. Optional no-auth mode for homelab

Date: 2026-06-21

## Status

Accepted. Supersedes, in part, [ADR-0017](0017-app-authentication.md) for issue
[#54](https://github.com/pfa230/labeler/issues/54): ADR-0017 mandated authentication on every `/api`
route; this ADR relaxes that for an explicit opt-in. ADR-0017 otherwise remains in force and auth-on
stays the default.

## Context

For a single-user homelab, the login wall is friction. ADR-0017 added accounts because the M7 Homebox
connector stores a third-party API token in app state, so the service could not be left open by default.
But an operator who accepts LAN-trust (the pre-#33 posture) should be able to run without a login. Two
asks: drop the 8-char password minimum (an implementer addition never in ADR-0017 or its plan), and add
an optional no-auth mode.

## Decision

- Drop the 8-char password rule: passwords must be non-empty, no length floor (`validate_password`).
- Add `LABELER_NO_AUTH=true` (fail-safe: any other value leaves auth ON; never the default). When set,
  the authentication subsystem is fully off:
  - `require_auth` short-circuits, injecting a distinct `Principal::Local` so data routes are open and
    no handler can mistake the caller for a real stored user.
  - The credential-management surface (`/auth/setup`, `/auth/login`, `/auth/logout`, `/auth/password`,
    `/users`, `/tokens` and their sub-paths) returns `403 "authentication is disabled"`, so no durable
    user or token can be created or changed while auth is off. Turning auth back on therefore leaves no
    seeded backdoor.
  - A relaxed origin check still rejects state-changing requests whose Origin/Referer is present and
    mismatched, while allowing no-Origin (non-browser) callers; this preserves drive-by CSRF protection
    without a session.
  - `/auth/me` reports `{ authed: true, needsSetup: false, me: local, noAuth: true }`, so the SPA skips
    the login/setup wall and hides credential-management UI.

## Consequences

- No-auth = LAN-trust: every non-credential `/api` route, including the stored Homebox API key, is
  reachable by anyone on the LAN. This is a deliberate, documented homelab opt-in, never a default.
- The blast radius is narrowed versus a naive bypass: credential management is inert, and browser
  drive-by writes with a mismatched Origin are rejected. The residual exposure (a non-browser attacker
  already on the LAN reading/writing the open data API) is inherent to the mode.
