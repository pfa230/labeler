# 6. Template edit ownership: manual vs GUI

**Status:** Accepted

## Context

Templates are authored two ways: hand-written YAML (git-friendly, comment-rich, the artifact users
version and share) and, once built, a WYSIWYG GUI editor (ADR pending for the editor stack). The GUI
keeps the YAML layout tree as its source of truth and serializes back to YAML on save. That round-trip
is lossy: standard YAML emitters strip comments and normalize key order, so a GUI save would silently
destroy a hand-authored template's comments and formatting. The GUI also needs design-time-only data
(sample/preview values for bound fields) that must not pollute the canonical template the renderer
consumes. Allowing both editors to write the same file makes the file's content depend on which tool
touched it last, which is the drift the source-of-truth principle (see the editor design) exists to
prevent.

## Decision

Make edit ownership a property of each template; never allow two editors to write the same file.

- A template is either **manual** (file-authored; the GUI may render, print, and preview it but not
  edit it) or **gui** (GUI-owned; the GUI is its only editor).
- Ownership is determined by location: manual templates live in `templates/`, GUI-owned templates in a
  separate writable store (`templates-gui/`). Both classes are YAML files in the same schema and feed
  one registry. For v1 the substrate is folders, not a database; app state (printers, settings, jobs)
  is the thing that warrants SQLite, not templates.
- A one-way **"Convert to GUI edit"** action *moves* a manual template into the GUI store and marks it
  GUI-owned. It is a move, not a copy, so there is never a duplicate id or two diverging sources. The
  reverse direction is a plain "export YAML" download; re-adopting it as manual is a manual file drop,
  not an automatic round-trip.
- Design-time-only data lives in an optional top-level `editor` block in the YAML, owned by the GUI and
  ignored by the renderer. Because the renderer's parser uses `deny_unknown_fields`, this block must be
  added to the schema explicitly. It only ever appears on GUI-owned templates.

## Consequences

- The lossy-round-trip problem disappears by construction: the GUI never rewrites a manually-owned
  file, so comments and formatting in `templates/` are safe.
- Both classes render and print identically; ownership gates editing only. The UI shows manual
  templates with a lock badge and the "Convert to GUI edit" action.
- The registry must scan both stores and still reject duplicate ids across them; the move semantics
  keep this from happening in normal use.
- Adding the `editor` block touches the schema (`raw.rs`, `models.rs`, the `TryFrom`) per ADR-0002,
  and the renderer must skip it.
- Converting is intentionally one-way and slightly inconvenient in reverse; this is the cost of keeping
  a single source of edits.
