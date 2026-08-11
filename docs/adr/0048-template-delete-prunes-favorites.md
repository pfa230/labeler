# 48. Deleting a template prunes favorites, not recents

Date: 2026-08-11

## Status

Accepted. Issue [#140](https://github.com/pfa230/labeler/issues/140).

## Context

`DELETE /templates/{id}` has existed since #10: it unlinks the template's YAML and reloads the
registry. Two personalisation surfaces reference templates by id and outlive them.

**Favorites** live in a `favorites (user_id, template_id)` table. `GET /favorites` filters its rows
against the live registry, so a favorited-then-deleted template simply stops appearing and the stale
row is invisible — until someone creates a new template reusing that id. The filter then passes the
old row and the favorite reappears, attached to a template the user never favorited. Ids are short
and descriptive (`brother_12mm`, `avery5163`), so reuse after a delete is a realistic way to replace
a template, not an exotic case.

**Recents** are not stored: `GET /recent-templates` derives them from the `jobs` table, grouping the
print log by template. The same registry filter hides a deleted id there too.

The delete path also assumed a template's file is `{id}.yaml`, while the registry loads any
`*.yaml`/`*.yml` and keys on the `id` inside the file. That assumption is what made this decision
urgent rather than theoretical: a `.yml`-backed template could not be deleted at all.

## Decision

**Deleting a template prunes every user's favorite row for that id**, not just the caller's:
favorites are keyed by actor, and the template's removal invalidates the row for everyone.

**Recents are not pruned.** They are print history. #94 exists to surface the job log as a viewable
audit trail, and deleting a template must not erase the record that labels were printed from it. The
existing registry filter already keeps the deleted id out of the endpoint's response, which is the
only user-visible surface.

**Order inside the write lock: resolve, unlink, prune, reload.** The unlink comes first because it is
the step that realistically fails (permissions, a read-only mount); if it fails, favorites are
untouched and nothing has changed. Pruning first would mean a failed unlink had already destroyed
favorites for a template that still exists. The prune comes *before* the reload because `reload()`
re-reads the whole directory and can fail for reasons unrelated to this delete — an invalid sibling
file, which `reload_invalid_file_keeps_previous_set` already exercises — and a prune after it would
be skipped by that unrelated failure, leaving exactly the stale row this decision removes.

**`PUT /favorites/{id}` validates under the write lock.** It previously checked the registry before
acquiring the lock, so a favorite could pass the check, block behind an in-flight delete, and insert
its row after the prune had run.

**The registry records the file each id was loaded from**, and `GET /templates/{id}/source`, `PUT`
and `DELETE` resolve through it. `load_from_dir` already computed this map for its duplicate-id
check and discarded it. Resolution deliberately does not fall back to guessing `{id}.yaml` or
`{id}.yml` on disk: `y1.yml` may declare `id: other`, and a filename fallback would let
`DELETE /templates/y1` unlink another template's file. `POST /templates` still writes new templates
as `{id}.yaml`; its duplicate guard checks the registry as well as that path, since a guard can only
refuse, never delete.

## Consequences

- Re-creating a template under a deleted id starts with no favorites. That is the point, but it means
  an owner who deletes and immediately re-uploads a corrected version loses the favorites on it. `PUT`
  is the non-destructive path for that and is unaffected.
- The delete is not atomic across the filesystem and the database. If the prune fails (a SQLite-level
  error, so a broken database), the caller gets a `500` for a template that is in fact deleted, and
  the orphaned row stays hidden behind the read-side filter — the pre-#140 state, no worse.
- The read-side registry filter on `GET /favorites` stays. It now covers only rows orphaned some other
  way (a template removed from disk by hand, a reload that dropped an id), not the ordinary delete.
- Templates whose filename differs from their id are now fully manageable through the API rather than
  half-visible: listable and renderable but not editable or deletable. The `template_ids_and_filenames`
  convention still holds for the catalog, enforced by its own gate; it is no longer load-bearing for
  the API.
- Deleting a template that a `jobs` row references leaves that row intact and its template id
  dangling. That is deliberate — an audit log that rewrites itself when the subject is deleted is not
  an audit log — and #94 will have to render such ids as historical rather than resolvable.
