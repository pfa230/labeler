# 9. Image source model

**Status:** Accepted

## Context

The `image` layout item (issue #3) must obtain image bytes from two places: a static asset bundled with
templates (the logo case) and per-label data (a product photo). Each transport has a security and
complexity cost. Fetching arbitrary URLs server-side is an SSRF risk and adds network, timeout, and
caching concerns; reading arbitrary filesystem paths is a path-traversal risk. Separately, the renderer
generates Typst source as a string and already embeds QR codes inline as `#image(bytes("<svg text>"))`,
which works only because SVG is text; raw PNG/JPEG bytes cannot be embedded safely in a source string.

## Decision

- **Static source:** a `src` path resolved under a configured assets root (`LABELER_ASSETS_DIR`,
  default `assets/`), with a path-traversal guard (canonicalize both the root and the candidate, reject
  if the candidate escapes the root).
- **Data-bound source:** a `name` data key whose value is a base64 data URI
  (`data:<mime>;base64,...`). The MIME carries the format; bare base64 is not accepted.
- **No server-side URL fetching in Phase 1.** A URL source can be added later as a third, additive
  option once the SSRF/network design is done; the caller or an integration fetches bytes for now.
- **Typst delivery:** decode to bytes in Rust and register them as an in-memory virtual file via
  `TypstEngine::builder().with_static_file_resolver(...)` (verified in typst-as-lib 0.15.0), referenced
  by an absolute virtual path in the source. A single image collector is shared across the whole render
  so virtual paths stay unique across sheet labels and nested containers.
- **Errors:** reuse existing codes — `MissingField` for an absent data key, `UnsupportedLayoutItem` for
  bad base64, unsupported format, or asset path problems. No new error code.

## Consequences

- No SSRF or arbitrary-filesystem surface in Phase 1: data-bound images touch neither network nor disk,
  and static reads are confined to one configured directory.
- Integrations that have a remote image URL (e.g. Homebox photos) must fetch and inline it as base64
  until URL fetching lands; this keeps the issue self-contained.
- Binary image support depends on the typst-as-lib static-file-resolver API; if that changes, the
  delivery mechanism must be revisited.
- The assets root is process-global config; per-template asset roots are out of scope.
