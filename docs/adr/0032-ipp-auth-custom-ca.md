# 32. IPP basic-auth + custom-CA for printing

Date: 2026-06-26

## Status

Accepted. Related to issue [#39](https://github.com/pfa230/labeler/issues/39).
Partially supersedes [ADR-0029](0029-runtime-base-debian-slim.md): the per-printer `ca_cert` field
makes the derived-image CA-certificate workaround from that ADR unnecessary for self-signed printer
CAs (resolves #91).

## Context

The `cups` driver introduced in ADR-0007 sent every IPP job unauthenticated over an unencrypted
connection. CUPS queues and IPP-Everywhere printers commonly sit behind basic-auth, use self-signed
TLS certificates, or both. Without support for credentials and custom CAs, the driver could not reach
any printer that requires authentication, and operators wanting TLS on a LAN printer had to either
build a derived image to inject a CA certificate into the system store (the workaround documented in
ADR-0029/#91) or disable TLS entirely.

The design goals:

- **Basic auth.** Supply a username and password to the `ipp` builder so authenticated CUPS queues
  work without a proxy or out-of-band credential store.
- **Custom CA.** Accept an inline PEM block trusted for that specific printer, rather than requiring a
  system-store change. This replaces the derived-image workaround for self-signed printer certificates.
- **Insecure skip-verify.** Provide an escape hatch for lab/dev setups where a printer's certificate
  cannot be trusted at all, with an explicit operator opt-in.
- **Password write-only.** Passwords must be replayed to the printer on every job, so they must be
  stored in a recoverable form. They must not leak via API responses.

## Decision

The `cups` driver config expands from `{ uri }` to:

```
{
  uri:       string            -- required; must start with ipp:// or ipps://
  username:  string | absent   -- optional; used only when password is also present
  password:  string | absent   -- optional; WRITE-ONLY (see below)
  ca_cert:   string | absent   -- optional; inline PEM (-----BEGIN CERTIFICATE-----)
  insecure:  bool   | absent   -- optional; default false
}
```

**Password write-only.** The `password` field is stored plaintext (it must be replayed to the printer
on every job) but is redacted by omission on every API response: `GET /printers`, `GET /printers/{id}`,
`POST /printers` (create response), and `PUT /printers/{id}` (replace response) never include the
`password` key. On update (`PUT /printers/{id}`), the merge rules are:

- Key absent in the request body: keep the stored password.
- Key present as a string: replace the stored password.
- Key present as `null`: clear the stored password (unauthenticated from this point).

This is the same keep/set/clear pattern as the connections credential field (ADR-0018).

**Custom CA.** `ca_cert` is a full inline PEM certificate (`-----BEGIN CERTIFICATE-----` / `-----END
CERTIFICATE-----`). It is trusted only for this printer's TLS handshake, via the `ipp` builder's
`ca_cert()` method. Validation at create/replace time requires the marker string to be present; an
obviously non-PEM value is rejected with `400`. This replaces the derived-image workaround
(ADR-0029/#91) for operators with self-signed printer certificates.

**`insecure` flag.** When `insecure: true`, the `ipp` builder's `ignore_tls_errors(true)` is set,
which skips TLS certificate verification entirely. If both `insecure` and `ca_cert` are set,
`insecure` dominates: certificate verification is still skipped. The UI warns when `insecure` is
combined with credentials (MITM credential-theft risk) and when credentials are sent over a plain
`ipp://` URI (cleartext on the wire).

**Credentials over `ipp://` (warn but allow).** The service does not block credential use over
`ipp://`. Blocking would be a breaking change for LAN setups where the printer is reachable only by
plain IPP, and operator intent cannot be inferred. The UI surfaces a visible warning; operators using
the API directly accept the risk. The correct posture for a production deployment is `ipps://`.

**Trusted-host assumption.** Credentials are stored as plaintext in the SQLite app-state file
(`LABELER_DATA_DIR/labeler.db`). Anyone with read access to that file can recover the printer
password. This is consistent with the trusted-LAN threat model of the app (ADR-0007). The operator is
responsible for restricting file-system access.

## Consequences

- Authenticated CUPS queues and self-signed CUPS certificates now work without out-of-band
  configuration.
- The derived-image CA-workaround documented in ADR-0029 is superseded for the per-printer use
  case. System-wide public-CA trust still relies on the `ca-certificates` package installed per
  ADR-0029.
- Printer passwords are stored as plaintext and are recoverable to anyone with filesystem or database
  access. Operators who treat the data volume as secret (e.g. restricted file permissions, encrypted
  volumes) carry the credential-at-rest risk accordingly.
- `insecure: true` disables all TLS verification. Combined with credentials it enables a MITM attacker
  on the LAN to harvest the printer password. The UI warns; the API does not block it.
- Credentials sent over `ipp://` travel unencrypted. The UI warns; the API does not block it.
- The authenticated `send` path (real CUPS server required) is not exercised in CI. Unit and HTTP
  tests cover config parsing, driver construction, redaction, and the keep/set/clear merge logic.
- No live send tests are added for custom-CA or `insecure` paths; those require a real TLS endpoint
  and are left as operator-verified with their own printer setup.
