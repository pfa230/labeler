# 61. A connection's public URL is the link base, its base URL is the fetch base

Date: 2026-08-21

## Status

Accepted. Issue [#169](https://github.com/pfa230/labeler/issues/169). Supersedes the **Connections store** decision of [ADR-0018](0018-api-integration-spine.md) in part.

## Context

When Labeler connects to an upstream inventory service such as Homebox running inside a Docker network or behind an internal reverse proxy, the `base_url` configured on the connection is an internal address (e.g. `http://homebox:7745`) reachable only by the Labeler server.

However, Labeler also generates entity links for humans to use: `url` on browsed rows in the Connect page, and `item_url` / `location_url` materialized into label data that is encoded into printed QR codes. If these links use `base_url`, physical labels and browser navigation point to an unresolvable or inaccessible internal hostname when accessed from client browsers or mobile devices scanning QR codes.

## Decision

**A connection carries two distinct addresses for two distinct jobs:**

1. **`base_url` is the fetch base.** All outbound HTTP requests made by the Labeler server to the upstream inventory system dial `base_url`.
2. **`public_url` is the link base.** Every URL generated for human consumption—the row `url` on browse results and `item_url` / `location_url` projected into label data—is built from `public_url` when present and non-blank, falling back to `base_url` when absent.

**URL validation and userinfo rejection.**
Both `base_url` and `public_url` undergo identical syntactic validation: trimmed of whitespace, valid absolute URL with `http` or `https` scheme, a host present, no query parameters, no fragments, and no embedded userinfo (`user:pass@`). Userinfo is rejected because connection URLs are printed into physical QR codes and rendered as navigation links; credentials embedded in the URL would leak onto physical labels. Trailing slashes are stripped on storage.

**Error discrimination.**
URL validation failures report `details.reason = "base_url_invalid"` for `base_url` and `details.reason = "public_url_invalid"` for `public_url`, allowing clients to distinguish which field was rejected.

**UI and partial updates.**
Settings > Connections provides a **public url** field beside **base url**. Saving from the UI always submits `public_url`, sending `null` when the field is left empty so that clearing the input clears the stored value.

## Consequences

- Printed QR codes and browser links resolve to the operator's public hostname while server egress continues dialing internal or container network addresses.
- The `connections` table record is `(connector, name, base_url, public_url, transforms, credential, enabled)`.
- Client and API callers receive `public_url_invalid` instead of `base_url_invalid` when a submitted public URL fails validation.
- Existing stored connections with userinfo continue functioning at rest, but will be rejected upon the next save until credentials are removed from the URL.
