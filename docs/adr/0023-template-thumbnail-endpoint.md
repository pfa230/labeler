# 23. Template thumbnail endpoint

## Status

Accepted. Implements milestone M9 ([#73](https://github.com/pfa230/labeler/issues/73)).

## Context

The template list and detail pages have no layout preview. A card shows the template id and format but
gives no visual sense of what the label looks like. The absence is especially noticeable for sheet
templates (e.g. Avery 5163), where the format is otherwise opaque without opening the YAML.

Two constraints shape the design. First, card-format templates carry no layout tree; only the server
has the full `TemplateDefinition` in memory after parsing. Attempting to render from the browser would
require either shipping the full Typst toolchain to the client or duplicating the render logic. Second,
sheet templates must not render a full sheet for a thumbnail: a thumbnail that shows 30 tiny labels on
a letter-sized page is not useful. The preview must be one label slot.

Templates can also change at runtime (via `PUT /templates/{id}` or `POST /templates/reload`), so any
caching strategy must survive mutations without serving stale images.

## Decision

Expose `GET /templates/{id}/thumbnail` on the server. The handler:

1. Fills every text and QR field with its field name as placeholder data, via `placeholder_data()`.
2. Selects the default option (first allowed value per key) via `default_option_selection()`.
3. Resolves `{vars.X}` tokens from the variable store via `store.all_variables()`.
4. Calls `render_thumbnail_png()`, which renders a single label slot regardless of template format
   (single or sheet). For sheet templates this clips one slot from the sheet render.
5. Returns `200 image/png` with `ETag: "<sha256-of-template-yaml>"` and `Cache-Control: no-cache`.
   Clients that send `If-None-Match` with a matching ETag receive `304 Not Modified` with no body.

The ETag is the SHA-256 of the stored YAML, computed once at load time per `content_hash()` (ADR from
Task 1 of M9). `Cache-Control: no-cache` means browsers and proxies always revalidate, but a 304
response is free if the template has not changed.

**Rejected alternatives:**

- **Client-side render from the detail endpoint.** The full template definition is already available at
  `GET /templates/{id}`, but rendering Typst in the browser is not practical without WASM bindings, and
  duplicating the layout math in JavaScript would create a persistent consistency hazard.
- **A `sample:` schema in the template YAML.** Authors could embed sample data per-field and the server
  would use that for the preview. This increases template authoring burden and still requires the same
  server render path. Field names are a useful zero-config fallback that communicates the data schema
  directly in the preview.

## Consequences

- Template list and detail pages can show a live PNG preview by fetching the thumbnail URL.
- Thumbnail renders use Typst synchronously on the request thread, consistent with all other render
  paths in this service.
- A template mutation (PUT or reload) changes the YAML, so the ETag changes, and clients that
  revalidate immediately get a fresh render. Clients that cache a response beyond the revalidation
  cycle see a stale image only until their next request.
- Undefined `{vars.X}` references in placeholder data cause a `422`, consistent with the render
  endpoint. Authors should either define variables in the store or avoid `{vars.X}` in fields that
  appear in the placeholder render.
