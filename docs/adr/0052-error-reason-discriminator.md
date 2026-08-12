# 52. A `details.reason` discriminator for `AppError`

Date: 2026-08-12

## Status

Accepted. Issue [#151](https://github.com/pfa230/labeler/issues/151). Refines the error contract of
SPEC §10; supersedes nothing.

## Context

Every error the API returns is `{ "error": { code, message, details } }`, and `code` is the only
machine-readable part of it. Four codes each carry a pile of unrelated causes:

| code | call sites | causes it flattens |
| --- | --- | --- |
| `RenderFailed` | 40 | Typst compile, PNG/PDF encode, font read/parse, template file writes, QR generation, zip, internal invariants |
| `InvalidRequest` | 39 | malformed JSON, bad path params, CSV header and row problems, credential rules, `start_slot`, connectors, id rules, variable keys, settings, datetime formats, render format/colour/resolution, copies, import mode |
| `UnsupportedLayoutItem` | 20 | image asset and data problems, unsupported formats, QR error-correction, size constraints, **and** coordinate, bounds and degeneracy failures |
| `TemplateInvalid` | 3 | parse, structural validation, and duplicate ids, behind one `_` match arm |

A client cannot tell "your template's coordinates are wrong" from "that image file is missing", and
on `RenderFailed` cannot tell a broken font from a full disk. The ambiguity predates #146/#147, which
only added five more geometry cases to a code that already mixed families.

The evidence that this hurts is in our own tests: three had already given up on `code` and were
matching prose instead (`.contains("outside the frame")`, `.contains("must differ")`,
`.contains("above and to the right")`). That couples the suite to wording, and lets a test pass
against the wrong failure whose phrasing happens to overlap. One of them turned out to be doing
exactly that: the "outside the frame" assertion was reached by the line-endpoint check, not the
coordinate check it appeared to describe.

## Decision

**1. A required `Reason` parameter, not an optional setter.** Each of the four constructors takes a
`Reason` as its first argument:

```rust
AppError::render_failed(Reason::TypstCompileFailed, format!("typst compile failed: {err}"))
```

The compiler then makes it impossible to add a call site without classifying it. An optional
`.with_reason()` could not: nothing would make a new call site use it, and the field would rot.

**2. One flat enum, not one per code.** A single `Reason` in `src/reason.rs` with
`as_slug(&self) -> &'static str`. Per-code enums would prevent pairing a render reason with an
`InvalidRequest`, but that is a mistake a reviewer catches trivially, and four parallel types is
ceremony for it. A declarative macro generates the enum, `as_slug`, and `ALL` from one table, so a
variant cannot be added without appearing in `ALL` — which is what makes the completeness test real
rather than decorative.

**3. The slug lands in `details.reason`, and the contract is scoped.** Only `RenderFailed`,
`InvalidRequest`, `UnsupportedLayoutItem` and `TemplateInvalid` carry a `reason`. `details` stays
`Option<Value>` in Rust and keeps `skip_serializing_if` on the wire, so it remains optional in JSON
and in the generated OpenAPI schema. Promising a global "`details` always has `reason`" would be
false the moment an un-migrated code returns, and making `details` required would be a schema change
for generated clients rather than the additive change this is meant to be.

**4. Per-label batch failures carry it too.** `/batch` reports failures as
`{ index, code, message }`. A top-level reason on `BatchInvalid` would only say "some labels failed",
leaving the nested `UnsupportedLayoutItem`s prose-discriminated — exactly the case #151 is about,
since geometry failures are most likely to surface per-label in a sheet run. `BatchFailure.reason` is
**optional**, because a per-label failure can carry a code outside the migrated four (`MissingField`
and `InvalidOptionValue` both reach that path), and a required field there would contradict the
scoping in decision 3.

**5. `AppError::new` is private.** "The compiler makes it impossible" only holds if there is no way
round the constructors. `new(status, code, msg, details)` was public and took an arbitrary code
string, and two paths used it directly — malformed JSON bodies, and connector errors. Both now go
through constructors (connector errors via `From<ConnectorError>`), and `new` is module-private.

**6. Merging is by remediation, not by message.** One reason per thing the caller must fix, not one
per validation predicate: the two checks that reject a `resolution` share `ResolutionInvalid`, since
either way the caller edits one field. Causes stay apart when the *fix* differs — a bad value versus
an inapplicable parameter, one field versus two that disagree, an absent asset versus a refused path
traversal, a bad request versus bad server state. This yields 69 reasons across 105 classifications.

## Consequences

- Slugs are API. A test pins the enum against the SPEC §10.1 table in both directions, so a rename
  fails loudly instead of silently changing what clients switch on, and the table cannot drift from
  the code.
- Adding a call site to any of the four codes now forces a classification decision. That is the
  point, and it is the cost.
- Prose stays prose. Messages are unchanged and are no longer load-bearing; tests that assert wording
  a user actually reads may keep doing so, but nothing machine-readable depends on it.
- Nothing in `ui/` consumes `reason` yet. Inventing a consumer before there is a need would be
  speculative; the field exists so that one can be written without a protocol change.

## Alternatives considered

**Splitting `RenderFailed` into six codes.** Rejected: `code` is bound to the HTTP status and is the
coarse contract clients already switch on, so this breaks anything matching it today. `details` is
additive — a client that ignores it sees no change.

**An optional `.with_reason()` setter.** Rejected as unenforceable, per decision 1.

**Deriving the slug from the enum variant name.** Rejected: it makes every rename a silent wire
change. The slug is written out beside the variant precisely so that renaming one does not move the
other.
